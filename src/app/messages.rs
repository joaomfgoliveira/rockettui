//! Per-room message cursor and row-based scroll model (D-04).
//!
//! Heights come from `ui::wrap::message_rows` at the width of `App::areas`, the same function
//! and geometry the renderer uses, so scroll math and drawing never disagree. Both build their
//! `RowCtx` with `App::row_ctx` (layout, author and time columns, time format, zone), so heights
//! use exactly the columns that are drawn.

use crate::action::Action;
use crate::app::App;
use crate::data::{Message, MessageKind, RoomId, SendState};
use crate::ui::wrap::{self, RowCtx};

/// Where a room's message pane is looking: the selected message and the first visible row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MsgView {
    pub cursor: usize,
    pub top_row: usize,
}

/// Inner (width, height) of the messages pane for the app's current size, borders removed.
pub fn viewport(app: &App) -> (u16, u16) {
    let pane = app.areas().messages;
    (pane.width.saturating_sub(2), pane.height.saturating_sub(2))
}

/// First row of every message of `room` at the current width, with the total row count as the
/// last element (so a room of `n` messages yields `n + 1` entries; no room data yields `[0]`).
pub fn row_starts(app: &App, room: &RoomId) -> Vec<usize> {
    let (width, _) = viewport(app);
    let msgs = app.messages.get(room).map_or(&[][..], Vec::as_slice);
    row_starts_of(msgs, width, &app.pane_row_ctx(room, msgs))
}

/// The new `top_row` that brings message `cursor` into the window.
///
/// `starts` is `row_starts`: message `i` occupies rows `starts[i]..starts[i + 1]`. A message
/// taller than the window is aligned to its first row. A zero-height window only clamps.
pub fn scroll_into_view(
    starts: &[usize],
    cursor: usize,
    top_row: usize,
    viewport_h: usize,
) -> usize {
    let total = starts.last().copied().unwrap_or(0);
    let max_top = total.saturating_sub(viewport_h);
    if viewport_h == 0 || cursor + 1 >= starts.len() {
        return top_row.min(max_top);
    }
    let first = starts[cursor];
    let end = starts[cursor + 1];
    let top = if end - first > viewport_h || first < top_row {
        first
    } else if end > top_row.saturating_add(viewport_h) {
        end - viewport_h
    } else {
        top_row
    };
    top.min(max_top)
}

/// The effective view of `room`: the stored one (or the newest default), clamped and scrolled so
/// the cursor message is inside the window at the current size. Nothing is cached: the viewport
/// and row heights are recomputed from `app.size` on every call, so a Resize can never leave the
/// cursor message outside the window.
pub fn view(app: &App, room: &RoomId) -> MsgView {
    let starts = row_starts(app, room);
    let count = starts.len() - 1;
    if count == 0 {
        return MsgView::default();
    }
    let total = starts[count];
    let viewport_h = usize::from(viewport(app).1);
    let (cursor, top_row) = match app.msg_views.get(room) {
        Some(v) => (v.cursor, v.top_row),
        None => (count - 1, total.saturating_sub(viewport_h)),
    };
    let cursor = cursor.min(count - 1);
    MsgView {
        cursor,
        top_row: scroll_into_view(&starts, cursor, top_row, viewport_h),
    }
}

/// First row of every message of `msgs` at `width` columns, with the total row count last.
pub fn row_starts_of(msgs: &[Message], width: u16, ctx: &RowCtx) -> Vec<usize> {
    let mut starts = vec![0];
    let mut total = 0;
    for (i, msg) in msgs.iter().enumerate() {
        let prev = i.checked_sub(1).map(|p| &msgs[p]);
        total += wrap::block_rows(msg, prev, width, ctx);
        starts.push(total);
    }
    starts
}

