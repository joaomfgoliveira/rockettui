//! The thread pane (`message-threads`): one open thread of the open room, read only.
//!
//! The pane holds the parent message first and then its replies oldest first, with its own cursor
//! and scroll. A reply lives only here (`app::realtime` keeps it out of the room's main flow), so
//! the pane is rebuilt from the server each time a thread is opened. Everything is addressed by
//! room and parent id, so an answer for a thread that is no longer open is dropped.

use crate::api::{ApiError, ThreadPage};
use crate::app::messages::{self, MsgView};
use crate::app::{App, DraftKey, InputTarget, StatusMsg, edit};
use crate::data::{Message, MessageKind, RoomId, SendState};
use crate::effect::Effect;

/// How many replies one request asks for.
pub const PAGE: u16 = 50;

/// Where the pane's load stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadLoad {
    /// The first request is in flight.
    Loading,
    Loaded,
    /// The first request failed; only the class of the failure is kept.
    Failed(ApiError),
}

/// The open thread. No `Debug`: it holds message text.
pub struct ThreadPane {
    pub room: RoomId,
    /// The id of the thread's parent message.
    pub parent_id: String,
    /// The main-flow message the thread was opened from: closing puts the cursor back on it.
    pub origin: String,
    /// The parent first (once known), then the replies oldest first.
    pub messages: Vec<Message>,
    pub view: MsgView,
    pub load: ThreadLoad,
    /// How many replies the server says the thread has.
    pub total: u32,
    /// The offset of the next page of replies: what the requests have brought so far. Live replies
    /// do not count, so offset paging stays aligned with the server.
    pub next_offset: u32,
    /// A further page is in flight.
    pub loading_more: bool,
    /// Opened from the thread list: closing the pane shows the list again.
    pub from_list: bool,
}

impl ThreadPane {
    /// The index in `messages` of the first reply (the parent, when held, is before it).
    fn replies_start(&self) -> usize {
        usize::from(
            self.messages
                .first()
                .is_some_and(|m| m.id == self.parent_id),
        )
    }

    /// Hold `message` once: a held id is kept as it is, a new one goes in by time after the parent
    /// and after every reply that is not newer.
    pub fn merge(&mut self, message: Message) {
        if self.messages.iter().any(|m| m.id == message.id) {
            return;
        }
        if message.id == self.parent_id {
            self.messages.insert(0, message);
            return;
        }
        let from = self.replies_start();
        let at = self.messages[from..]
            .iter()
            .position(|m| m.ts > message.ts)
            .map_or(self.messages.len(), |i| from + i);
        self.messages.insert(at, message);
    }
}

/// The thread the message under the cursor belongs to: a reply (shown in the channel) names its
/// parent, a message with replies is its own parent, any other has none.
fn target(message: &Message) -> Option<String> {
    match &message.tmid {
        Some(parent) => Some(parent.clone()),
        None => (message.tcount > 0).then(|| message.id.clone()),
    }
}

/// The id of the parent of the thread the message under the cursor belongs to, when the messages
/// pane is focused on one: what `open_thread` acts on, and what the context popup checks before it
/// lists the action. `None` for any other pane or a message with no thread.
pub fn under_cursor(app: &App) -> Option<String> {
    use crate::app::{Focus, Mode};
    if app.mode != Mode::Normal || app.focus != Focus::Messages {
        return None;
    }
    let room = app.current_room.as_ref()?;
    target(
        app.messages
            .get(room)?
            .get(messages::view(app, room).cursor)?,
    )
}

/// `open_thread`: open the thread of the message under the cursor of the open room, replacing any
/// open one, and ask the server for its replies. A message with no thread says so on the status
/// line.
pub fn open(app: &mut App) {
    use crate::app::{Focus, Mode};
    if app.mode != Mode::Normal || app.focus != Focus::Messages {
        return;
    }
    let Some(room) = app.current_room.clone() else {
        return;
    };
    let Some(held) = app.messages.get(&room) else {
        return;
    };
    let cursor = messages::view(app, &room).cursor;
    let Some(message) = held.get(cursor) else {
        return;
    };
    let Some(parent_id) = target(message) else {
        app.status_msg = Some(StatusMsg::Info("No thread on this message".to_string()));
        return;
    };
    let origin = message.id.clone();
    open_at(app, room, parent_id, origin, true, None, false);
}

/// Open the thread `parent_id` chosen in the thread list. `fetched` is the parent when the list had
/// to fetch it. The main flow is not touched: closing the pane puts the cursor back on the message
/// it is on now, or leaves it when that is not held.
pub fn open_from_list(app: &mut App, room: RoomId, parent_id: String, fetched: Option<Message>) {
    let origin = app
        .messages
        .get(&room)
        .and_then(|held| held.get(messages::view(app, &room).cursor))
        .map_or_else(|| parent_id.clone(), |m| m.id.clone());
    open_at(app, room, parent_id, origin, true, fetched, true);
}

/// Open the pane on thread `parent_id` of `room`, replacing any open one. The parent is taken from
/// the main flow when it is held and fetched otherwise. With `fetch_replies` the replies are asked
/// for; without it the thread is known to be empty (a message about to get its first reply), so the
/// pane is `Loaded` at once. Replies not yet sent come back from the outbox.
fn open_at(
    app: &mut App,
    room: RoomId,
    parent_id: String,
    origin: String,
    fetch_replies: bool,
    fetched: Option<Message>,
    from_list: bool,
) {
    retarget(app, InputTarget::Room);
    if let Some(old) = app.thread.take() {
        keep_unsent(app, &old);
    }
    // A thread opened any other way than from the list takes the column from the list for good.
    if !from_list {
        app.thread_list = None;
    }
    let parent = app
        .messages
        .get(&room)
        .and_then(|held| held.iter().find(|m| m.id == parent_id))
        .cloned()
        .or(fetched);
    let fetch_parent = parent.is_none();
    let mut pending = Vec::new();
    app.outbox.retain(|m| {
        let mine = m.room_id == room && m.tmid.as_deref() == Some(parent_id.as_str());
        if mine {
            pending.push(m.clone());
        }
        !mine
    });
    let request = fetch_replies || fetch_parent;
    app.thread = Some(ThreadPane {
        room: room.clone(),
        parent_id: parent_id.clone(),
        origin,
        messages: parent.into_iter().chain(pending).collect(),
        view: MsgView::default(),
        load: if request {
            ThreadLoad::Loading
        } else {
            ThreadLoad::Loaded
        },
        total: 0,
        next_offset: 0,
        loading_more: false,
        from_list,
    });
    if request {
        app.effects.push(Effect::LoadThread {
            room,
            parent_id,
            offset: 0,
            count: PAGE,
            fetch_parent,
        });
    }
    app.focus = crate::app::Focus::Thread;
    // An open thread is being read: the wait starts (it sends nothing for a thread that is not unread).
    super::read::thread_restart(app);
}

/// True when the message under the cursor of the focused messages pane can be replied to: what the
/// context popup checks before it lists the action.
pub fn replyable_under_cursor(app: &App) -> bool {
    use crate::app::{Focus, Mode};
    if app.mode != Mode::Normal || app.focus != Focus::Messages {
        return false;
    }
    let Some(room) = app.current_room.as_ref() else {
        return false;
    };
    app.messages
        .get(room)
        .and_then(|held| held.get(messages::view(app, room).cursor))
        .is_some_and(|m| m.kind == MessageKind::Normal && !m.send.is_unsent())
}

/// `reply_in_thread`: start a reply to the message under the cursor. From the messages pane the
/// thread of that message opens (empty when it has no replies yet, and the first reply creates it);
/// on a reply shown in the channel it is the reply's parent. From the thread pane it is the open
/// thread. A system message, a deleted one and an encrypted room refuse on the status line.
pub fn start_reply(app: &mut App) {
    use crate::app::{Focus, Mode};
    if app.mode != Mode::Normal {
        return;
    }
    let Some(room) = app.current_room.clone() else {
        return;
    };
    let parent_id = match app.focus {
        Focus::Thread => {
            let Some(pane) = app.thread.as_ref() else {
                return;
            };
            pane.parent_id.clone()
        }
        Focus::Messages => {
            let Some(message) = app
                .messages
                .get(&room)
                .and_then(|held| held.get(messages::view(app, &room).cursor))
            else {
                return;
            };
            if message.kind != MessageKind::Normal || message.send.is_unsent() {
                app.status_msg = Some(StatusMsg::Info(
                    "This message cannot be replied to".to_string(),
                ));
                return;
            }
            let parent_id = message.tmid.clone().unwrap_or_else(|| message.id.clone());
            if app.rooms.iter().any(|r| r.id == room && r.encrypted) {
                app.status_msg = Some(StatusMsg::Error("encrypted, not supported".into()));
                return;
            }
            let has_replies = message.tcount > 0 || message.tmid.is_some();
            let origin = message.id.clone();
            let already_open = app
                .thread
                .as_ref()
                .is_some_and(|p| p.room == room && p.parent_id == parent_id);
            if !already_open {
                open_at(
                    app,
                    room.clone(),
                    parent_id.clone(),
                    origin,
                    has_replies,
                    None,
                    false,
                );
            }
            parent_id
        }
        _ => return,
    };
    if app.rooms.iter().any(|r| r.id == room && r.encrypted) {
        app.status_msg = Some(StatusMsg::Error("encrypted, not supported".into()));
        return;
    }
    retarget(app, InputTarget::thread(parent_id));
    app.prev_focus = Focus::Thread;
    app.focus = Focus::Input;
    app.mode = Mode::Insert;
}

/// `toggle_reply_to_channel`: in Insert mode with a thread as the target, flip whether the reply is
/// also shown in the channel. Anything else changes nothing.
pub fn toggle_also_in_channel(app: &mut App) {
    if app.mode != crate::app::Mode::Insert {
        return;
    }
    if let InputTarget::Thread {
        also_in_channel, ..
    } = &mut app.input_target
    {
        *also_in_channel = !*also_in_channel;
    }
}

