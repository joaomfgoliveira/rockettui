//! Following a thread (`thread-follow`): the toggle, its answer and the state it changes.
//!
//! The state is the server's: a parent whose follower list holds the user's own id is followed. A
//! toggle sends one request and changes the held lists only when the server accepts it, so a
//! failure has nothing to revert. Status lines hold a fixed reason, never server or message text.

use crate::api::ApiError;
use crate::app::{App, Focus, Mode, StatusMsg, send, thread};
use crate::data::RoomId;
use crate::effect::Effect;

/// `toggle_follow_thread`: follow the thread of the message under the cursor, or the open thread
/// when the thread pane is focused, or unfollow it when the user already follows it.
pub fn toggle(app: &mut App) {
    if app.mode != Mode::Normal {
        return;
    }
    let Some(room) = app.current_room.clone() else {
        return;
    };
    let parent_id = match app.focus {
        Focus::Thread => match app.thread.as_ref() {
            Some(pane) => pane.parent_id.clone(),
            None => return,
        },
        Focus::Messages => match thread::under_cursor(app) {
            Some(parent_id) => parent_id,
            None => {
                app.status_msg = Some(StatusMsg::Info("No thread on this message".to_string()));
                return;
            }
        },
        _ => return,
    };
    if app.follow_pending {
        return;
    }
    let Some(own_id) = app.own_user_id.clone() else {
        app.status_msg = Some(StatusMsg::Error(
            "User id not loaded yet: try again in a moment".to_string(),
        ));
        return;
    };
    let followed =
        thread::held_message(app, &room, &parent_id).is_some_and(|m| m.followed_by(&own_id));
    app.follow_pending = true;
    app.effects.push(Effect::FollowThread {
        room,
        parent_id,
        follow: !followed,
    });
}

/// The server accepted the request: the user is now in (or out of) the follower list of every held
/// copy of the parent, and the next press may go out.
pub fn on_done(app: &mut App, room: &RoomId, parent_id: &str, follow: bool) {
    app.follow_pending = false;
    let Some(own_id) = app.own_user_id.clone() else {
        return;
    };
    let pane = app
        .thread
        .iter_mut()
        .filter(|p| &p.room == room)
        .flat_map(|p| p.messages.iter_mut());
    let main = app.messages.get_mut(room).into_iter().flatten();
    for parent in main.chain(pane).filter(|m| m.id == parent_id) {
        let mut followers = parent.followers.take().unwrap_or_default();
        followers.retain(|id| *id != own_id);
        if follow {
            followers.push(own_id.clone());
        }
        parent.followers = Some(followers);
    }
}

/// The request failed: nothing changed, and the status line says which way and why.
pub fn on_failed(app: &mut App, follow: bool, error: ApiError) {
    app.follow_pending = false;
    let what = if follow { "follow" } else { "unfollow" };
    let lead = if error.outcome_unknown() {
        format!("thread may not have been {what}ed")
    } else {
        format!("thread not {what}ed")
    };
    app.status_msg = Some(StatusMsg::Error(format!(
        "{lead}: {}",
        send::error_reason(error)
    )));
}