/// The cursor and scroll position a motion leads to, or `None` when `motion` is not one of the
/// six list motions or the list is empty. `starts` is `row_starts`; `current` is the effective
/// view. Pure, so the messages pane and the search results list move by the same rules.
pub fn step(
    starts: &[usize],
    current: MsgView,
    viewport_h: usize,
    motion: &Action,
    count: Option<u32>,
) -> Option<MsgView> {
    if !matches!(
        motion,
        Action::MoveDown
            | Action::MoveUp
            | Action::GotoTop
            | Action::GotoBottom
            | Action::HalfPageDown
            | Action::HalfPageUp
    ) {
        return None;
    }
    let messages = starts.len() - 1;
    if messages == 0 {
        return None;
    }
    let last = messages - 1;
    let total = starts[messages];
    let max_top = total.saturating_sub(viewport_h);
    let n = count.map(|c| c as usize);
    let step = n.unwrap_or(1);
    // A half page is max(1, rows / 2); the cursor moves by the same number of rows.
    let delta = (viewport_h / 2).max(1).saturating_mul(step);
    let message_at = |row: usize| (starts.partition_point(|&s| s <= row) - 1).min(last);

    let (cursor, top_row) = match motion {
        Action::MoveDown => (
            current.cursor.saturating_add(step).min(last),
            current.top_row,
        ),
        Action::MoveUp => (current.cursor.saturating_sub(step), current.top_row),
        Action::GotoTop => (
            n.map_or(0, |c| c.saturating_sub(1)).min(last),
            current.top_row,
        ),
        Action::GotoBottom => (
            n.map_or(last, |c| c.saturating_sub(1)).min(last),
            current.top_row,
        ),
        Action::HalfPageDown => {
            let row = starts[current.cursor]
                .saturating_add(delta)
                .min(total.saturating_sub(1));
            (
                message_at(row),
                current.top_row.saturating_add(delta).min(max_top),
            )
        }
        _ => {
            let row = starts[current.cursor].saturating_sub(delta);
            (message_at(row), current.top_row.saturating_sub(delta))
        }
    };
    Some(MsgView {
        cursor,
        top_row: scroll_into_view(starts, cursor, top_row, viewport_h),
    })
}

/// Apply a Normal-mode motion to the messages pane of the current room.
///
/// h and l (and Select and za) are intentional no-ops in this pane (D-19): they return before
/// anything is stored. An empty room or no room is a no-op too.
pub fn apply_motion(app: &mut App, motion: &Action, count: Option<u32>) {
    let Some(room) = app.current_room.clone() else {
        return;
    };
    let starts = row_starts(app, &room);
    let viewport_h = usize::from(viewport(app).1);
    let current = view(app, &room);
    if let Some(next) = step(&starts, current, viewport_h, motion, count) {
        app.msg_views.insert(room, next);
    }
}

/// True when the last row of `room` is visible in the effective view. Call it before a new row is
/// appended: a room with no stored view shows its newest rows, so it counts as at the bottom, and a
/// room shorter than the window is at the bottom with any view (D-05).
pub fn is_at_bottom(app: &App, room: &RoomId) -> bool {
    if !app.msg_views.contains_key(room) {
        return true;
    }
    let total = row_starts(app, room).last().copied().unwrap_or(0);
    let viewport_h = usize::from(viewport(app).1);
    view(app, room).top_row + viewport_h >= total
}

/// What `merge_incoming` did with a server message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Merge {
    /// The `_id` was absent: the message was appended to the room.
    Appended,
    /// The `_id` is held by a `Sent` message: nothing changed.
    Ignored,
    /// The `_id` was held by a still-pending message (Queued, Sending, Failed or Unconfirmed): it
    /// was replaced in place, at this index, and marked `Sent`.
    Confirmed(usize),
}

/// The held-id rule of D-13 as amended by D-21, for one server message: the one place it lives,
/// shared by `merge_incoming` and the history merges.
///
/// `None` when `room` holds no message with `message.id` (nothing changed; the caller adds it). A
/// held `Sent` copy is `Some(Ignored)`. A held copy that is still pending (Queued, Sending, Failed
/// or Unconfirmed) is replaced in place by a clone of `message` marked `Sent` and reported as
/// `Some(Confirmed(index))`, so a send whose server answer was lost is confirmed by a later server
/// copy. When the replaced copy was `Sending`, its POST has not answered, so the room's single
/// in-flight slot stays occupied: `App.awaiting_answer` records the room and the id until
/// `send::on_sent` or `send::on_failed` releases it (D-03).
///
/// It matches by `_id` within `room` only (T-04-18) and never pumps, touches `msg_views` or records
/// an effect: an in-place replace keeps every index.
pub fn confirm_held(app: &mut App, room: &RoomId, message: &Message) -> Option<Merge> {
    let held = app.messages.get_mut(room)?;
    let index = held.iter().position(|m| m.id == message.id)?;
    let was = held[index].send;
    if was == SendState::Sent {
        return Some(Merge::Ignored);
    }
    held[index] = Message {
        send: SendState::Sent,
        ..message.clone()
    };
    if was == SendState::Sending {
        app.awaiting_answer.insert(room.clone(), message.id.clone());
    }
    Some(Merge::Confirmed(index))
}

