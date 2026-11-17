//! Per-room history state and the decisions about when to fetch (D-11, D-12, D-13, D-16).
//!
//! The functions here are pure: they read and change `App` and push `Effect`s onto `app.effects`,
//! and never do IO. The run loop turns the effects into requests and the answers come back as
//! `Action::HistoryLoaded` and `Action::HistoryFailed`.
//!
//! Rules this module keeps:
//! - an encrypted room is never fetched and never decrypted (D-12, ROOM-03);
//! - a room is fetched on its first open only; a Loading or Loaded room is not fetched again
//!   (D-13), and nothing is prefetched;
//! - opening a room never marks it read on the server: every effect is a GET (D-11);
//! - only a Failed room is fetched again when it is reopened.

use crate::api::ApiError;
use crate::app::{App, StatusMsg};
use crate::data::{Message, RoomId};
use crate::effect::Effect;
use std::collections::HashSet;

/// Where a room's history stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryState {
    /// The first page was requested and has not arrived.
    Loading,
    /// The first page arrived. `oldest_ts` is the raw server timestamp of the oldest message held,
    /// the `latest` value of the next older page; `exhausted` is true once the server has no
    /// older message; `loading_older` is true while an older page is in flight.
    Loaded {
        oldest_ts: Option<String>,
        exhausted: bool,
        loading_older: bool,
    },
    /// The first page failed; reopening the room tries again.
    Failed,
}

/// A room was opened: decide whether its first page must be fetched. An unknown room does
/// nothing, an encrypted room does nothing (D-12), a room that is Loading or Loaded does nothing
/// (D-13), and a room with no state or a Failed one becomes Loading and records one
/// `Effect::LoadHistory` for the newest `history_size` messages.
pub fn on_open(app: &mut App, id: &RoomId) {
    let Some(room) = app.rooms.iter().find(|r| &r.id == id) else {
        return;
    };
    if room.encrypted {
        return;
    }
    if matches!(
        app.history.get(id),
        Some(HistoryState::Loading | HistoryState::Loaded { .. })
    ) {
        return;
    }
    let effect = Effect::LoadHistory {
        room: room.id.clone(),
        kind: room.kind,
        private: room.private,
        count: app.settings.history_size,
        before: None,
    };
    app.history.insert(id.clone(), HistoryState::Loading);
    app.effects.push(effect);
}

/// A page of history arrived (`messages` oldest first). The first page is merged in front of any
/// messages the room already holds (local sends), skipping ids already present; an empty first
/// page marks the room exhausted, since no older page can follow. An older page only clears
/// `loading_older` here (and sets `exhausted` when empty); plan 03-04 completes that branch.
pub fn on_loaded(
    app: &mut App,
    room: RoomId,
    messages: Vec<Message>,
    oldest_ts: Option<String>,
    older: bool,
) {
    if older {
        if let Some(HistoryState::Loaded {
            exhausted,
            loading_older,
            ..
        }) = app.history.get_mut(&room)
        {
            *loading_older = false;
            if messages.is_empty() {
                *exhausted = true;
            }
        }
        return;
    }

    let exhausted = messages.is_empty();
    let held = app.messages.entry(room.clone()).or_default();
    let have: HashSet<String> = held.iter().map(|m| m.id.clone()).collect();
    let mut merged: Vec<Message> = messages
        .into_iter()
        .filter(|m| !have.contains(&m.id))
        .collect();
    let added = merged.len();
    merged.append(held);
    *held = merged;
    // A stored view points at an index; the new messages sit in front of it.
    if let Some(view) = app.msg_views.get_mut(&room) {
        view.cursor += added;
        view.top_row = 0;
    }
    app.history.insert(
        room,
        HistoryState::Loaded {
            oldest_ts,
            exhausted,
            loading_older: false,
        },
    );
}

