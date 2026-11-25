//! The thread pane (`message-threads`): one open thread of the open room, read only.
//!
//! The pane holds the parent message first and then its replies oldest first, with its own cursor
//! and scroll. A reply lives only here (`app::realtime` keeps it out of the room's main flow), so
//! the pane is rebuilt from the server each time a thread is opened. Everything is addressed by
//! room and parent id, so an answer for a thread that is no longer open is dropped.

use crate::api::{ApiError, ThreadPage};
use crate::app::messages::{self, MsgView};
use crate::app::{App, StatusMsg};
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
    let parent = held.iter().find(|m| m.id == parent_id).cloned();
    let fetch_parent = parent.is_none();
    app.thread = Some(ThreadPane {
        room: room.clone(),
        parent_id: parent_id.clone(),
        origin,
        messages: parent.into_iter().collect(),
        view: MsgView::default(),
        load: ThreadLoad::Loading,
        total: 0,
        next_offset: 0,
        loading_more: false,
    });
    app.effects.push(Effect::LoadThread {
        room,
        parent_id,
        offset: 0,
        count: PAGE,
        fetch_parent,
    });
    app.focus = crate::app::Focus::Thread;
}

/// `close_thread`: close the pane and put the main flow's cursor on the message it was opened from
/// (when that message is held). Nothing happens when no thread is open.
pub fn close(app: &mut App) {
    let Some(pane) = app.thread.take() else {
        return;
    };
    if app.focus == crate::app::Focus::Thread {
        app.focus = crate::app::Focus::Messages;
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

/// Drop the pane without touching the main flow: the room it belonged to is no longer open.
pub fn discard(app: &mut App) {
    app.thread = None;
    if app.focus == crate::app::Focus::Thread {
        app.focus = crate::app::Focus::Messages;
    }
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
        if slot.kind != MessageKind::Deleted && messages::changed(slot, message) {
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
    messages::row_starts_of(msgs, width, &app.row_ctx(msgs))
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
        }
    }

    fn parent(id: &str, ts: i64, replies: u32) -> Message {
        Message {
            tcount: replies,
            tlm: Some(ts + 10),
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
        press(&mut app, "j");
        assert!(app.effects.is_empty(), "not at the edge yet");
        press(&mut app, "j");
        assert_eq!(thread_effects(&app).len(), 1);
    }
}
