//! Table-driven `:` commands (D-16). Later phases add rows to `COMMANDS`.

use crate::app::{App, ConnState, StatusMsg, history};
use crate::effect::Effect;

pub struct CommandDef {
    pub names: &'static [&'static str],
    pub run: fn(&mut App, &[&str]) -> Result<(), String>,
}

pub const COMMANDS: &[CommandDef] = &[
    CommandDef {
        names: &["q", "quit"],
        run: cmd_quit,
    },
    CommandDef {
        names: &["reconnect", "retry"],
        run: cmd_reconnect,
    },
];

/// `:q` / `:quit` — the only way to leave the app.
pub fn cmd_quit(app: &mut App, _args: &[&str]) -> Result<(), String> {
    app.should_quit = true;
    Ok(())
}

/// `:reconnect` / `:retry` (D-06, D-16): load the rooms again, re-learn the own username (D-17) and
/// retry every history page that failed. This and reopening a room are the only retries; the
/// reducer never retries by itself, so a broken token cannot hammer the server. It never touches a
/// pending, failed or unconfirmed send and sends nothing (D-08, as clarified by D-20). Realtime
/// reconnect is Phase 5.
pub fn cmd_reconnect(app: &mut App, _args: &[&str]) -> Result<(), String> {
    app.conn = ConnState::Connecting;
    app.effects.push(Effect::LoadRooms);
    app.effects.push(Effect::LoadMe);
    history::retry_failed(app);
    Ok(())
}