/// `close_thread`: close the pane and put the main flow's cursor on the message it was opened from
/// (when that message is held). Nothing happens when no thread is open.
pub fn close(app: &mut App) {
    if app.thread.is_none() {
        return;
    }
    retarget(app, InputTarget::Room);
    let Some(pane) = app.thread.take() else {
        return;
    };
    super::read::thread_cancel(app);
    keep_unsent(app, &pane);
    // A thread opened from the list gives the column back to it, as it was.
    let back_to_list = pane.from_list && app.thread_list.is_some();
    if app.focus == crate::app::Focus::Thread {
        app.focus = if back_to_list {
            crate::app::Focus::ThreadList
        } else {
            crate::app::Focus::Messages
        };
    }
    if back_to_list {
        return;
    }
    if let Some(index) = app
        .messages
        .get(&pane.room)
        .and_then(|held| held.iter().position(|m| m.id == pane.origin))
    {
        let top_row = messages::view(app, &pane.room).top_row;
        app.msg_views.insert(
            pane.room.clone(),
            MsgView {
                cursor: index,
                top_row,
            },
        );
    }
}

/// Move the replies of a pane that is going away and are not `Sent` to the outbox, so they still go
/// out and come back when their thread is reopened.
fn keep_unsent(app: &mut App, pane: &ThreadPane) {
    app.outbox
        .extend(pane.messages.iter().filter(|m| m.send.is_unsent()).cloned());
}

/// A reply of `room` to `parent_id` was accepted by the server. The first reply of a thread raises
/// the parent's count from 0 to 1 in the main flow, so the marker row shows at once; the server's
/// own update of the parent, which follows, is the value that stays.
pub fn on_reply_sent(app: &mut App, room: &RoomId, parent_id: &str, reply: &Message) {
    if let Some(parent) = app
        .messages
        .get_mut(room)
        .and_then(|held| held.iter_mut().find(|m| m.id == parent_id))
        .filter(|m| m.tcount == 0)
    {
        parent.tcount = 1;
        parent.tlm = Some(reply.ts);
    }
    if let Some(parent) = app
        .thread
        .as_mut()
        .filter(|p| &p.room == room && p.parent_id == parent_id)
        .and_then(|p| p.messages.iter_mut().find(|m| m.id == parent_id))
        .filter(|m| m.tcount == 0)
    {
        parent.tcount = 1;
        parent.tlm = Some(reply.ts);
    }
}

/// Drop the pane without touching the main flow: the room it belonged to is no longer open.
pub fn discard(app: &mut App) {
    super::read::thread_cancel(app);
    retarget(app, InputTarget::Room);
    if let Some(pane) = app.thread.take() {
        keep_unsent(app, &pane);
    }
    if app.focus == crate::app::Focus::Thread {
        app.focus = crate::app::Focus::Messages;
    }
}

/// Point the input bar at `target`, keeping every draft: the live text goes back in `drafts` under
/// its own key (unless it is empty) and the new target's draft, empty when it has none, becomes the
/// live text. Nothing happens when the target is already the one in use, so a repeated `i` keeps
/// the also-in-channel state. An edit in progress ends first, so the text that moves is a draft and
/// never the text being edited.
pub fn retarget(app: &mut App, target: InputTarget) {
    if app.input_target.parent_id() == target.parent_id() {
        return;
    }
    edit::cancel(app);
    if let Some(room) = app.current_room.clone() {
        let old = DraftKey {
            room: room.clone(),
            thread: app.input_target.parent_id().map(str::to_string),
        };
        let new = DraftKey {
            room,
            thread: target.parent_id().map(str::to_string),
        };
        let live = std::mem::take(&mut app.input);
        if !live.is_empty() {
            app.drafts.insert(old, live);
        }
        app.input = app.drafts.remove(&new).unwrap_or_default();
        app.settle_input();
    }
    app.input_target = target;
}

/// The input target an Insert-mode start from the focused pane means: the open thread when the pane
/// is focused, the room from any other pane, and the target already in use when the input bar
/// itself is focused.
pub fn target_for_focus(app: &App) -> InputTarget {
    use crate::app::Focus;
    match (&app.thread, app.focus) {
        (Some(pane), Focus::Thread) => InputTarget::thread(pane.parent_id.clone()),
        (_, Focus::Input) => app.input_target.clone(),
        _ => InputTarget::Room,
    }
}

/// The message under the cursor of the thread pane while the pane is focused in `room`.
pub fn under_pane_cursor<'a>(app: &'a App, room: &RoomId) -> Option<&'a Message> {
    if app.focus != crate::app::Focus::Thread {
        return None;
    }
    let pane = app.thread.as_ref().filter(|p| &p.room == room)?;
    pane.messages.get(view(app).cursor)
}

/// The held message `id` of `room`: in the main flow, or else in the open thread pane. What edit,
/// delete and react look a message up with, so they work on the pane's messages too.
pub fn held_message<'a>(app: &'a App, room: &RoomId, id: &str) -> Option<&'a Message> {
    app.messages
        .get(room)
        .and_then(|held| held.iter().find(|m| m.id == id))
        .or_else(|| {
            app.thread
                .as_ref()
                .filter(|p| &p.room == room)
                .and_then(|p| p.messages.iter().find(|m| m.id == id))
        })
}

/// The open pane, when it is the thread `parent_id` of `room`.
fn pane_for<'a>(app: &'a mut App, room: &RoomId, parent_id: &str) -> Option<&'a mut ThreadPane> {
    app.thread
        .as_mut()
        .filter(|pane| &pane.room == room && pane.parent_id == parent_id)
}

/// A page of replies arrived. `offset` 0 is the first page (with the parent when it had to be
/// fetched). Replies already held, such as one that came live meanwhile, are not duplicated.
pub fn on_loaded(
    app: &mut App,
    room: &RoomId,
    parent_id: &str,
    offset: u32,
    parent: Option<Message>,
    page: ThreadPage,
) {
    let Some(pane) = pane_for(app, room, parent_id) else {
        return;
    };
    if let Some(parent) = parent {
        pane.merge(parent);
    }
    let received = u32::try_from(page.messages.len()).unwrap_or(u32::MAX);
    for message in page.messages {
        pane.merge(message);
    }
    pane.total = page.total;
    pane.next_offset = offset.saturating_add(received);
    pane.loading_more = false;
    pane.load = ThreadLoad::Loaded;
    if offset == 0 {
        // Opening a thread lands on its newest message, and only now has the user been shown it.
        pane.view.cursor = pane.messages.len().saturating_sub(1);
        super::read::thread_restart(app);
    }
}

/// A request failed. The first page leaves the pane showing the failure class; a later page only
/// ends its request and says so on the status line, keeping what is shown.
pub fn on_failed(app: &mut App, room: &RoomId, parent_id: &str, offset: u32, error: ApiError) {
    let Some(pane) = pane_for(app, room, parent_id) else {
        return;
    };
    if offset == 0 {
        pane.load = ThreadLoad::Failed(error);
    } else {
        pane.loading_more = false;
        app.status_msg = Some(StatusMsg::Error(error.to_string()));
    }
}

/// A live message of the open room reached the pane's thread. A message the pane holds (the parent
/// or a reply) is updated in place when its text, edited state, reactions or reply count changed.
/// A new reply to this thread is added in order, and the pane follows it when its cursor is on the
/// newest message. A message of another thread, or of another room, is none of the pane's business.
pub fn on_live_message(app: &mut App, message: &Message) {
    let Some(pane) = app.thread.as_mut().filter(|p| p.room == message.room_id) else {
        return;
    };
    if let Some(slot) = pane.messages.iter_mut().find(|m| m.id == message.id) {
        if slot.send != SendState::Sent {
            // The server's copy of a reply of ours that is still pending: confirm it in place. A
            // request still in flight keeps the room's send slot until its own answer (D-03).
            let was = slot.send;
            *slot = Message {
                send: SendState::Sent,
                ..message.clone()
            };
            if was == SendState::Sending {
                app.awaiting_answer
                    .insert(message.room_id.clone(), message.id.clone());
            }
        } else if slot.kind != MessageKind::Deleted && messages::changed(slot, message) {
            messages::replace_slot(slot, message.clone());
        }
        return;
    }
    if message.tmid.as_deref() != Some(pane.parent_id.as_str()) {
        return;
    }
    let at_newest = pane.view.cursor + 1 >= pane.messages.len();
    pane.merge(Message {
        send: SendState::Sent,
        ..message.clone()
    });
    if at_newest && pane.load == ThreadLoad::Loaded {
        pane.view.cursor = pane.messages.len() - 1;
    }
}

/// A message of `room` was deleted: the pane's copy becomes a tombstone, as in the main flow.
pub fn on_deleted(app: &mut App, room: &RoomId, id: &str) {
    if let Some(slot) = app
        .thread
        .as_mut()
        .filter(|pane| &pane.room == room)
        .and_then(|pane| pane.messages.iter_mut().find(|m| m.id == id))
        .filter(|m| m.send == SendState::Sent && m.kind != MessageKind::Deleted)
    {
        messages::tombstone_slot(slot);
    }
}

/// Inner (width, height) of the thread pane, borders removed; (0, 0) while no thread is open.
pub fn viewport(app: &App) -> (u16, u16) {
    let pane = app.areas().thread.unwrap_or_default();
    (pane.width.saturating_sub(2), pane.height.saturating_sub(2))
}

