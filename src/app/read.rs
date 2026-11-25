//! Marking the open room read (`rooms` spec). Pure: no IO, no clock.
//!
//! A room is marked read once it has stayed the open room for `mark_read_delay_ms`. The reducer
//! only records that a mark is wanted (`ReadState::pending`); `run()` in `lib.rs` arms a timer for
//! it exactly like the key timeout and sends `Action::MarkReadDue` when it fires. Leaving the room
//! drops the pending mark, and a new live message from someone else restarts it. Nothing is
//! requested for a room that is not unread, and a failed request is silent: the room just stays
//! unread until the next open or live message tries again.
//!
//! The same module owns the `unread` divider of the open room: it is placed once and never moves,
//! it outlives the mark-read (so it does not vanish mid-read) and is dropped when the room is left.

use crate::app::history::HistoryState;
use crate::app::{App, realtime};
use crate::data::{MessageKind, RoomId, UnreadUpdate};
use crate::effect::Effect;

/// A mark-read waiting for its timer. `generation` changes on every (re)start, so the loop can
/// tell a restart from the same wait and re-arm its timer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingRead {
    pub room: RoomId,
    pub generation: u64,
}

/// Where the `unread` divider of the open room goes. `before` is the id of the message it sits
/// above; it is set once (an id, not an index, so older pages loading above it never move it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnreadDivider {
    room: RoomId,
    /// The room's last-seen time when it was opened. It is not advanced when the room is marked
    /// read, so the divider stays while the room is open.
    last_seen: Option<i64>,
    /// The first page has been looked at: the divider is where it will be, or it waits for a live
    /// message.
    settled: bool,
    before: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReadState {
    pub pending: Option<PendingRead>,
    pub divider: Option<UnreadDivider>,
    generation: u64,
}

/// The id of the message the `unread` divider is above in `room`, if it is the open room's.
pub fn divider_before<'a>(app: &'a App, room: &RoomId) -> Option<&'a str> {
    app.read
        .divider
        .as_ref()
        .filter(|d| &d.room == room)
        .and_then(|d| d.before.as_deref())
}

fn is_unread(app: &App, room: &RoomId) -> bool {
    app.rooms
        .iter()
        .any(|r| &r.id == room && r.unread.is_unread())
}

/// Ask the server to mark `room` read, when it is unread.
fn send(app: &mut App, room: &RoomId) {
    if is_unread(app, room) {
        app.effects.push(Effect::MarkRead { room: room.clone() });
    }
}

/// Start (or restart) the wait for `room`, which must be the open room. With a delay of 0 the
/// request is sent at once.
fn restart(app: &mut App, room: &RoomId) {
    if app.current_room.as_ref() != Some(room) {
        return;
    }
    if app.settings.mark_read_delay_ms == 0 {
        app.read.pending = None;
        send(app, room);
        return;
    }
    app.read.generation += 1;
    app.read.pending = Some(PendingRead {
        room: room.clone(),
        generation: app.read.generation,
    });
}

/// The open room changed (called from `App::open_room`): the old wait is dropped, and an unread
/// room starts its own.
pub fn on_open(app: &mut App) {
    app.read.pending = None;
    app.read.divider = app.current_room.as_ref().map(|room| UnreadDivider {
        room: room.clone(),
        last_seen: app
            .rooms
            .iter()
            .find(|r| &r.id == room)
            .and_then(|r| r.last_seen),
        settled: false,
        before: None,
    });
    settle_divider(app);
    if let Some(room) = app.current_room.clone()
        && is_unread(app, &room)
    {
        restart(app, &room);
    }
}

/// Place the divider above the oldest message from someone else that is newer than the room's
/// last-seen time, once the first page of the open room is held. Without a last-seen time there
/// is nothing to compare with and the divider waits for a live message.
pub fn settle_divider(app: &mut App) {
    let Some(divider) = app.read.divider.as_ref() else {
        return;
    };
    if divider.settled || app.current_room.as_ref() != Some(&divider.room) {
        return;
    }
    if !matches!(
        app.history.get(&divider.room),
        Some(HistoryState::Loaded { .. })
    ) {
        return;
    }
    let before = divider.last_seen.and_then(|seen| {
        app.messages
            .get(&divider.room)?
            .iter()
            .find(|m| {
                m.ts > seen
                    && m.kind != MessageKind::Deleted
                    && !m.send.is_unsent()
                    && app.own_username.as_deref() != Some(m.author.as_str())
            })
            .map(|m| m.id.clone())
    });
    if let Some(divider) = app.read.divider.as_mut() {
        divider.settled = true;
        divider.before = before;
    }
}