/// Run one command line (without the leading `:`).
pub fn execute(app: &mut App, line: &str) {
    let mut tokens = line.split_whitespace();
    let Some(name) = tokens.next() else {
        return;
    };
    let args: Vec<&str> = tokens.collect();
    match COMMANDS.iter().find(|def| def.names.contains(&name)) {
        Some(def) => {
            if let Err(msg) = (def.run)(app, &args) {
                app.status_msg = Some(StatusMsg::Error(msg));
            }
        }
        None => {
            app.status_msg = Some(StatusMsg::Error(format!("Unknown command: {name}")));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::action::Action;
    use crate::api::ApiError;
    use crate::app::history::HistoryState;
    use crate::app::update::update;
    use crate::app::{App, ConnState, Mode, StatusMsg};
    use crate::data::{Message, MessageKind, Room, RoomId, RoomKind, SendState};
    use crate::effect::Effect;
    use crate::test_util::{app_80x24, press};
    use ratatui::layout::Size;

    #[test]
    fn colon_q_quits() {
        let mut app = app_80x24();
        press(&mut app, ":q<CR>");
        assert!(app.should_quit);
    }

    #[test]
    fn colon_quit_quits() {
        let mut app = app_80x24();
        press(&mut app, ":quit<CR>");
        assert!(app.should_quit);
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn unknown_command_errors() {
        let mut app = app_80x24();
        press(&mut app, ":foo<CR>");
        assert!(matches!(
            &app.status_msg,
            Some(StatusMsg::Error(msg)) if msg == "Unknown command: foo"
        ));
        assert_eq!(app.mode, Mode::Normal);
        assert!(!app.should_quit);
        press(&mut app, "j");
        assert!(app.status_msg.is_none());
    }

    #[test]
    fn empty_command_is_noop() {
        let mut app = app_80x24();
        press(&mut app, ":<CR>");
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.status_msg.is_none());
        assert!(!app.should_quit);
    }

    fn rid(id: &str) -> RoomId {
        RoomId(id.to_string())
    }

    fn room(id: &str) -> Room {
        Room {
            id: rid(id),
            name: id.to_string(),
            kind: RoomKind::Channel,
            encrypted: false,
            private: false,
            unread: false,
            last_activity: 0,
        }
    }

    fn history_effect(id: &str) -> Effect {
        Effect::LoadHistory {
            room: rid(id),
            kind: RoomKind::Channel,
            private: false,
            count: 50,
            before: None,
        }
    }

    /// An app whose room load failed with `error`, holding rooms r-a (open), r-b and r-c.
    fn failed_app(error: ApiError) -> App {
        let mut app = App::new(Size::new(80, 24));
        app.rooms = vec![room("r-a"), room("r-b"), room("r-c")];
        app.current_room = Some(rid("r-a"));
        app.conn = ConnState::Failed(error);
        app
    }

    #[test]
    fn reconnect_reloads() {
        let mut app = failed_app(ApiError::Unauthorized);
        press(&mut app, ":reconnect<CR>");
        assert_eq!(app.conn, ConnState::Connecting);
        assert_eq!(app.effects, [Effect::LoadRooms, Effect::LoadMe]);
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.status_msg.is_none());
        assert!(!app.should_quit);
    }

    #[test]
    fn retry_alias() {
        let mut app = failed_app(ApiError::Unauthorized);
        press(&mut app, ":retry<CR>");
        assert_eq!(app.conn, ConnState::Connecting);
        assert_eq!(app.effects, [Effect::LoadRooms, Effect::LoadMe]);
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.status_msg.is_none());
    }

    #[test]
    fn reconnect_retries_failed_history() {
        let mut app = failed_app(ApiError::Timeout);
        let loaded = HistoryState::Loaded {
            oldest_ts: None,
            exhausted: false,
            loading_older: false,
        };
        app.history.insert(rid("r-a"), HistoryState::Failed);
        app.history.insert(rid("r-b"), HistoryState::Failed);
        app.history.insert(rid("r-c"), loaded.clone());
        press(&mut app, ":reconnect<CR>");
        assert_eq!(
            app.history.get(&rid("r-b")),
            None,
            "a closed Failed room is forgotten"
        );
        assert_eq!(
            app.history.get(&rid("r-c")),
            Some(&loaded),
            "a Loaded room is not refetched"
        );
        assert_eq!(app.history.get(&rid("r-a")), Some(&HistoryState::Loading));
        assert_eq!(
            app.effects,
            [Effect::LoadRooms, Effect::LoadMe, history_effect("r-a")]
        );
    }

    #[test]
    fn reconnect_leaves_loading_and_loaded_rooms_alone() {
        let mut app = failed_app(ApiError::Timeout);
        app.history.insert(rid("r-a"), HistoryState::Loading);
        press(&mut app, ":reconnect<CR>");
        assert_eq!(app.history.get(&rid("r-a")), Some(&HistoryState::Loading));
        assert_eq!(app.effects, [Effect::LoadRooms, Effect::LoadMe]);
    }

    #[test]
    fn reconnect_loads_me() {
        let mut app = failed_app(ApiError::Timeout);
        app.history.insert(rid("r-a"), HistoryState::Failed);
        press(&mut app, ":reconnect<CR>");
        assert_eq!(
            app.effects,
            [Effect::LoadRooms, Effect::LoadMe, history_effect("r-a")]
        );
        assert!(
            !app.effects
                .iter()
                .any(|e| matches!(e, Effect::SendMessage { .. })),
            "{:?}",
            app.effects
        );
    }

    #[test]
    fn reconnect_leaves_sends() {
        let mut app = failed_app(ApiError::Timeout);
        let states = [
            SendState::Failed,
            SendState::Unconfirmed,
            SendState::Queued { reconcile: true },
            SendState::Sending,
        ];
        let rows: Vec<Message> = states
            .iter()
            .enumerate()
            .map(|(n, send)| Message {
                id: format!("rt-{n}"),
                room_id: rid("r-a"),
                author: "me".to_string(),
                ts: 0,
                body: "text".to_string(),
                kind: MessageKind::Normal,
                send: *send,
            })
            .collect();
        app.messages.insert(rid("r-a"), rows.clone());
        press(&mut app, ":reconnect<CR>");
        assert_eq!(app.messages[&rid("r-a")], rows, "D-08: no send is touched");
        assert!(
            !app.effects
                .iter()
                .any(|e| matches!(e, Effect::SendMessage { .. })),
            "nothing is sent: {:?}",
            app.effects
        );
    }

    #[test]
    fn quit_after_failure() {
        let mut app = failed_app(ApiError::Unreachable);
        press(&mut app, ":q<CR>");
        assert!(app.should_quit);
    }

    #[test]
    fn no_auto_retry() {
        let mut app = failed_app(ApiError::Unauthorized);
        app.history.insert(rid("r-a"), HistoryState::Loading);
        update(&mut app, Action::LoadFailed(ApiError::RateLimited));
        assert_eq!(app.conn, ConnState::Failed(ApiError::RateLimited));
        update(
            &mut app,
            Action::HistoryFailed {
                room: rid("r-a"),
                older: false,
                error: ApiError::RateLimited,
            },
        );
        assert_eq!(app.history.get(&rid("r-a")), Some(&HistoryState::Failed));
        assert!(app.effects.is_empty(), "{:?}", app.effects);
    }
}