/// First row of every message of the pane at the current width, the total last (like
/// `messages::row_starts`). The pane is a list of its own, so its row context has no divider.
pub fn row_starts(app: &App) -> Vec<usize> {
    let (width, _) = viewport(app);
    let msgs = app
        .thread
        .as_ref()
        .map_or(&[][..], |p| p.messages.as_slice());
    messages::row_starts_of(msgs, width, &app.thread_row_ctx(msgs))
}

/// The effective view of the pane: its stored cursor, clamped, and scrolled so the cursor message
/// is inside the window at the current size. Recomputed on every call, so a resize cannot leave the
/// cursor outside the window.
pub fn view(app: &App) -> MsgView {
    let Some(pane) = app.thread.as_ref() else {
        return MsgView::default();
    };
    let starts = row_starts(app);
    let count = starts.len() - 1;
    if count == 0 {
        return MsgView::default();
    }
    let cursor = pane.view.cursor.min(count - 1);
    let viewport_h = usize::from(viewport(app).1);
    MsgView {
        cursor,
        top_row: messages::scroll_into_view(&starts, cursor, pane.view.top_row, viewport_h),
    }
}

/// Apply a Normal-mode motion to the thread pane, with the same rules as the message pane. Moving
/// onto the last held reply asks for the next page when the thread has more.
pub fn apply_motion(app: &mut App, motion: &crate::action::Action, count: Option<u32>) {
    if app.thread.is_none() {
        return;
    }
    let starts = row_starts(app);
    let viewport_h = usize::from(viewport(app).1);
    let current = view(app);
    if let Some(next) = messages::step(&starts, current, viewport_h, motion, count)
        && let Some(pane) = app.thread.as_mut()
    {
        pane.view = next;
        if next.cursor + 1 >= starts.len() - 1 {
            request_more(app);
        }
    }
}