/// The one append path for a server message that arrives outside a history page (D-13, D-21):
/// whatever `confirm_held` decides when the `_id` is held, otherwise the message is appended,
/// marked `Sent`, creating the room's vector. Phase 5's realtime echo of the user's own message
/// lands here, as a no-op or a confirmation on its client id. It does not pump and does not touch
/// `msg_views`: an append shifts no existing index.
pub fn merge_incoming(app: &mut App, room: &RoomId, message: Message) -> Merge {
    if let Some(outcome) = confirm_held(app, room, &message) {
        return outcome;
    }
    app.messages.entry(room.clone()).or_default().push(Message {
        send: SendState::Sent,
        ..message
    });
    Merge::Appended
}

/// Run `change` on `app` and keep a stored view of `room` on the same message at the same screen
/// row afterwards. For a change that alters row heights without moving an index (an edit, a
/// tombstone): the cursor message and its row inside the window are read first, then put back by
/// message id. A room with no stored view follows the newest rows and needs nothing.
pub fn anchored<R>(app: &mut App, room: &RoomId, change: impl FnOnce(&mut App) -> R) -> R {
    let anchor = app.msg_views.contains_key(room).then(|| {
        let view = view(app, room);
        let starts = row_starts(app, room);
        let id = app.messages[room][view.cursor].id.clone();
        (id, starts[view.cursor] as isize - view.top_row as isize)
    });
    let result = change(app);
    if let Some((id, offset)) = anchor
        && let Some(cursor) = app
            .messages
            .get(room)
            .and_then(|held| held.iter().position(|m| m.id == id))
    {
        let starts = row_starts(app, room);
        let top_row = (starts[cursor] as isize - offset).max(0) as usize;
        app.msg_views
            .insert(room.clone(), MsgView { cursor, top_row });
    }
    result
}

/// True when `message` differs from the held copy `held` in what a live update changes: the text,
/// the edited state, the reactions and, for a thread parent, the reply count and last reply time.
pub fn changed(held: &Message, message: &Message) -> bool {
    held.body != message.body
        || held.edited != message.edited
        || held.reactions != message.reactions
        || held.tcount != message.tcount
        || held.tlm != message.tlm
}

/// True when `room` holds a message with this id.
pub fn holds(app: &App, room: &RoomId, id: &str) -> bool {
    app.messages
        .get(room)
        .is_some_and(|held| held.iter().any(|m| m.id == id))
}

/// Put `message` into the held `slot`, as a `Sent` message. An `rm` copy comes from the deleter:
/// the original author and time stay.
pub fn replace_slot(slot: &mut Message, message: Message) {
    let keep = (message.kind == MessageKind::Deleted).then(|| (slot.author.clone(), slot.ts));
    *slot = Message {
        send: SendState::Sent,
        ..message
    };
    if let Some((author, ts)) = keep {
        slot.author = author;
        slot.ts = ts;
    }
}

/// Turn `slot` into a tombstone: author and time stay, the body and the reactions go.
pub fn tombstone_slot(slot: &mut Message) {
    slot.body.clear();
    slot.kind = MessageKind::Deleted;
    slot.edited = false;
    // A tombstone shows nothing of the message, reactions included (as `map::message` maps it).
    slot.reactions.clear();
}

/// True when `message` is a live update of a message `room` holds: the held copy is a server
/// message (`Sent`, not a tombstone) whose text, edited state or reactions differ. An equal copy
/// (a pin) and a copy of a pending local row are not updates. A reaction changes only the
/// reactions, so it replaces them without the message becoming edited: the `edited` flag comes from
/// the server's `editedAt`, which a reaction does not set.
pub fn is_update(app: &App, room: &RoomId, message: &Message) -> bool {
    app.messages
        .get(room)
        .and_then(|held| held.iter().find(|m| m.id == message.id))
        .is_some_and(|held| {
            held.send == SendState::Sent
                && held.kind != MessageKind::Deleted
                && changed(held, message)
        })
}