/// A new message arrived live in `room`. A message from someone else restarts the wait when `room`
/// is the open one; the user's own messages never do.
pub fn on_live_message(app: &mut App, room: &RoomId, id: &str, author: &str) {
    if app.own_username.as_deref() == Some(author) {
        return;
    }
    // With nothing unread above it, a message from someone else is the first unread one.
    if let Some(divider) = app.read.divider.as_mut()
        && &divider.room == room
        && divider.before.is_none()
    {
        divider.before = Some(id.to_string());
    }
    restart(app, room);
}

/// The server flagged `room` unread. When it is the open room (a message whose flag arrives after
/// the message itself) the wait starts.
pub fn on_unread(app: &mut App, room: &RoomId) {
    restart(app, room);
}

/// The timer fired: send the request if the room it was for is still the open one and unread.
pub fn on_due(app: &mut App) {
    let Some(pending) = app.read.pending.take() else {
        return;
    };
    if app.current_room.as_ref() == Some(&pending.room) {
        send(app, &pending.room);
    }
}

/// The server marked `room` read: it leaves Unread now, without waiting for the realtime update.
pub fn on_done(app: &mut App, room: &RoomId) {
    // The room was read up to its newest held message: a later open compares against that, so the
    // divider is not drawn again for messages that were read.
    let newest = app.messages.get(room).and_then(|m| m.last()).map(|m| m.ts);
    if let (Some(newest), Some(held)) = (newest, app.rooms.iter_mut().find(|r| &r.id == room)) {
        held.last_seen = Some(held.last_seen.map_or(newest, |seen| seen.max(newest)));
    }
    realtime::on_subscription(
        app,
        room,
        UnreadUpdate {
            alert: Some(false),
            unread: Some(0),
            ..UnreadUpdate::default()
        },
    );
}

/// Keeps one timer in step with `ReadState::pending` for the event loop, which has the clock. It
/// is armed when a new wait appears (or restarts) and cleared when there is none.
#[derive(Debug, Default)]
pub struct ReadTimer {
    armed: Option<u64>,
    deadline: Option<tokio::time::Instant>,
}

impl ReadTimer {
    /// Bring the timer in line with `pending`, a wait that started "now".
    pub fn sync(
        &mut self,
        pending: Option<&PendingRead>,
        now: tokio::time::Instant,
        delay: std::time::Duration,
    ) {
        let wanted = pending.map(|p| p.generation);
        if wanted != self.armed {
            self.armed = wanted;
            self.deadline = wanted.map(|_| now + delay);
        }
    }

    pub fn deadline(&self) -> Option<tokio::time::Instant> {
        self.deadline
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::Instant;

    fn pending(generation: u64) -> PendingRead {
        PendingRead {
            room: RoomId("r".to_string()),
            generation,
        }
    }

    #[test]
    fn timer_arms_one_delay_after_the_wait_appears() {
        let mut timer = ReadTimer::default();
        let t0 = Instant::now();
        let delay = Duration::from_millis(3000);
        timer.sync(None, t0, delay);
        assert_eq!(timer.deadline(), None);
        timer.sync(Some(&pending(1)), t0, delay);
        assert_eq!(timer.deadline(), Some(t0 + delay));
        // The same wait seen again later does not push the deadline back.
        timer.sync(Some(&pending(1)), t0 + Duration::from_secs(1), delay);
        assert_eq!(timer.deadline(), Some(t0 + delay));
    }

    #[test]
    fn a_restart_moves_the_deadline_and_no_wait_clears_it() {
        let mut timer = ReadTimer::default();
        let t0 = Instant::now();
        let delay = Duration::from_millis(3000);
        timer.sync(Some(&pending(1)), t0, delay);
        let t1 = t0 + Duration::from_secs(2);
        timer.sync(Some(&pending(2)), t1, delay);
        assert_eq!(timer.deadline(), Some(t1 + delay));
        timer.sync(None, t1, delay);
        assert_eq!(timer.deadline(), None);
    }
}