/// A history request failed. The first page leaves the room Failed (reopening retries); an older
/// page only clears `loading_older`. Both show the fixed error line in the status bar.
pub fn on_failed(app: &mut App, room: RoomId, older: bool, error: ApiError) {
    if older {
        if let Some(HistoryState::Loaded { loading_older, .. }) = app.history.get_mut(&room) {
            *loading_older = false;
        }
    } else {
        app.history.insert(room, HistoryState::Failed);
    }
    app.status_msg = Some(StatusMsg::Error(error.to_string()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::api::map;
    use crate::api::wire::{HistoryResp, RoomsResp, SubscriptionsResp};
    use crate::app::update::update;
    use crate::data::RoomKind;
    use crate::test_util::{app_80x24, press, render_to_string};
    use ratatui::layout::Size;

    const SUBS: &str = include_str!("../api/fixtures/subscriptions.json");
    const ROOMS: &str = include_str!("../api/fixtures/rooms.json");
    const HISTORY: &str = include_str!("../api/fixtures/history.json");

    fn rid(name: &str) -> RoomId {
        RoomId(name.to_string())
    }

    /// An app with the fixture rooms loaded through the reducer, as the server would deliver them.
    fn fixture_app() -> App {
        let subs = serde_json::from_str::<SubscriptionsResp>(SUBS)
            .expect("subscriptions")
            .update;
        let records = serde_json::from_str::<RoomsResp>(ROOMS)
            .expect("rooms")
            .update;
        let mut app = App::new(Size::new(80, 24));
        update(&mut app, Action::RoomsLoaded(map::rooms(&subs, &records)));
        app
    }

    fn history_messages(room: &str) -> Vec<Message> {
        let msgs = serde_json::from_str::<HistoryResp>(HISTORY)
            .expect("history")
            .messages;
        map::history(&rid(room), msgs).messages
    }

    fn loaded_action(room: &str) -> Action {
        Action::HistoryLoaded {
            room: rid(room),
            messages: history_messages(room),
            oldest_ts: Some("2026-11-16T09:59:00.000Z".to_string()),
            older: false,
        }
    }

    /// Put the sidebar cursor on `name` and open it with Enter.
    fn open(app: &mut App, name: &str) {
        press(app, "<C-w>hgg");
        let mut moves = 0;
        loop {
            if crate::app::sidebar::selected_room(app) == Some(rid(name)) {
                break;
            }
            press(app, "j");
            moves += 1;
            assert!(moves < 40, "{name} is not in the sidebar");
        }
        press(app, "<CR>");
    }

    #[test]
    fn first_open_issues_one_load() {
        let mut app = fixture_app();
        // general is the newest unread room, so it is the first selectable row.
        press(&mut app, "<CR>");
        assert_eq!(app.current_room, Some(rid("r-general")));
        assert_eq!(
            app.effects,
            [Effect::LoadHistory {
                room: rid("r-general"),
                kind: RoomKind::Channel,
                private: false,
                count: 50,
                before: None,
            }]
        );
        assert_eq!(
            app.history.get(&rid("r-general")),
            Some(&HistoryState::Loading)
        );

        app.effects.clear();
        press(&mut app, "<C-w>h");
        press(&mut app, "<CR>");
        assert_eq!(app.current_room, Some(rid("r-general")));
        assert!(
            app.effects.is_empty(),
            "a Loading room is not fetched again"
        );
    }

    #[test]
    fn effect_follows_room_kind_and_history_size() {
        let mut app = fixture_app();
        app.settings.history_size = 20;
        open(&mut app, "r-devteam");
        open(&mut app, "r-dm-alice");
        let kinds: Vec<(&str, RoomKind, bool, u16)> = app
            .effects
            .iter()
            .map(|e| match e {
                Effect::LoadHistory {
                    room,
                    kind,
                    private,
                    count,
                    ..
                } => (room.0.as_str(), *kind, *private, *count),
                other => panic!("unexpected effect {other:?}"),
            })
            .collect();
        assert_eq!(
            kinds,
            [
                ("r-devteam", RoomKind::PrivateGroup, true, 20),
                ("r-dm-alice", RoomKind::DirectMessage, false, 20),
            ]
        );
    }

    #[test]
    fn encrypted_never_loads() {
        let mut app = fixture_app();
        open(&mut app, "r-secrets");
        assert_eq!(app.current_room, Some(rid("r-secrets")));
        assert!(app.effects.is_empty(), "{:?}", app.effects);
        assert!(!app.history.contains_key(&rid("r-secrets")));
    }

    #[test]
    fn loaded_then_cached() {
        let mut app = fixture_app();
        press(&mut app, "<CR>");
        app.effects.clear();
        update(&mut app, loaded_action("r-general"));
        let held: Vec<&str> = app.messages[&rid("r-general")]
            .iter()
            .map(|m| m.id.as_str())
            .collect();
        assert_eq!(held, ["m1", "m2", "m3", "m4"]);
        assert_eq!(
            app.history.get(&rid("r-general")),
            Some(&HistoryState::Loaded {
                oldest_ts: Some("2026-11-16T09:59:00.000Z".to_string()),
                exhausted: false,
                loading_older: false,
            })
        );
        let screen = render_to_string(&app, 80, 24);
        assert!(screen.contains("fourth and newest message"), "{screen}");

        press(&mut app, "<C-w>h");
        press(&mut app, "<CR>");
        assert!(app.effects.is_empty(), "a Loaded room is not fetched again");
    }

    #[test]
    fn failed_reopen_retries() {
        let mut app = fixture_app();
        press(&mut app, "<CR>");
        app.effects.clear();
        update(
            &mut app,
            Action::HistoryFailed {
                room: rid("r-general"),
                older: false,
                error: ApiError::Timeout,
            },
        );
        assert_eq!(
            app.history.get(&rid("r-general")),
            Some(&HistoryState::Failed)
        );
        match &app.status_msg {
            Some(StatusMsg::Error(text)) => assert_eq!(text, &ApiError::Timeout.to_string()),
            _ => panic!("expected a status error"),
        }

        press(&mut app, "<C-w>h");
        press(&mut app, "<CR>");
        assert_eq!(
            app.history.get(&rid("r-general")),
            Some(&HistoryState::Loading)
        );
        assert_eq!(app.effects.len(), 1, "{:?}", app.effects);
    }

    #[test]
    fn empty_first_page() {
        let mut app = fixture_app();
        press(&mut app, "<CR>");
        update(
            &mut app,
            Action::HistoryLoaded {
                room: rid("r-general"),
                messages: Vec::new(),
                oldest_ts: None,
                older: false,
            },
        );
        assert_eq!(
            app.history.get(&rid("r-general")),
            Some(&HistoryState::Loaded {
                oldest_ts: None,
                exhausted: true,
                loading_older: false,
            })
        );
        let screen = render_to_string(&app, 80, 24);
        assert!(screen.contains("No messages yet"), "{screen}");
    }

    #[test]
    fn fake_rooms_never_load() {
        let mut app = app_80x24();
        press(&mut app, "<CR>");
        assert!(app.current_room.is_some());
        assert!(app.effects.is_empty(), "{:?}", app.effects);
    }

    #[test]
    fn first_page_merges_before_local_messages() {
        let mut app = fixture_app();
        press(&mut app, "<CR>");
        // A local send while the page is in flight, then the page arrives.
        press(&mut app, "ihello<CR><Esc>");
        update(&mut app, loaded_action("r-general"));
        let held: Vec<&str> = app.messages[&rid("r-general")]
            .iter()
            .map(|m| m.id.as_str())
            .collect();
        assert_eq!(held, ["m1", "m2", "m3", "m4", "local-1"]);
        // The same page twice never duplicates a message.
        update(&mut app, loaded_action("r-general"));
        assert_eq!(app.messages[&rid("r-general")].len(), 5);
    }

    #[test]
    fn older_page_only_clears_the_flag() {
        let mut app = fixture_app();
        app.history.insert(
            rid("r-general"),
            HistoryState::Loaded {
                oldest_ts: Some("x".to_string()),
                exhausted: false,
                loading_older: true,
            },
        );
        update(
            &mut app,
            Action::HistoryLoaded {
                room: rid("r-general"),
                messages: Vec::new(),
                oldest_ts: None,
                older: true,
            },
        );
        assert_eq!(
            app.history.get(&rid("r-general")),
            Some(&HistoryState::Loaded {
                oldest_ts: Some("x".to_string()),
                exhausted: true,
                loading_older: false,
            })
        );
    }
}