/// Replace the held server message with `message.id` by `message`, in place (no index moves), and
/// keep a stored view on the same message and screen row. A no-op for anything but a held `Sent`,
/// non-deleted message that `is_update` says changed.
pub fn replace_updated(app: &mut App, room: &RoomId, message: Message) {
    if is_update(app, room, &message) {
        anchored(app, room, |app| apply_update(app, room, message));
    }
}

/// The unanchored half of `replace_updated`, for a caller that anchors the view itself (a history
/// merge). Replaces the held message with `message.id` when `is_update` says it is one.
pub fn apply_update(app: &mut App, room: &RoomId, message: Message) {
    if !is_update(app, room, &message) {
        return;
    }
    if let Some(slot) = app
        .messages
        .get_mut(room)
        .and_then(|held| held.iter_mut().find(|m| m.id == message.id))
    {
        replace_slot(slot, message);
    }
}

/// Turn the held server message `id` of `room` into a tombstone: author and time stay, the body is
/// cleared, the kind becomes `Deleted`. Returns whether anything changed: an unknown id, a pending
/// local row (any state but `Sent`) and an already deleted message change nothing. No index moves;
/// a stored view keeps the same message and screen row.
pub fn tombstone(app: &mut App, room: &RoomId, id: &str) -> bool {
    let deletable = app
        .messages
        .get(room)
        .and_then(|held| held.iter().find(|m| m.id == id))
        .is_some_and(|m| m.send == SendState::Sent && m.kind != MessageKind::Deleted);
    if !deletable {
        return false;
    }
    anchored(app, room, |app| tombstone_in_place(app, room, id));
    true
}

/// The unanchored half of `tombstone`, for a caller that anchors the view itself (a history merge).
/// The caller has already checked the message is a deletable server message.
pub fn tombstone_in_place(app: &mut App, room: &RoomId, id: &str) {
    if let Some(slot) = app
        .messages
        .get_mut(room)
        .and_then(|held| held.iter_mut().find(|m| m.id == id))
    {
        tombstone_slot(slot);
    }
}