/// Ask for the next page of replies when the thread has more and no request is in flight.
pub fn request_more(app: &mut App) {
    let Some(pane) = app.thread.as_mut() else {
        return;
    };
    if pane.load != ThreadLoad::Loaded || pane.loading_more || pane.next_offset >= pane.total {
        return;
    }
    pane.loading_more = true;
    let effect = Effect::LoadThread {
        room: pane.room.clone(),
        parent_id: pane.parent_id.clone(),
        offset: pane.next_offset,
        count: PAGE,
        fetch_parent: false,
    };
    app.effects.push(effect);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::api::ApiError;
    use crate::api::map;
    use crate::api::wire::{RoomsResp, SubscriptionsResp};
    use crate::app::update::update;
    use crate::app::{Focus, Mode};
    use crate::data::{MessageKind, SendState};
    use crate::test_util::press;
    use ratatui::layout::Size;

    const SUBS: &str = include_str!("../api/fixtures/subscriptions.json");
    const ROOMS: &str = include_str!("../api/fixtures/rooms.json");

    fn rid(name: &str) -> RoomId {
        RoomId(name.to_string())
    }

    fn msg(id: &str, ts: i64) -> Message {
        Message {
            id: id.to_string(),
            room_id: rid("r-general"),
            author: "alice".to_string(),
            ts,
            body: format!("body of {id}"),
            kind: MessageKind::Normal,
            send: SendState::Sent,
            edited: false,
            plain_text: true,
            reactions: Vec::new(),
            tmid: None,
            tshow: false,
            tcount: 0,
            tlm: None,
            followers: None,
        }
    }

    fn parent(id: &str, ts: i64, replies: u32) -> Message {
        Message {
            tcount: replies,
            tlm: Some(ts + 10),
            followers: None,
            ..msg(id, ts)
        }
    }

    fn reply(id: &str, ts: i64, to: &str, tshow: bool) -> Message {
        Message {
            tmid: Some(to.to_string()),
            tshow,
            ..msg(id, ts)
        }
    }

    /// `r-general` open with `held` loaded, focus on the messages pane, cursor on the newest.
    fn app_with(held: Vec<Message>) -> App {
        let subs = serde_json::from_str::<SubscriptionsResp>(SUBS)
            .expect("subscriptions")
            .update;
        let records = serde_json::from_str::<RoomsResp>(ROOMS)
            .expect("rooms")
            .update;
        let mut app = App::new(Size::new(80, 24));
        update(&mut app, Action::RoomsLoaded(map::rooms(&subs, &records)));
        press(&mut app, "<CR>");
        assert_eq!(app.current_room, Some(rid("r-general")));
        update(
            &mut app,
            Action::HistoryLoaded {
                room: rid("r-general"),
                messages: held,
                oldest_ts: None,
                older: false,
            },
        );
        app.effects.clear();
        app.focus = Focus::Messages;
        app.mode = Mode::Normal;
        app
    }

    fn page(ids: &[(&str, i64)], to: &str, total: u32) -> ThreadPage {
        ThreadPage {
            messages: ids
                .iter()
                .map(|(id, ts)| reply(id, *ts, to, false))
                .collect(),
            total,
        }
    }

    fn loaded(
        app: &mut App,
        parent_id: &str,
        offset: u32,
        parent: Option<Message>,
        page: ThreadPage,
    ) {
        update(
            app,
            Action::ThreadLoaded {
                room: rid("r-general"),
                parent_id: parent_id.to_string(),
                offset,
                parent,
                page,
            },
        );
    }

    fn ids(app: &App) -> Vec<&str> {
        app.thread
            .as_ref()
            .map(|p| p.messages.iter().map(|m| m.id.as_str()).collect())
            .unwrap_or_default()
    }

    fn thread_effects(app: &App) -> Vec<&Effect> {
        app.effects
            .iter()
            .filter(|e| matches!(e, Effect::LoadThread { .. }))
            .collect()
    }

    #[test]
    fn opening_a_parent_asks_for_its_replies_and_shows_it_first() {
        let mut app = app_with(vec![msg("a", 100), parent("p", 110, 2)]);
        update(&mut app, Action::OpenThread);
        let pane = app.thread.as_ref().expect("pane open");
        assert_eq!((pane.parent_id.as_str(), pane.origin.as_str()), ("p", "p"));
        assert_eq!(pane.load, ThreadLoad::Loading);
        assert_eq!(ids(&app), ["p"], "the held parent shows while loading");
        assert_eq!(
            thread_effects(&app),
            [&Effect::LoadThread {
                room: rid("r-general"),
                parent_id: "p".to_string(),
                offset: 0,
                count: PAGE,
                fetch_parent: false,
            }]
        );

        loaded(
            &mut app,
            "p",
            0,
            None,
            page(&[("r1", 120), ("r2", 130)], "p", 2),
        );
        assert_eq!(ids(&app), ["p", "r1", "r2"]);
        let pane = app.thread.as_ref().unwrap();
        assert_eq!(
            (pane.load, pane.total, pane.next_offset),
            (ThreadLoad::Loaded, 2, 2)
        );
    }

    #[test]
    fn opening_from_a_channel_visible_reply_opens_the_parent() {
        let mut app = app_with(vec![parent("p", 100, 1), reply("r1", 110, "p", true)]);
        update(&mut app, Action::OpenThread);
        let pane = app.thread.as_ref().expect("pane open");
        assert_eq!((pane.parent_id.as_str(), pane.origin.as_str()), ("p", "r1"));
        assert_eq!(ids(&app), ["p"]);
    }

    #[test]
    fn a_parent_the_main_flow_lacks_is_fetched_with_the_replies() {
        let mut app = app_with(vec![reply("r9", 500, "old-parent", true)]);
        update(&mut app, Action::OpenThread);
        assert_eq!(ids(&app), Vec::<&str>::new());
        assert!(matches!(
            thread_effects(&app)[..],
            [Effect::LoadThread {
                fetch_parent: true,
                ..
            }]
        ));
        loaded(
            &mut app,
            "old-parent",
            0,
            Some(parent("old-parent", 100, 1)),
            page(&[("r9", 500)], "old-parent", 1),
        );
        assert_eq!(ids(&app), ["old-parent", "r9"]);
    }

    #[test]
    fn a_message_with_no_thread_opens_nothing_and_says_so() {
        let mut app = app_with(vec![msg("a", 100)]);
        update(&mut app, Action::OpenThread);
        assert!(app.thread.is_none());
        assert!(app.effects.is_empty());
        assert!(matches!(&app.status_msg, Some(StatusMsg::Info(t)) if t.contains("No thread")));
    }

    #[test]
    fn opening_needs_the_messages_pane_in_normal_mode() {
        let mut app = app_with(vec![parent("p", 100, 1)]);
        app.focus = Focus::Sidebar;
        update(&mut app, Action::OpenThread);
        assert!(app.thread.is_none());
        app.focus = Focus::Messages;
        app.mode = Mode::Insert;
        update(&mut app, Action::OpenThread);
        assert!(app.thread.is_none());
    }

    #[test]
    fn opening_another_thread_replaces_the_first() {
        let mut app = app_with(vec![parent("p1", 100, 1), parent("p2", 110, 1)]);
        update(&mut app, Action::OpenThread);
        assert_eq!(app.thread.as_ref().unwrap().parent_id, "p2");
        loaded(&mut app, "p2", 0, None, page(&[("x", 120)], "p2", 1));
        press(&mut app, "<C-w>h");
        press(&mut app, "k");
        update(&mut app, Action::OpenThread);
        let pane = app.thread.as_ref().unwrap();
        assert_eq!(pane.parent_id, "p1");
        assert_eq!(ids(&app), ["p1"], "nothing of the first thread is kept");
        // The late answer of the replaced thread is dropped.
        loaded(&mut app, "p2", 0, None, page(&[("y", 130)], "p2", 1));
        assert_eq!(ids(&app), ["p1"]);
    }

    #[test]
    fn closing_puts_the_main_cursor_on_the_opened_message() {
        let mut app = app_with(vec![
            parent("p", 100, 1),
            msg("a", 110),
            msg("b", 120),
            msg("c", 130),
        ]);
        press(&mut app, "3k");
        update(&mut app, Action::OpenThread);
        // The user went back to the messages and moved the main cursor while the pane was open.
        press(&mut app, "<C-w>h");
        press(&mut app, "G");
        assert_eq!(messages::view(&app, &rid("r-general")).cursor, 3);
        update(&mut app, Action::CloseThread);
        assert!(app.thread.is_none());
        assert_eq!(messages::view(&app, &rid("r-general")).cursor, 0);
        // Closing with nothing open does nothing.
        update(&mut app, Action::CloseThread);
        assert!(app.thread.is_none());
    }

    #[test]
    fn a_room_change_closes_the_pane() {
        let mut app = app_with(vec![parent("p", 100, 1)]);
        update(&mut app, Action::OpenThread);
        assert!(app.thread.is_some());
        let other = app
            .rooms
            .iter()
            .map(|r| r.id.clone())
            .find(|id| *id != rid("r-general"))
            .expect("another room");
        app.open_room(other);
        assert!(app.thread.is_none());
    }

    #[test]
    fn a_failed_first_page_shows_the_class_and_keeps_the_app_going() {
        let mut app = app_with(vec![parent("p", 100, 1)]);
        update(&mut app, Action::OpenThread);
        update(
            &mut app,
            Action::ThreadLoadFailed {
                room: rid("r-general"),
                parent_id: "p".to_string(),
                offset: 0,
                error: ApiError::Rejected,
            },
        );
        assert_eq!(
            app.thread.as_ref().unwrap().load,
            ThreadLoad::Failed(ApiError::Rejected)
        );
        // A failure for a thread that is not open changes nothing.
        update(
            &mut app,
            Action::ThreadLoadFailed {
                room: rid("r-general"),
                parent_id: "other".to_string(),
                offset: 0,
                error: ApiError::NotFound,
            },
        );
        assert_eq!(
            app.thread.as_ref().unwrap().load,
            ThreadLoad::Failed(ApiError::Rejected)
        );
    }

    #[test]
    fn a_failed_later_page_keeps_the_replies_and_allows_another_try() {
        let mut app = app_with(vec![parent("p", 100, 90)]);
        update(&mut app, Action::OpenThread);
        loaded(&mut app, "p", 0, None, page(&[("r1", 110)], "p", 90));
        app.effects.clear();
        request_more(&mut app);
        assert!(app.thread.as_ref().unwrap().loading_more);
        update(
            &mut app,
            Action::ThreadLoadFailed {
                room: rid("r-general"),
                parent_id: "p".to_string(),
                offset: 1,
                error: ApiError::Timeout,
            },
        );
        let pane = app.thread.as_ref().unwrap();
        assert_eq!((pane.load, pane.loading_more), (ThreadLoad::Loaded, false));
        assert_eq!(ids(&app), ["p", "r1"]);
        assert!(matches!(app.status_msg, Some(StatusMsg::Error(_))));
    }

    #[test]
    fn further_pages_are_asked_by_offset_once_and_merged_without_duplicates() {
        let mut app = app_with(vec![parent("p", 100, 3)]);
        update(&mut app, Action::OpenThread);
        loaded(
            &mut app,
            "p",
            0,
            None,
            page(&[("r1", 110), ("r2", 120)], "p", 3),
        );
        // A live reply was merged between the two pages.
        app.thread
            .as_mut()
            .unwrap()
            .merge(reply("r3", 130, "p", false));
        app.effects.clear();
        request_more(&mut app);
        request_more(&mut app);
        assert_eq!(
            thread_effects(&app),
            [&Effect::LoadThread {
                room: rid("r-general"),
                parent_id: "p".to_string(),
                offset: 2,
                count: PAGE,
                fetch_parent: false,
            }],
            "one request, at the offset the server pages by"
        );
        loaded(&mut app, "p", 2, None, page(&[("r3", 130)], "p", 3));
        assert_eq!(ids(&app), ["p", "r1", "r2", "r3"]);
        app.effects.clear();
        request_more(&mut app);
        assert!(app.effects.is_empty(), "nothing more to load");
    }

    #[test]
    fn replies_are_ordered_by_time_after_the_parent() {
        let mut app = app_with(vec![parent("p", 100, 3)]);
        update(&mut app, Action::OpenThread);
        // A live reply arrived first; the page then brings the older ones.
        app.thread
            .as_mut()
            .unwrap()
            .merge(reply("late", 150, "p", false));
        loaded(
            &mut app,
            "p",
            0,
            None,
            page(&[("r1", 110), ("r2", 120)], "p", 3),
        );
        assert_eq!(ids(&app), ["p", "r1", "r2", "late"]);
    }

    // ---- live replies, edits, deletes and reactions

    fn live(app: &mut App, message: Message) {
        update(app, Action::RtMessage(message));
    }

    fn main_ids(app: &App) -> Vec<&str> {
        app.messages[&rid("r-general")]
            .iter()
            .map(|m| m.id.as_str())
            .collect()
    }

    /// A thread of parent `p` (2 replies) that is open and loaded, after a main flow `a`, `p`.
    fn open_thread_app() -> App {
        let mut app = app_with(vec![msg("a", 100), parent("p", 110, 2)]);
        update(&mut app, Action::OpenThread);
        loaded(
            &mut app,
            "p",
            0,
            None,
            page(&[("r1", 120), ("r2", 130)], "p", 2),
        );
        app.effects.clear();
        app
    }

    fn pane_message<'a>(app: &'a App, id: &str) -> &'a Message {
        app.thread
            .as_ref()
            .and_then(|p| p.messages.iter().find(|m| m.id == id))
            .expect("held by the pane")
    }

    #[test]
    fn a_reply_to_the_open_thread_joins_the_pane_and_not_the_main_flow() {
        let mut app = open_thread_app();
        live(&mut app, reply("r3", 140, "p", false));
        assert_eq!(ids(&app), ["p", "r1", "r2", "r3"]);
        assert_eq!(main_ids(&app), ["a", "p"]);
    }

    #[test]
    fn a_duplicate_reply_is_held_once() {
        let mut app = open_thread_app();
        live(&mut app, reply("r3", 140, "p", false));
        live(&mut app, reply("r3", 140, "p", false));
        live(&mut app, reply("r1", 120, "p", false));
        assert_eq!(ids(&app), ["p", "r1", "r2", "r3"]);
    }

    #[test]
    fn a_reply_to_a_thread_that_is_not_open_changes_no_pane() {
        let mut app = open_thread_app();
        live(&mut app, reply("x1", 140, "other-parent", false));
        assert_eq!(ids(&app), ["p", "r1", "r2"]);
        assert_eq!(main_ids(&app), ["a", "p"]);
        update(&mut app, Action::CloseThread);
        live(&mut app, reply("r3", 150, "p", false));
        assert!(app.thread.is_none());
        assert_eq!(main_ids(&app), ["a", "p"]);
    }

    #[test]
    fn a_reply_also_sent_to_the_channel_is_in_both() {
        let mut app = open_thread_app();
        live(&mut app, reply("r3", 140, "p", true));
        assert_eq!(ids(&app), ["p", "r1", "r2", "r3"]);
        assert_eq!(main_ids(&app), ["a", "p", "r3"]);
    }

    #[test]
    fn the_pane_follows_a_reply_only_from_the_newest_message() {
        let mut app = open_thread_app();
        app.thread.as_mut().unwrap().view.cursor = 2;
        live(&mut app, reply("r3", 140, "p", false));
        assert_eq!(app.thread.as_ref().unwrap().view.cursor, 3, "follows");
        app.thread.as_mut().unwrap().view.cursor = 1;
        live(&mut app, reply("r4", 150, "p", false));
        assert_eq!(app.thread.as_ref().unwrap().view.cursor, 1, "stays");
    }

    #[test]
    fn an_update_of_a_thread_parent_the_room_does_not_hold_is_not_appended() {
        let mut app = app_with(vec![msg("a", 100)]);
        live(&mut app, parent("old", 10, 4));
        assert_eq!(main_ids(&app), ["a"]);
    }

    #[test]
    fn the_parents_update_changes_its_count_in_the_main_flow() {
        let mut app = app_with(vec![msg("a", 100), parent("p", 110, 2)]);
        live(
            &mut app,
            Message {
                tlm: Some(300),
                followers: None,
                ..parent("p", 110, 3)
            },
        );
        let held = &app.messages[&rid("r-general")][1];
        assert_eq!((held.tcount, held.tlm), (3, Some(300)));
        assert_eq!(main_ids(&app), ["a", "p"]);
    }

    #[test]
    fn an_edit_a_reaction_and_a_delete_reach_the_pane() {
        let mut app = open_thread_app();
        // An edit of a reply.
        live(
            &mut app,
            Message {
                body: "new text".to_string(),
                edited: true,
                ..reply("r1", 120, "p", false)
            },
        );
        let r1 = pane_message(&app, "r1");
        assert_eq!((r1.body.as_str(), r1.edited), ("new text", true));
        // A reaction on a reply, without the message becoming edited.
        live(
            &mut app,
            Message {
                reactions: vec![crate::data::Reaction {
                    name: "tada".to_string(),
                    users: vec!["bob".to_string()],
                }],
                ..reply("r2", 130, "p", false)
            },
        );
        let r2 = pane_message(&app, "r2");
        assert_eq!((r2.reactions.len(), r2.edited), (1, false));
        // An edit of the parent shows in the pane and in the main flow.
        live(
            &mut app,
            Message {
                body: "parent edited".to_string(),
                edited: true,
                ..parent("p", 110, 2)
            },
        );
        assert_eq!(pane_message(&app, "p").body, "parent edited");
        assert_eq!(app.messages[&rid("r-general")][1].body, "parent edited");
        // A delete of a reply: a tombstone that keeps author and time; one in the room changes
        // nothing in the pane.
        update(
            &mut app,
            Action::RtDeleted {
                room: rid("r-general"),
                id: "r1".to_string(),
            },
        );
        let r1 = pane_message(&app, "r1");
        assert_eq!(r1.kind, MessageKind::Deleted);
        assert_eq!(
            (r1.author.as_str(), r1.ts, r1.body.as_str()),
            ("alice", 120, "")
        );
        update(
            &mut app,
            Action::RtDeleted {
                room: rid("r-general"),
                id: "a".to_string(),
            },
        );
        assert_eq!(ids(&app), ["p", "r1", "r2"]);
    }

    #[test]
    fn an_unchanged_redelivery_changes_nothing() {
        let mut app = open_thread_app();
        let before = pane_message(&app, "r1").clone();
        live(&mut app, reply("r1", 120, "p", false));
        assert!(*pane_message(&app, "r1") == before);
    }

    /// The captured streams of both servers, replayed in order: the room ends with the plain
    /// message, the parent with the server's count and the channel-visible reply; the open pane
    /// ends with the parent and the three replies.
    #[test]
    fn the_captured_streams_converge_on_the_captured_thread() {
        for (stream, thread, label) in [
            (
                include_str!("../api/fixtures/docker/thread_stream.json"),
                include_str!("../api/fixtures/docker/thread_messages.json"),
                "8.9.1",
            ),
            (
                include_str!("../api/fixtures/docker/thread_stream_9.json"),
                include_str!("../api/fixtures/docker/thread_messages_9.json"),
                "9.0.0",
            ),
        ] {
            let room = rid("r-general");
            let frames: Vec<Message> = serde_json::from_str::<Vec<serde_json::Value>>(stream)
                .expect("stream")
                .into_iter()
                .map(|f| {
                    map::message(
                        &room,
                        serde_json::from_value(f["args"][0].clone()).expect("wire"),
                    )
                })
                .collect();
            let mut app = app_with(Vec::new());
            // Up to the parent's first update: the plain message, the parent, its first count.
            for frame in &frames[..3] {
                live(&mut app, frame.clone());
            }
            update(&mut app, Action::OpenThread);
            let parent_id = app.thread.as_ref().expect(label).parent_id.clone();
            loaded(
                &mut app,
                &parent_id,
                0,
                None,
                ThreadPage {
                    messages: Vec::new(),
                    total: 0,
                },
            );
            for frame in &frames[3..] {
                live(&mut app, frame.clone());
            }
            let replies: Vec<Message> =
                serde_json::from_str::<crate::api::wire::ThreadResp>(thread)
                    .expect("thread")
                    .messages
                    .into_iter()
                    .map(|m| map::message(&room, m))
                    .collect();
            let pane: Vec<&str> = ids(&app);
            let mut want: Vec<&str> = vec![parent_id.as_str()];
            want.extend(replies.iter().map(|r| r.id.as_str()));
            assert_eq!(pane, want, "{label}: the pane");
            let held = &app.messages[&room];
            assert_eq!(
                held.len(),
                3,
                "{label}: plain, parent, channel-visible reply"
            );
            let main_parent = held.iter().find(|m| m.id == parent_id).expect(label);
            assert_eq!(main_parent.tcount, 3, "{label}: the server's count");
            assert_eq!(
                pane_message(&app, &parent_id).tcount,
                3,
                "{label}: in the pane too"
            );
            assert!(
                held.iter().any(|m| m.id == replies[2].id && m.tshow),
                "{label}"
            );
        }
    }

    // ---- focus, keys and motions

    #[test]
    fn opening_focuses_the_pane_and_gt_is_the_default_key() {
        let mut app = app_with(vec![msg("a", 100), parent("p", 110, 2)]);
        press(&mut app, "gt");
        assert!(app.thread.is_some());
        assert_eq!(app.focus, Focus::Thread);
        // `gg` and `gr` still work from the message pane.
        press(&mut app, "<C-w>h");
        press(&mut app, "gg");
        assert_eq!(messages::view(&app, &rid("r-general")).cursor, 0);
    }

    #[test]
    fn tab_cycles_through_the_pane_only_while_it_is_open() {
        let mut app = app_with(vec![parent("p", 110, 2)]);
        app.focus = Focus::Sidebar;
        update(&mut app, Action::FocusNext);
        assert_eq!(app.focus, Focus::Messages);
        update(&mut app, Action::FocusNext);
        assert_eq!(app.focus, Focus::Sidebar, "no pane: back to the sidebar");
        app.focus = Focus::Messages;
        update(&mut app, Action::OpenThread);
        app.focus = Focus::Messages;
        update(&mut app, Action::FocusNext);
        assert_eq!(app.focus, Focus::Thread, "after the messages");
        update(&mut app, Action::FocusNext);
        assert_eq!(app.focus, Focus::Sidebar);
        update(&mut app, Action::FocusPrev);
        assert_eq!(app.focus, Focus::Thread, "backward from the sidebar");
    }

    #[test]
    fn ctrl_w_l_and_h_move_between_the_messages_and_the_pane() {
        let mut app = app_with(vec![parent("p", 110, 2)]);
        press(&mut app, "<C-w>l");
        assert_eq!(app.focus, Focus::Messages, "no pane open: the messages");
        update(&mut app, Action::OpenThread);
        press(&mut app, "<C-w>h");
        assert_eq!(app.focus, Focus::Messages);
        press(&mut app, "<C-w>l");
        assert_eq!(app.focus, Focus::Thread);
        press(&mut app, "<C-w>h");
        assert_eq!(app.focus, Focus::Messages);
        press(&mut app, "<C-w>h");
        assert_eq!(app.focus, Focus::Sidebar);
        press(&mut app, "<C-w>l");
        assert_eq!(
            app.focus,
            Focus::Messages,
            "from the sidebar: the messages first"
        );
    }

    #[test]
    fn esc_closes_the_focused_pane_and_returns_to_the_opened_message() {
        let mut app = app_with(vec![parent("p", 110, 2), msg("a", 120), msg("b", 130)]);
        press(&mut app, "2k");
        press(&mut app, "gt");
        assert_eq!(app.focus, Focus::Thread);
        press(&mut app, "<Esc>");
        assert!(app.thread.is_none());
        assert_eq!(app.focus, Focus::Messages);
        assert_eq!(messages::view(&app, &rid("r-general")).cursor, 0);
        // Esc with the messages focused does not close an open pane.
        press(&mut app, "gt");
        press(&mut app, "<C-w>h");
        press(&mut app, "<Esc>");
        assert!(app.thread.is_some());
    }

    #[test]
    fn a_room_change_returns_the_focus_from_the_pane() {
        let mut app = app_with(vec![parent("p", 110, 2)]);
        press(&mut app, "gt");
        assert_eq!(app.focus, Focus::Thread);
        let other = app
            .rooms
            .iter()
            .map(|r| r.id.clone())
            .find(|id| *id != rid("r-general"))
            .expect("another room");
        app.open_room(other);
        assert_eq!((app.thread.is_none(), app.focus), (true, Focus::Messages));
    }

    #[test]
    fn motions_in_the_pane_move_its_own_cursor() {
        let mut app = open_thread_app();
        app.focus = Focus::Thread;
        app.thread.as_mut().unwrap().view.cursor = 0;
        let main_before = messages::view(&app, &rid("r-general"));
        press(&mut app, "j");
        assert_eq!(view(&app).cursor, 1);
        press(&mut app, "G");
        assert_eq!(view(&app).cursor, 2);
        press(&mut app, "j");
        assert_eq!(view(&app).cursor, 2, "stops at the last message");
        press(&mut app, "gg");
        assert_eq!(view(&app).cursor, 0);
        assert_eq!(messages::view(&app, &rid("r-general")), main_before);
    }

    #[test]
    fn reaching_the_last_held_reply_asks_for_the_next_page() {
        let mut app = app_with(vec![parent("p", 100, 90)]);
        update(&mut app, Action::OpenThread);
        loaded(
            &mut app,
            "p",
            0,
            None,
            page(&[("r1", 110), ("r2", 120)], "p", 90),
        );
        app.effects.clear();
        app.focus = Focus::Thread;
        app.thread.as_mut().unwrap().view.cursor = 0;
        press(&mut app, "j");
        assert!(app.effects.is_empty(), "not at the edge yet");
        press(&mut app, "j");
        assert_eq!(thread_effects(&app).len(), 1);
    }

    /// The thread of `p-1` open and focused, as after `gt` on a parent with two replies.
    fn open_pane() -> App {
        let mut app = app_with(vec![parent("p-1", 100, 2)]);
        app.thread = None;
        press(&mut app, "gt");
        loaded(
            &mut app,
            "p-1",
            0,
            None,
            page(&[("r-1", 110), ("r-2", 120)], "p-1", 2),
        );
        app
    }

    fn room_key() -> DraftKey {
        DraftKey::room(rid("r-general"))
    }

    fn thread_key() -> DraftKey {
        DraftKey {
            room: rid("r-general"),
            thread: Some("p-1".to_string()),
        }
    }

    #[test]
    fn insert_from_the_pane_targets_the_thread() {
        let mut app = open_pane();
        assert_eq!(app.focus, Focus::Thread);
        press(&mut app, "i");
        assert_eq!(app.mode, Mode::Insert);
        assert_eq!(app.focus, Focus::Input);
        assert_eq!(app.input_target, InputTarget::thread("p-1".to_string()));
    }

    #[test]
    fn insert_from_the_messages_pane_targets_the_room() {
        let mut app = open_pane();
        app.focus = Focus::Messages;
        press(&mut app, "i");
        assert_eq!(app.input_target, InputTarget::Room);
    }

    #[test]
    fn text_and_target_survive_esc() {
        let mut app = open_pane();
        press(&mut app, "ihalf<Esc>");
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.focus, Focus::Thread);
        assert_eq!(app.input.text(), "half");
        assert_eq!(app.input_target, InputTarget::thread("p-1".to_string()));
        press(&mut app, "i");
        assert_eq!(app.input.text(), "half");
        assert_eq!(app.input_target, InputTarget::thread("p-1".to_string()));
    }

    #[test]
    fn room_and_thread_drafts_stay_apart() {
        let mut app = open_pane();
        app.focus = Focus::Messages;
        press(&mut app, "ia<Esc>");
        app.focus = Focus::Thread;
        press(&mut app, "ib<Esc>");
        assert_eq!(app.input.text(), "b");
        assert_eq!(app.drafts[&room_key()].text(), "a");

        press(&mut app, "<Esc>");
        assert!(app.thread.is_none());
        assert_eq!(app.input_target, InputTarget::Room);
        assert_eq!(app.input.text(), "a");
        assert_eq!(app.drafts[&thread_key()].text(), "b");

        press(&mut app, "gt");
        press(&mut app, "i");
        assert_eq!(app.input.text(), "b");
        assert!(!app.drafts.contains_key(&thread_key()));
    }

    #[test]
    fn an_empty_thread_draft_is_not_stored() {
        let mut app = open_pane();
        press(&mut app, "i<Esc><Esc>");
        assert!(app.drafts.is_empty());
        assert_eq!(app.input_target, InputTarget::Room);
    }

    #[test]
    fn changing_room_resets_the_target_and_keeps_the_thread_draft() {
        let mut app = open_pane();
        press(&mut app, "ireply<Esc>");
        app.open_room(rid("r-random"));
        assert!(app.thread.is_none());
        assert_eq!(app.input_target, InputTarget::Room);
        assert!(app.input.is_empty(), "the other room has no draft");
        assert_eq!(app.drafts[&thread_key()].text(), "reply");
        assert!(!app.drafts.contains_key(&room_key()));
    }

    #[test]
    fn opening_a_thread_puts_the_target_back_on_the_room_and_keeps_the_draft() {
        let mut app = open_pane();
        press(&mut app, "ione<Esc>");
        app.focus = Focus::Messages;
        press(&mut app, "gt");
        assert_eq!(app.input_target, InputTarget::Room);
        assert!(app.input.is_empty());
        assert_eq!(app.drafts[&thread_key()].text(), "one");
    }

    fn sends(app: &App) -> Vec<(String, Option<String>, bool)> {
        app.effects
            .iter()
            .filter_map(|e| match e {
                Effect::SendMessage {
                    id, tmid, tshow, ..
                } => Some((id.clone(), tmid.clone(), *tshow)),
                _ => None,
            })
            .collect()
    }

    fn server_copy(id: &str, tshow: bool) -> Message {
        Message {
            body: "ok".to_string(),
            ts: 130,
            ..reply(id, 130, "p-1", tshow)
        }
    }

    #[test]
    fn enter_in_the_thread_input_sends_a_reply_to_the_thread() {
        let mut app = open_pane();
        let main_before = app.messages[&rid("r-general")].len();
        press(&mut app, "iok<CR>");
        assert_eq!(
            sends(&app),
            vec![("local-1".to_string(), Some("p-1".to_string()), false)]
        );
        let pane = app.thread.as_ref().expect("open");
        let last = pane.messages.last().expect("row");
        assert_eq!((last.id.as_str(), last.body.as_str()), ("local-1", "ok"));
        assert_eq!(last.send, SendState::Sending);
        assert_eq!(app.messages[&rid("r-general")].len(), main_before);
        assert!(app.input.is_empty());
        assert_eq!(app.input_target, InputTarget::thread("p-1".to_string()));
    }

    #[test]
    fn also_in_channel_rides_the_request_and_resets_after_the_send() {
        let mut app = open_pane();
        press(&mut app, "i");
        app.input_target = InputTarget::Thread {
            parent_id: "p-1".to_string(),
            also_in_channel: true,
        };
        press(&mut app, "ok<CR>");
        assert_eq!(
            sends(&app),
            vec![("local-1".to_string(), Some("p-1".to_string()), true)]
        );
        assert_eq!(app.input_target, InputTarget::thread("p-1".to_string()));
    }

    #[test]
    fn replies_and_room_messages_share_one_ordered_queue() {
        let mut app = open_pane();
        app.focus = Focus::Messages;
        press(&mut app, "ione<CR><Esc>");
        app.focus = Focus::Thread;
        press(&mut app, "itwo<CR>");
        app.focus = Focus::Input;
        assert_eq!(sends(&app).len(), 1, "one in flight per room");
        assert_eq!(sends(&app)[0].0, "local-1");
        update(
            &mut app,
            Action::MessageSent {
                room: rid("r-general"),
                id: "local-1".to_string(),
                message: Message {
                    id: "local-1".to_string(),
                    ..msg("local-1", 130)
                },
            },
        );
        let all = sends(&app);
        assert_eq!(all.len(), 2);
        assert_eq!(
            all[1],
            ("local-2".to_string(), Some("p-1".to_string()), false)
        );
    }

    #[test]
    fn the_live_copy_confirms_the_pending_reply_once() {
        let mut app = open_pane();
        press(&mut app, "iok<CR>");
        let before = app.thread.as_ref().expect("open").messages.len();
        update(
            &mut app,
            Action::RtMessage(Message {
                id: "local-1".to_string(),
                ..server_copy("local-1", false)
            }),
        );
        let pane = app.thread.as_ref().expect("open");
        assert_eq!(pane.messages.len(), before, "confirmed in place");
        assert_eq!(pane.messages.last().expect("row").send, SendState::Sent);
        assert_eq!(
            app.messages[&rid("r-general")]
                .iter()
                .filter(|m| m.id == "local-1")
                .count(),
            0
        );
    }

    #[test]
    fn a_rejected_reply_fails_in_the_pane_and_retry_and_discard_work() {
        let mut app = open_pane();
        press(&mut app, "iok<CR>");
        update(
            &mut app,
            Action::SendFailed {
                room: rid("r-general"),
                id: "local-1".to_string(),
                error: ApiError::Rejected,
            },
        );
        let state = |app: &App| {
            app.thread
                .as_ref()
                .expect("open")
                .messages
                .last()
                .expect("row")
                .send
        };
        assert_eq!(state(&app), SendState::Failed);
        assert!(
            app.messages[&rid("r-general")]
                .iter()
                .all(|m| m.id != "local-1"),
            "never in the main flow"
        );
        app.effects.clear();
        app.focus = Focus::Thread;
        app.mode = Mode::Normal;
        press(&mut app, "G");
        press(&mut app, "r");
        assert_eq!(sends(&app).len(), 1);
        assert_eq!(state(&app), SendState::Sending);

        update(
            &mut app,
            Action::SendFailed {
                room: rid("r-general"),
                id: "local-1".to_string(),
                error: ApiError::Forbidden,
            },
        );
        press(&mut app, "dd");
        assert!(
            app.thread
                .as_ref()
                .expect("open")
                .messages
                .iter()
                .all(|m| m.id != "local-1")
        );
    }

    #[test]
    fn a_timed_out_reply_that_arrives_live_is_shown_once_as_sent() {
        let mut app = open_pane();
        press(&mut app, "iok<CR>");
        update(
            &mut app,
            Action::SendFailed {
                room: rid("r-general"),
                id: "local-1".to_string(),
                error: ApiError::Timeout,
            },
        );
        let pane = app.thread.as_ref().expect("open");
        assert_eq!(
            pane.messages.last().expect("row").send,
            SendState::Unconfirmed
        );
        update(
            &mut app,
            Action::RtMessage(Message {
                id: "local-1".to_string(),
                ..server_copy("local-1", false)
            }),
        );
        let pane = app.thread.as_ref().expect("open");
        assert_eq!(
            pane.messages.iter().filter(|m| m.id == "local-1").count(),
            1
        );
        assert_eq!(pane.messages.last().expect("row").send, SendState::Sent);
    }

    #[test]
    fn the_first_reply_raises_the_parents_count() {
        let mut app = app_with_thread_less_parent();
        press(&mut app, "gt");
        assert!(app.thread.is_none(), "no thread yet");
        // The reducer path of `reply_in_thread` opens the pane; here it is opened by hand.
        let mut pane_app = app;
        pane_app.thread = Some(ThreadPane {
            room: rid("r-general"),
            parent_id: "p-0".to_string(),
            origin: "p-0".to_string(),
            messages: vec![msg("p-0", 100)],
            view: MsgView::default(),
            load: ThreadLoad::Loaded,
            total: 0,
            next_offset: 0,
            loading_more: false,
            from_list: false,
        });
        pane_app.focus = Focus::Thread;
        press(&mut pane_app, "ihi<CR>");
        update(
            &mut pane_app,
            Action::MessageSent {
                room: rid("r-general"),
                id: "local-1".to_string(),
                message: Message {
                    id: "local-1".to_string(),
                    ..reply("local-1", 130, "p-0", false)
                },
            },
        );
        let parent = pane_app.messages[&rid("r-general")]
            .iter()
            .find(|m| m.id == "p-0")
            .expect("parent");
        assert_eq!(parent.tcount, 1);
    }

    fn app_with_thread_less_parent() -> App {
        app_with(vec![msg("p-0", 100)])
    }

    #[test]
    fn closing_the_pane_keeps_an_unsent_reply_and_it_comes_back_on_reopen() {
        let mut app = open_pane();
        press(&mut app, "iok<CR><Esc>");
        press(&mut app, "<Esc>");
        assert!(app.thread.is_none());
        assert_eq!(app.outbox.len(), 1);
        assert_eq!(app.undelivered_count(), 1);
        press(&mut app, "gt");
        assert!(app.outbox.is_empty());
        assert!(ids(&app).contains(&"local-1"));
        assert_eq!(app.undelivered_count(), 1);
    }

    #[test]
    fn gr_on_a_message_without_replies_starts_the_first_reply() {
        let mut app = app_with(vec![msg("p-0", 100)]);
        press(&mut app, "gR");
        let pane = app.thread.as_ref().expect("the thread pane opens");
        assert_eq!(pane.parent_id, "p-0");
        assert_eq!(pane.load, ThreadLoad::Loaded, "nothing to fetch");
        assert!(thread_effects(&app).is_empty());
        assert_eq!(app.mode, Mode::Insert);
        assert_eq!(app.focus, Focus::Input);
        assert_eq!(app.input_target, InputTarget::thread("p-0".to_string()));

        press(&mut app, "hi<CR>");
        assert_eq!(
            sends(&app),
            vec![("local-1".to_string(), Some("p-0".to_string()), false)]
        );
    }

    #[test]
    fn gr_on_a_message_with_replies_opens_its_thread_and_loads_it() {
        let mut app = app_with(vec![parent("p-1", 100, 2)]);
        press(&mut app, "gR");
        assert_eq!(thread_effects(&app).len(), 1);
        assert_eq!(app.input_target, InputTarget::thread("p-1".to_string()));
        assert_eq!(app.mode, Mode::Insert);
    }

    #[test]
    fn gr_on_a_reply_shown_in_the_channel_acts_on_its_parent() {
        let mut app = app_with(vec![parent("p-1", 100, 1), reply("r-1", 110, "p-1", true)]);
        press(&mut app, "G");
        press(&mut app, "gR");
        assert_eq!(app.thread.as_ref().expect("open").parent_id, "p-1");
        assert_eq!(app.input_target, InputTarget::thread("p-1".to_string()));
    }

    #[test]
    fn gr_refuses_system_and_deleted_messages() {
        for kind in [MessageKind::System, MessageKind::Deleted] {
            let mut app = app_with(vec![Message {
                kind,
                ..msg("m-1", 100)
            }]);
            press(&mut app, "gR");
            assert!(app.thread.is_none(), "{kind:?}");
            assert_eq!(app.mode, Mode::Normal, "{kind:?}");
            assert!(app.status_msg.is_some(), "{kind:?}");
        }
    }

    #[test]
    fn gr_refuses_in_an_encrypted_room() {
        let mut app = app_with(vec![msg("m-1", 100)]);
        for room in &mut app.rooms {
            room.encrypted = true;
        }
        press(&mut app, "gR");
        assert!(app.thread.is_none());
        assert_eq!(app.mode, Mode::Normal);
        assert!(matches!(app.status_msg, Some(StatusMsg::Error(_))));
    }

    #[test]
    fn gr_from_the_thread_pane_replies_to_the_open_thread() {
        let mut app = open_pane();
        press(&mut app, "gR");
        assert_eq!(app.input_target, InputTarget::thread("p-1".to_string()));
        assert_eq!(app.mode, Mode::Insert);
    }

    #[test]
    fn the_toggle_flips_only_for_a_thread_reply_and_resets_with_the_target() {
        let mut app = open_pane();
        press(&mut app, "i<C-b>");
        assert_eq!(
            app.input_target,
            InputTarget::Thread {
                parent_id: "p-1".to_string(),
                also_in_channel: true
            }
        );
        press(&mut app, "<C-b>");
        assert_eq!(app.input_target, InputTarget::thread("p-1".to_string()));
        press(&mut app, "<C-b>x<CR>");
        assert_eq!(
            sends(&app),
            vec![("local-1".to_string(), Some("p-1".to_string()), true)]
        );
        assert_eq!(
            app.input_target,
            InputTarget::thread("p-1".to_string()),
            "off after a send"
        );

        let mut room = app_with(vec![msg("m-1", 100)]);
        press(&mut room, "i<C-b>");
        assert_eq!(room.input_target, InputTarget::Room);
    }

    /// The pane of `open_pane` with the user being `alice`, the cursor on the last reply (`r-2`).
    fn own_pane() -> App {
        let mut app = open_pane();
        app.own_username = Some("alice".to_string());
        press(&mut app, "G");
        assert_eq!(view(&app).cursor, 2);
        app
    }

    #[test]
    fn e_in_the_pane_edits_an_own_reply_in_the_panes_bar() {
        let mut app = own_pane();
        press(&mut app, "e");
        assert_eq!(app.mode, Mode::Insert);
        assert_eq!(app.input.text(), "body of r-2");
        assert_eq!(app.input_target, InputTarget::thread("p-1".to_string()));
        press(&mut app, "!<CR>");
        assert!(app.effects.iter().any(|e| matches!(
            e,
            Effect::EditMessage { id, .. } if id == "r-2"
        )));
        let copy = Message {
            body: "body of r-2!".to_string(),
            edited: true,
            ..reply("r-2", 120, "p-1", false)
        };
        update(
            &mut app,
            Action::EditDone {
                room: rid("r-general"),
                id: "r-2".to_string(),
                message: copy,
            },
        );
        let pane = app.thread.as_ref().expect("open");
        assert_eq!(pane.messages[2].body, "body of r-2!");
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.focus, Focus::Thread);
    }

    #[test]
    fn e_on_someone_elses_reply_does_nothing() {
        let mut app = own_pane();
        app.own_username = Some("carol".to_string());
        press(&mut app, "e");
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.editing.is_none());
        assert!(
            app.effects
                .iter()
                .all(|e| !matches!(e, Effect::EditMessage { .. }))
        );
    }

    #[test]
    fn dd_in_the_pane_asks_then_deletes_an_own_reply() {
        let mut app = own_pane();
        press(&mut app, "dd");
        assert!(app.confirm.is_some());
        press(&mut app, "y");
        assert!(app.effects.iter().any(|e| matches!(
            e,
            Effect::DeleteMessage { id, .. } if id == "r-2"
        )));
        update(
            &mut app,
            Action::DeleteDone {
                room: rid("r-general"),
                id: "r-2".to_string(),
            },
        );
        let pane = app.thread.as_ref().expect("open");
        assert_eq!(pane.messages[2].kind, MessageKind::Deleted);
    }

    #[test]
    fn dd_on_someone_elses_reply_asks_nothing() {
        let mut app = own_pane();
        app.own_username = Some("carol".to_string());
        press(&mut app, "dd");
        assert!(app.confirm.is_none());
    }

    #[test]
    fn plus_in_the_pane_opens_the_picker_on_the_reply() {
        let mut app = own_pane();
        press(&mut app, "+");
        let picker = app.emoji_box.as_ref().expect("the picker opens");
        assert!(
            matches!(&picker.target, crate::app::react::Target::React { id, .. } if id == "r-2")
        );
        assert!(crate::app::react::view(&app).is_some());
    }

    fn follow_effects(app: &App) -> Vec<(String, bool)> {
        app.effects
            .iter()
            .filter_map(|e| match e {
                Effect::FollowThread {
                    parent_id, follow, ..
                } => Some((parent_id.clone(), *follow)),
                _ => None,
            })
            .collect()
    }

    fn follower_app(followers: Option<Vec<&str>>) -> App {
        let mut p = parent("p-1", 100, 2);
        p.followers = followers.map(|f| f.into_iter().map(str::to_string).collect());
        let mut app = app_with(vec![p]);
        app.own_user_id = Some("me".to_string());
        app
    }

    fn followed(app: &App) -> bool {
        app.messages[&rid("r-general")][0].followed_by("me")
    }

    #[test]
    fn gf_follows_an_unfollowed_thread_when_the_server_accepts() {
        let mut app = follower_app(Some(vec!["other"]));
        press(&mut app, "gf");
        assert_eq!(follow_effects(&app), vec![("p-1".to_string(), true)]);
        assert!(!followed(&app), "nothing changes before the answer");
        update(
            &mut app,
            Action::FollowDone {
                room: rid("r-general"),
                parent_id: "p-1".to_string(),
                follow: true,
            },
        );
        assert!(followed(&app));
        assert!(app.messages[&rid("r-general")][0].followed_by("other"));
        assert!(!app.follow_pending);
    }

    #[test]
    fn gf_unfollows_a_followed_thread() {
        let mut app = follower_app(Some(vec!["me", "other"]));
        press(&mut app, "gf");
        assert_eq!(follow_effects(&app), vec![("p-1".to_string(), false)]);
        update(
            &mut app,
            Action::FollowDone {
                room: rid("r-general"),
                parent_id: "p-1".to_string(),
                follow: false,
            },
        );
        assert!(!followed(&app));
        assert!(app.messages[&rid("r-general")][0].followed_by("other"));
    }

    #[test]
    fn a_rejected_follow_changes_nothing_and_says_so_without_text() {
        let mut app = follower_app(Some(vec![]));
        press(&mut app, "gf");
        update(
            &mut app,
            Action::FollowFailed {
                follow: true,
                error: ApiError::Rejected,
            },
        );
        assert!(!followed(&app));
        assert!(!app.follow_pending);
        let Some(StatusMsg::Error(line)) = &app.status_msg else {
            panic!("a status line");
        };
        assert!(line.starts_with("thread not followed"), "{line}");
        update(
            &mut app,
            Action::FollowFailed {
                follow: false,
                error: ApiError::Timeout,
            },
        );
        let Some(StatusMsg::Error(line)) = &app.status_msg else {
            panic!("a status line");
        };
        assert!(
            line.starts_with("thread may not have been unfollowed"),
            "{line}"
        );
    }

    #[test]
    fn a_second_press_waits_for_the_first_answer() {
        let mut app = follower_app(Some(vec![]));
        press(&mut app, "gf");
        press(&mut app, "gf");
        assert_eq!(follow_effects(&app).len(), 1);
    }

    #[test]
    fn gf_on_a_message_with_no_thread_says_so() {
        let mut app = app_with(vec![msg("m-1", 100)]);
        app.own_user_id = Some("me".to_string());
        press(&mut app, "gf");
        assert!(follow_effects(&app).is_empty());
        assert!(matches!(app.status_msg, Some(StatusMsg::Info(_))));
    }

    #[test]
    fn gf_waits_for_the_own_id() {
        let mut app = follower_app(Some(vec![]));
        app.own_user_id = None;
        press(&mut app, "gf");
        assert!(follow_effects(&app).is_empty());
        assert!(matches!(app.status_msg, Some(StatusMsg::Error(_))));
    }

    #[test]
    fn gf_in_the_pane_toggles_the_open_thread_and_updates_the_pane_copy() {
        let mut app = follower_app(Some(vec![]));
        press(&mut app, "gt");
        loaded(&mut app, "p-1", 0, None, page(&[("r-1", 110)], "p-1", 1));
        press(&mut app, "gf");
        assert_eq!(follow_effects(&app), vec![("p-1".to_string(), true)]);
        update(
            &mut app,
            Action::FollowDone {
                room: rid("r-general"),
                parent_id: "p-1".to_string(),
                follow: true,
            },
        );
        let pane = app.thread.as_ref().expect("open");
        assert!(pane.messages[0].followed_by("me"));
    }

    #[test]
    fn the_parents_live_update_carries_the_follow_state_from_another_client() {
        let mut app = follower_app(Some(vec!["me"]));
        let mut update_copy = parent("p-1", 100, 2);
        update_copy.followers = Some(vec![]);
        update(&mut app, Action::RtMessage(update_copy));
        assert!(!followed(&app), "unfollowed from the web client");
        // A copy that does not say keeps what is known.
        let mut app = follower_app(Some(vec!["me"]));
        update(&mut app, Action::RtMessage(parent("p-1", 100, 3)));
        assert!(followed(&app));
        assert_eq!(app.messages[&rid("r-general")][0].tcount, 3);
    }

    #[test]
    fn the_own_id_arrives_with_the_me_answer() {
        let mut app = app_with(vec![]);
        update(&mut app, Action::OwnIdLoaded("u1".to_string()));
        assert_eq!(app.own_user_id.as_deref(), Some("u1"));
    }

    // ---- reading a thread (`thread-unread` spec)

    /// A thread of parent `p` listed unread for `r-general` that is open and loaded.
    fn open_unread_thread_app() -> App {
        let mut app = app_with(vec![msg("a", 100), parent("p", 110, 2)]);
        let room = app
            .rooms
            .iter_mut()
            .find(|r| r.id == rid("r-general"))
            .unwrap();
        room.threads.unread.insert("p".to_string());
        update(&mut app, Action::OpenThread);
        loaded(
            &mut app,
            "p",
            0,
            None,
            page(&[("r1", 120), ("r2", 130)], "p", 2),
        );
        app
    }

    fn thread_marks(app: &App) -> Vec<(&RoomId, &str)> {
        app.effects
            .iter()
            .filter_map(|e| match e {
                Effect::MarkThreadRead { room, parent_id } => Some((room, parent_id.as_str())),
                _ => None,
            })
            .collect()
    }

    fn thread_is_unread(app: &App) -> bool {
        app.rooms
            .iter()
            .find(|r| r.id == rid("r-general"))
            .unwrap()
            .threads
            .is_unread("p")
    }

    #[test]
    fn an_open_unread_thread_is_marked_read_after_the_delay() {
        let mut app = open_unread_thread_app();
        // Opening sends nothing: the mark waits for its timer.
        assert!(thread_marks(&app).is_empty());
        let pending = app.read.thread_pending.clone().expect("a wait");
        assert_eq!(
            (pending.room, pending.parent_id.as_str()),
            (rid("r-general"), "p")
        );
        update(&mut app, Action::MarkThreadReadDue);
        assert_eq!(thread_marks(&app), [(&rid("r-general"), "p")]);
        assert!(app.read.thread_pending.is_none());
        // The server confirms: `new` and the badge share go, no realtime update needed.
        update(
            &mut app,
            Action::MarkThreadReadDone {
                room: rid("r-general"),
                parent_id: "p".to_string(),
            },
        );
        assert!(!thread_is_unread(&app));
    }

    #[test]
    fn closing_before_the_delay_leaves_the_thread_unread() {
        let mut app = open_unread_thread_app();
        update(&mut app, Action::CloseThread);
        assert!(app.read.thread_pending.is_none());
        update(&mut app, Action::MarkThreadReadDue);
        assert!(thread_marks(&app).is_empty());
        assert!(thread_is_unread(&app));
    }

    #[test]
    fn a_stale_timer_for_a_replaced_thread_sends_nothing() {
        let mut app = open_unread_thread_app();
        let pending = app.read.thread_pending.clone().expect("a wait");
        // The pane now shows another thread, as if the timer had been about to fire.
        app.thread.as_mut().unwrap().parent_id = "other".to_string();
        app.read.thread_pending = Some(pending);
        update(&mut app, Action::MarkThreadReadDue);
        assert!(thread_marks(&app).is_empty());
    }

    #[test]
    fn a_thread_that_is_not_unread_sends_nothing() {
        let mut app = open_thread_app();
        update(&mut app, Action::MarkThreadReadDue);
        assert!(thread_marks(&app).is_empty());
    }

    #[test]
    fn a_loaded_thread_opens_on_its_newest_message() {
        let mut app = app_with(vec![msg("a", 100), parent("p", 110, 2)]);
        update(&mut app, Action::OpenThread);
        loaded(
            &mut app,
            "p",
            0,
            None,
            page(&[("r1", 120), ("r2", 130)], "p", 90),
        );
        assert_eq!(view(&app).cursor, 2, "parent, r1, r2: on r2");
    }

    #[test]
    fn a_thread_not_focused_is_not_marked_when_the_wait_ends() {
        let mut app = open_unread_thread_app();
        app.focus = Focus::Messages;
        update(&mut app, Action::MarkThreadReadDue);
        assert!(thread_marks(&app).is_empty());
    }

    #[test]
    fn a_zero_delay_marks_the_thread_at_once() {
        let mut app = app_with(vec![msg("a", 100), parent("p", 110, 2)]);
        app.settings.mark_read_delay_ms = 0;
        let room = app
            .rooms
            .iter_mut()
            .find(|r| r.id == rid("r-general"))
            .unwrap();
        room.threads.unread.insert("p".to_string());
        update(&mut app, Action::OpenThread);
        assert!(thread_marks(&app).is_empty(), "nothing is shown yet");
        loaded(&mut app, "p", 0, None, page(&[("r1", 120)], "p", 90));
        assert_eq!(thread_marks(&app), [(&rid("r-general"), "p")]);
        assert!(app.read.thread_pending.is_none());
    }

    #[test]
    fn a_failed_mark_leaves_the_thread_unread_and_shows_nothing() {
        let mut app = open_unread_thread_app();
        update(&mut app, Action::MarkThreadReadDue);
        // A failure answers no Action at all (see `effect::run`): the state is unchanged.
        assert!(thread_is_unread(&app));
        assert!(app.status_msg.is_none());
    }

    /// The server says thread `p` has replies the user has not read.
    fn thread_unread_update() -> Action {
        Action::RtSubscription {
            room: rid("r-general"),
            update: crate::data::UnreadUpdate {
                threads: crate::data::ThreadUnreadUpdate {
                    unread: Some(std::collections::HashSet::from(["p".to_string()])),
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }

    #[test]
    fn a_live_reply_to_the_focused_open_thread_restarts_the_wait() {
        let mut app = open_unread_thread_app();
        update(&mut app, Action::MarkThreadReadDue);
        update(
            &mut app,
            Action::MarkThreadReadDone {
                room: rid("r-general"),
                parent_id: "p".to_string(),
            },
        );
        app.effects.clear();
        assert!(!thread_is_unread(&app));
        assert!(app.read.thread_pending.is_none());
        // Another user replies: the server lists the thread unread again.
        live(&mut app, reply("r3", 140, "p", false));
        update(&mut app, thread_unread_update());
        assert!(thread_is_unread(&app));
        assert!(app.read.thread_pending.is_some(), "the wait restarted");
        update(&mut app, Action::MarkThreadReadDue);
        assert_eq!(thread_marks(&app), [(&rid("r-general"), "p")]);
    }

    #[test]
    fn a_live_reply_to_an_open_but_unfocused_thread_does_not_restart_the_wait() {
        let mut app = open_unread_thread_app();
        update(&mut app, Action::MarkThreadReadDue);
        update(
            &mut app,
            Action::MarkThreadReadDone {
                room: rid("r-general"),
                parent_id: "p".to_string(),
            },
        );
        app.focus = crate::app::Focus::Messages;
        update(&mut app, thread_unread_update());
        assert!(app.read.thread_pending.is_none());
    }

    #[test]
    fn a_colon_opens_the_emoji_box_in_the_panes_bar_and_while_editing() {
        let mut app = own_pane();
        press(&mut app, "i");
        assert_eq!(app.input_target, InputTarget::thread("p-1".to_string()));
        press(&mut app, ":");
        assert!(app.emoji_box.is_some());
        press(&mut app, "tada");
        press(&mut app, "<CR>");
        assert_eq!(app.input.text(), "\u{1F389}");
        press(&mut app, "<Esc>");

        let mut app = own_pane();
        press(&mut app, "e");
        assert_eq!(app.input.text(), "body of r-2");
        press(&mut app, "<Space>:");
        assert!(app.emoji_box.is_some());
        press(&mut app, "tada");
        press(&mut app, "<CR>");
        assert_eq!(app.input.text(), "body of r-2 \u{1F389}");
    }
}