/// Forget the stored view so the room shows its newest message again (plan 01-06 calls this
/// after appending a local message).
pub fn follow_newest(app: &mut App, room: &RoomId) {
    app.msg_views.remove(room);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Focus;
    use crate::app::update::update;
    use crate::test_util::{app_80x24, app_with_config, config_from_toml, press};

    fn general() -> RoomId {
        RoomId("general".to_string())
    }

    #[test]
    fn viewport_follows_sidebar_width() {
        for (toml, width) in [
            ("sidebar_width = 40\n", 38),
            ("sidebar_width = 10\n", 68),
            ("", 50),
        ] {
            let app = app_with_config(config_from_toml(toml), 80, 24);
            assert_eq!(viewport(&app).0, width, "config {toml:?}");
        }
    }

    #[test]
    fn row_model_matches_renderer_custom_format() {
        let config = config_from_toml("timestamp_format = \"%Y-%m-%d %H:%M\"\n");
        let mut app = app_with_config(config, 80, 24);
        app.open_room(general());
        let (width, _) = viewport(&app);
        let msgs = &app.messages[&general()];
        let ctx = app.row_ctx(msgs);
        let expected: usize = msgs
            .iter()
            .enumerate()
            .map(|(i, m)| wrap::block_rows(m, i.checked_sub(1).map(|p| &msgs[p]), width, &ctx))
            .sum();
        let starts = row_starts(&app, &general());
        assert_eq!(starts.last().copied(), Some(expected));
        assert_eq!(ctx.time_col, 16);
    }

    fn open_general() -> App {
        let mut app = app_80x24();
        app.open_room(general());
        app
    }

    /// A reaction named `name` from one user.
    fn reaction(name: &str) -> crate::data::Reaction {
        crate::data::Reaction {
            name: name.to_string(),
            users: vec!["u".to_string()],
        }
    }

    /// The cursor message's id and the screen row it sits on.
    fn anchor(app: &App) -> (String, isize) {
        let v = view(app, &general());
        let starts = row_starts(app, &general());
        (
            app.messages[&general()][v.cursor].id.clone(),
            starts[v.cursor] as isize - v.top_row as isize,
        )
    }

    #[test]
    fn a_taller_message_above_the_cursor_keeps_the_cursor_on_its_row() {
        let mut app = open_general();
        press(&mut app, "gg");
        press(&mut app, "20j");
        let room = general();
        assert_eq!(view(&app, &room).cursor, 20);
        let before = anchor(&app);
        let starts_before = row_starts(&app, &room);

        // Many reactions on a message above the cursor wrap onto several extra rows.
        let many: Vec<_> = (0..12).map(|n| reaction(&format!("custom_{n}"))).collect();
        anchored(&mut app, &room, |app| {
            app.messages.get_mut(&room).unwrap()[5].reactions = many;
        });
        let starts_after = row_starts(&app, &room);
        assert!(
            starts_after[20] >= starts_before[20] + 3,
            "the reactions must shift the rows below: {} then {}",
            starts_before[20],
            starts_after[20]
        );
        assert_eq!(anchor(&app), before, "same message at the same screen row");
        assert_cursor_visible(&app, view(&app, &room));

        // Taking them back restores the rows and the anchor.
        anchored(&mut app, &room, |app| {
            app.messages.get_mut(&room).unwrap()[5].reactions.clear();
        });
        assert_eq!(row_starts(&app, &room), starts_before);
        assert_eq!(anchor(&app), before);
    }

    fn viewport_h(app: &App) -> usize {
        usize::from(viewport(app).1)
    }

    /// Index of the message that holds `row`.
    fn containing(starts: &[usize], row: usize) -> usize {
        starts.partition_point(|&s| s <= row) - 1
    }

    /// The cursor message lies inside the window, or starts at the top when it is taller.
    fn assert_cursor_visible(app: &App, v: MsgView) {
        let starts = row_starts(app, &general());
        let vh = viewport_h(app);
        let first = starts[v.cursor];
        let end = starts[v.cursor + 1];
        if end - first > vh {
            assert_eq!(v.top_row, first, "tall message must start at the top");
        } else {
            assert!(v.top_row <= first, "{v:?} first {first}");
            assert!(end <= v.top_row + vh, "{v:?} end {end} vh {vh}");
        }
    }

    #[test]
    fn open_room_starts_at_newest() {
        let app = open_general();
        let v = view(&app, &general());
        assert_eq!(v.cursor, 59);
        let starts = row_starts(&app, &general());
        let vh = viewport_h(&app);
        let last_row = starts[60] - 1;
        assert!(v.top_row <= last_row && last_row < v.top_row + vh);
        assert_eq!(v.top_row, starts[60] - vh);
        assert!(app.msg_views.is_empty(), "the default view is not stored");
    }

    #[test]
    fn motion_edges() {
        let mut app = open_general();
        press(&mut app, "gg");
        press(&mut app, "k");
        assert_eq!(view(&app, &general()).cursor, 0);
        press(&mut app, "G");
        press(&mut app, "j");
        assert_eq!(view(&app, &general()).cursor, 59);
        press(&mut app, "k");
        assert_eq!(view(&app, &general()).cursor, 58);
        press(&mut app, "j");
        assert_eq!(view(&app, &general()).cursor, 59);
    }

    #[test]
    fn motion_gg_g_capital() {
        let mut app = open_general();
        press(&mut app, "gg");
        let v = view(&app, &general());
        assert_eq!((v.cursor, v.top_row), (0, 0));
        press(&mut app, "G");
        let v = view(&app, &general());
        assert_eq!(v.cursor, 59);
        assert_cursor_visible(&app, v);
        let starts = row_starts(&app, &general());
        assert_eq!(v.top_row, starts[60] - viewport_h(&app));
    }

    #[test]
    fn motion_counts() {
        let mut app = open_general();
        press(&mut app, "5k");
        assert_eq!(view(&app, &general()).cursor, 54);
        press(&mut app, "5gg");
        assert_eq!(view(&app, &general()).cursor, 4);
        press(&mut app, "G");
        press(&mut app, "5G");
        assert_eq!(view(&app, &general()).cursor, 4);
        press(&mut app, "5j");
        assert_eq!(view(&app, &general()).cursor, 9);
        press(&mut app, "9999j");
        assert_eq!(view(&app, &general()).cursor, 59);
        press(&mut app, "9999gg");
        assert_eq!(view(&app, &general()).cursor, 59);
    }

    #[test]
    fn motion_half_page() {
        let mut app = open_general();
        let starts = row_starts(&app, &general());
        let half = (viewport_h(&app) / 2).max(1);
        assert_eq!(half, 8, "17 inner rows at 80x24");

        press(&mut app, "gg");
        press(&mut app, "<C-d>");
        let down = view(&app, &general());
        assert_eq!(down.cursor, containing(&starts, half));
        assert!(down.cursor > 0);
        // The window moved down by the half page, ending aligned to the cursor message's
        // first row when that message begins above the new top.
        assert!(down.top_row > 0 && down.top_row <= half, "{down:?}");
        assert_eq!(down.top_row, starts[down.cursor]);
        assert_cursor_visible(&app, down);

        press(&mut app, "<C-u>");
        let up = view(&app, &general());
        assert!(up.cursor < down.cursor, "{up:?} {down:?}");
        assert!(up.top_row <= down.top_row);
        assert_cursor_visible(&app, up);

        press(&mut app, "gg");
        press(&mut app, "3<C-d>");
        let three = view(&app, &general());
        assert_eq!(three.cursor, containing(&starts, 3 * half));
        assert_cursor_visible(&app, three);

        press(&mut app, "99<C-d>");
        let bottom = view(&app, &general());
        assert_eq!(bottom.cursor, 59);
        assert_cursor_visible(&app, bottom);
        press(&mut app, "<C-d>");
        assert_eq!(view(&app, &general()), bottom);

        press(&mut app, "99<C-u>");
        let top = view(&app, &general());
        assert_eq!((top.cursor, top.top_row), (0, 0));
    }

    #[test]
    fn long_message_aligns_top() {
        let mut app = open_general();
        update(&mut app, Action::Resize(80, 12));
        let starts = row_starts(&app, &general());
        let vh = viewport_h(&app);
        assert!(starts[31] - starts[30] > vh, "long message must be taller");
        press(&mut app, "31gg");
        let v = view(&app, &general());
        assert_eq!(v.cursor, 30);
        assert_eq!(v.top_row, starts[30]);
        // Moving down keeps the long message reachable and then leaves it.
        press(&mut app, "j");
        let v = view(&app, &general());
        assert_eq!(v.cursor, 31);
        assert_cursor_visible(&app, v);
    }

    #[test]
    fn empty_room_noop() {
        let mut app = app_80x24();
        app.open_room(RoomId("quiet".to_string()));
        for keys in ["j", "k", "gg", "G", "<C-d>", "<C-u>", "5j", "5gg", "h", "l"] {
            press(&mut app, keys);
        }
        assert!(app.msg_views.is_empty());
        let room = RoomId("quiet".to_string());
        assert_eq!(view(&app, &room), MsgView::default());
    }

    #[test]
    fn no_room_noop() {
        let mut app = app_80x24();
        press(&mut app, "<C-w>l");
        assert_eq!(app.focus, Focus::Messages);
        assert_eq!(app.current_room, None);
        for keys in ["j", "k", "gg", "G", "<C-d>", "<C-u>", "5j"] {
            press(&mut app, keys);
        }
        assert!(app.msg_views.is_empty());
    }

    #[test]
    fn resize_clamps_view() {
        let mut app = open_general();
        press(&mut app, "G");
        update(&mut app, Action::Resize(200, 60));
        let v = view(&app, &general());
        assert_eq!(v.cursor, 59);
        assert_cursor_visible(&app, v);
        let starts = row_starts(&app, &general());
        assert!(v.top_row <= starts[60].saturating_sub(viewport_h(&app)));
    }

    #[test]
    fn resize_shrink_scrolls_into_view() {
        for (keys, expected) in [("G", 59), ("G20k", 39), ("gg", 0)] {
            for sizes in [[(80u16, 24u16), (50, 12)], [(80, 24), (40, 8)]] {
                let mut app = open_general();
                update(&mut app, Action::Resize(200, 60));
                press(&mut app, keys);
                assert_eq!(view(&app, &general()).cursor, expected, "{keys}");
                for (w, h) in sizes {
                    update(&mut app, Action::Resize(w, h));
                    let v = view(&app, &general());
                    assert_eq!(v.cursor, expected, "{keys} at {w}x{h}");
                    assert_cursor_visible(&app, v);
                }
            }
        }
    }

    #[test]
    fn rows_match_render_with_long_author() {
        let mut app = open_general();
        press(&mut app, "G");
        app.messages
            .get_mut(&general())
            .expect("general")
            .push(crate::data::Message {
                edited: false,
                plain_text: true,
                tmid: None,
                tshow: false,
                tcount: 0,
                tlm: None,
                reactions: Vec::new(),
                id: "long-author".to_string(),
                room_id: general(),
                author: "firstname.lastname.contractor.external".to_string(),
                ts: 0,
                body: "a fairly long message body that has to wrap on narrow panes".to_string(),
                kind: crate::data::MessageKind::Normal,
                send: crate::data::SendState::Sent,
            });
        for (w, h) in [(80u16, 24u16), (50, 12), (40, 8)] {
            update(&mut app, Action::Resize(w, h));
            let (width, _) = viewport(&app);
            let msgs = &app.messages[&general()];
            let ctx = app.row_ctx(msgs);
            let expected: usize = msgs
                .iter()
                .enumerate()
                .map(|(i, m)| wrap::block_rows(m, i.checked_sub(1).map(|p| &msgs[p]), width, &ctx))
                .sum();
            let starts = row_starts(&app, &general());
            assert_eq!(starts.last().copied(), Some(expected), "{w}x{h}");
            let v = view(&app, &general());
            assert_eq!(v.cursor, 59, "{w}x{h}");
            assert_cursor_visible(&app, v);
            press(&mut app, "G");
            let v = view(&app, &general());
            assert_eq!(v.cursor, 60, "{w}x{h}");
            assert_cursor_visible(&app, v);
            press(&mut app, "60gg");
        }
    }

    #[test]
    fn motion_h_l_noop() {
        let mut app = open_general();
        let before = view(&app, &general());
        for keys in ["h", "l", "5l", "3h"] {
            press(&mut app, keys);
            assert_eq!(view(&app, &general()), before, "{keys}");
            assert!(app.msg_views.is_empty(), "{keys} stored a view");
        }
        press(&mut app, "k");
        let moved = view(&app, &general());
        assert_ne!(moved, before);
        for keys in ["h", "l", "5l"] {
            press(&mut app, keys);
            assert_eq!(view(&app, &general()), moved, "{keys}");
        }
        assert_eq!(app.msg_views.len(), 1);
    }

    #[test]
    fn follow_newest_resets() {
        let mut app = open_general();
        let newest = view(&app, &general());
        press(&mut app, "gg");
        assert_ne!(view(&app, &general()), newest);
        follow_newest(&mut app, &general());
        assert_eq!(view(&app, &general()), newest);
        assert!(app.msg_views.is_empty());
    }

    #[test]
    fn is_at_bottom_cases() {
        // No stored view: the default newest view, at the bottom.
        let mut app = open_general();
        assert!(is_at_bottom(&app, &general()));
        // `gg` stores a view at the top of a long room: not at the bottom.
        press(&mut app, "gg");
        assert!(!is_at_bottom(&app, &general()));
        // `G` goes back to the last message: at the bottom again.
        press(&mut app, "G");
        assert!(is_at_bottom(&app, &general()));
        // A room with nothing in it, and a room that never had a view, are at the bottom.
        assert!(is_at_bottom(&app, &RoomId("quiet".to_string())));

        // A room shorter than the viewport is at the bottom with any stored view.
        let mut app = open_general();
        let room = general();
        let short: Vec<crate::data::Message> = app.messages[&room][..2].to_vec();
        app.messages.insert(room.clone(), short);
        app.msg_views.insert(
            room.clone(),
            MsgView {
                cursor: 0,
                top_row: 0,
            },
        );
        assert!(is_at_bottom(&app, &room));
    }

    fn server_copy(room: &RoomId, id: &str, body: &str) -> Message {
        Message {
            edited: false,
            plain_text: true,
            tmid: None,
            tshow: false,
            tcount: 0,
            tlm: None,
            reactions: Vec::new(),
            id: id.to_string(),
            room_id: room.clone(),
            author: "smoke".to_string(),
            ts: 1_794_909_605,
            body: body.to_string(),
            kind: crate::data::MessageKind::Normal,
            send: SendState::Sent,
        }
    }

    #[test]
    fn merge_incoming_absent_appends() {
        let mut app = open_general();
        let room = general();
        let before = app.messages[&room].len();
        let outcome = merge_incoming(&mut app, &room, server_copy(&room, "srv-1", "hello"));
        assert_eq!(outcome, Merge::Appended);
        let held = &app.messages[&room];
        assert_eq!(held.len(), before + 1);
        assert_eq!(held[before].id, "srv-1");
        assert_eq!(held[before].send, SendState::Sent);

        // A room with no vector gets one.
        let fresh = RoomId("fresh".to_string());
        assert!(!app.messages.contains_key(&fresh));
        let outcome = merge_incoming(&mut app, &fresh, server_copy(&fresh, "srv-2", "x"));
        assert_eq!(outcome, Merge::Appended);
        assert_eq!(app.messages[&fresh].len(), 1);
        assert!(app.awaiting_answer.is_empty());
    }

    #[test]
    fn merge_incoming_sent_ignored() {
        let mut app = open_general();
        let room = general();
        let held_id = app.messages[&room][5].id.clone();
        let before = app.messages[&room].clone();
        let outcome = merge_incoming(
            &mut app,
            &room,
            server_copy(&room, &held_id, "a different body"),
        );
        assert_eq!(outcome, Merge::Ignored);
        assert_eq!(app.messages[&room], before);
        assert!(app.awaiting_answer.is_empty());
    }

    #[test]
    fn merge_incoming_pending_confirms() {
        for pending in [
            SendState::Queued { reconcile: false },
            SendState::Sending,
            SendState::Failed,
            SendState::Unconfirmed,
        ] {
            let mut app = open_general();
            let room = general();
            let mut held = server_copy(&room, "local-9", "typed");
            held.author = "me".to_string();
            held.ts = 1;
            held.send = pending;
            app.messages.get_mut(&room).expect("room").push(held);
            let index = app.messages[&room].len() - 1;
            let view = MsgView {
                cursor: 5,
                top_row: 2,
            };
            app.msg_views.insert(room.clone(), view);
            let len = app.messages[&room].len();

            let outcome = merge_incoming(&mut app, &room, server_copy(&room, "local-9", "typed"));
            assert_eq!(outcome, Merge::Confirmed(index), "{pending:?}");
            let held = &app.messages[&room];
            assert_eq!(held.len(), len, "{pending:?}: replaced, not appended");
            assert_eq!(held[index].id, "local-9");
            assert_eq!(
                held[index].author, "smoke",
                "{pending:?}: the server's copy"
            );
            assert_eq!(held[index].send, SendState::Sent, "{pending:?}");
            assert_eq!(app.msg_views[&room], view, "{pending:?}: cursor unchanged");
            // Only a confirmed Sending copy keeps the in-flight slot (D-03).
            let expected = (pending == SendState::Sending).then(|| "local-9".to_string());
            assert_eq!(
                app.awaiting_answer.get(&room),
                expected.as_ref(),
                "{pending:?}"
            );
        }
    }

    #[test]
    fn focus_routing() {
        let mut app = open_general();
        let selected = crate::app::sidebar::selected_room(&app);
        press(&mut app, "5gg");
        assert_eq!(view(&app, &general()).cursor, 4);
        assert_eq!(crate::app::sidebar::selected_room(&app), selected);

        press(&mut app, "G");
        assert_eq!(view(&app, &general()).cursor, 59);
        press(&mut app, "<C-w>h");
        assert_eq!(app.focus, Focus::Sidebar);
        press(&mut app, "<C-w>l");
        assert_eq!(app.focus, Focus::Messages);
        press(&mut app, "gg");
        assert_eq!(view(&app, &general()).cursor, 0);
        assert_eq!(crate::app::sidebar::selected_room(&app), selected);
    }
}
