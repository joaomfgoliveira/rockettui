//! Table-driven `:` commands (D-16). Later phases add rows to `COMMANDS`.

use crate::app::{App, ConnState, RtState, StatusMsg, history};
use crate::effect::Effect;

pub struct CommandDef {
    pub names: &'static [&'static str],
    /// One short line for the help overlay.
    pub desc: &'static str,
    pub run: fn(&mut App, &[&str]) -> Result<(), String>,
}

pub const COMMANDS: &[CommandDef] = &[
    CommandDef {
        names: &["q", "quit"],
        desc: "quit (warns once if messages are undelivered)",
        run: cmd_quit,
    },
    CommandDef {
        names: &["q!", "quit!"],
        desc: "quit at once",
        run: cmd_force_quit,
    },
    CommandDef {
        names: &["reconnect", "retry"],
        desc: "reload rooms and restart the live connection",
        run: cmd_reconnect,
    },
    CommandDef {
        names: &["help"],
        desc: "show this help",
        run: cmd_help,
    },
];

/// `:q` / `:quit` — leaves the app, but warns once while messages are undelivered (D-20): the
/// first `:q` only shows how many there are, the next one quits. The text is built from the count
/// alone, so it never holds a message, an author or a room name (D-14).
pub fn cmd_quit(app: &mut App, _args: &[&str]) -> Result<(), String> {
    let count = app.undelivered_count();
    if count == 0 || app.quit_warned {
        app.should_quit = true;
        return Ok(());
    }
    app.quit_warned = true;
    let noun = if count == 1 { "message" } else { "messages" };
    Err(format!(
        "{count} {noun} not delivered: :q again or :q! to quit"
    ))
}

/// `:q!` / `:quit!` (D-20): leave at once. Undelivered messages live in memory only and are lost.
pub fn cmd_force_quit(app: &mut App, _args: &[&str]) -> Result<(), String> {
    app.should_quit = true;
    Ok(())
}

/// `:reconnect` / `:retry` (D-06, D-16): load the rooms again, re-learn the own username (D-17) and
/// retry every history page that failed. This and reopening a room are the only retries; the
/// reducer never retries by itself, so a broken token cannot hammer the server. It never touches a
/// pending, failed or unconfirmed send and sends nothing (D-08, as clarified by D-20). It also
/// restarts the realtime connection (`App.restart_realtime`).
pub fn cmd_reconnect(app: &mut App, _args: &[&str]) -> Result<(), String> {
    app.conn = ConnState::Connecting;
    app.effects.push(Effect::LoadRooms);
    app.effects.push(Effect::LoadMe);
    history::retry_failed(app);
    // The realtime connection restarts too (`realtime-ddp`): its reconnect refetches the open room.
    app.restart_realtime = true;
    if app.rt == RtState::Unavailable {
        app.rt = RtState::Reconnecting;
    }
    Ok(())
}

/// `:help`: open the help overlay (the app is already back in Normal mode).
pub fn cmd_help(app: &mut App, _args: &[&str]) -> Result<(), String> {
    app.help = Some(0);
    Ok(())
}

/// Run one command line (without the leading `:`).
pub fn execute(app: &mut App, line: &str) {
    let mut tokens = line.split_whitespace();
    let Some(name) = tokens.next() else {
        return;
    };
    let args: Vec<&str> = tokens.collect();
    // Only a `:q` straight after the warning quits through it (D-20): any other command disarms
    // it. `q!` and `quit!` quit anyway, and an empty line (returned above) leaves the arm alone.
    if !matches!(name, "q" | "quit") {
        app.quit_warned = false;
    }
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
                edited: false,
                plain_text: true,
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

    /// A fake-data app with `general` open and one undelivered message in it.
    fn app_with_failed() -> App {
        let mut app = crate::test_util::app_80x24();
        app.open_room(rid("general"));
        app.messages
            .get_mut(&rid("general"))
            .expect("general")
            .push(Message {
                edited: false,
                plain_text: true,
                id: "u-1".to_string(),
                room_id: rid("general"),
                author: "me".to_string(),
                ts: 0,
                body: "SENTINEL-BODY-7f3a".to_string(),
                kind: MessageKind::Normal,
                send: SendState::Failed,
            });
        app
    }

    fn status_text(app: &App) -> String {
        match &app.status_msg {
            Some(StatusMsg::Error(m)) | Some(StatusMsg::Info(m)) => m.clone(),
            None => String::new(),
        }
    }

    #[test]
    fn quit_warns_then_quits() {
        let mut app = app_with_failed();
        press(&mut app, ":q<CR>");
        assert!(!app.should_quit);
        assert_eq!(
            status_text(&app),
            "1 message not delivered: :q again or :q! to quit"
        );
        assert!(!status_text(&app).contains("SENTINEL"));
        press(&mut app, ":q<CR>");
        assert!(app.should_quit);
    }

    #[test]
    fn force_quit_skips_warning() {
        let mut app = app_with_failed();
        press(&mut app, ":q!<CR>");
        assert!(app.should_quit);
        let mut fresh = app_with_failed();
        press(&mut fresh, ":quit!<CR>");
        assert!(fresh.should_quit);
    }

    fn push_message(app: &mut App, room: &str, n: usize, send: SendState) {
        app.messages
            .get_mut(&rid(room))
            .expect("room")
            .push(Message {
                edited: false,
                plain_text: true,
                id: format!("w-{n}"),
                room_id: rid(room),
                author: "me".to_string(),
                ts: 0,
                body: "text".to_string(),
                kind: MessageKind::Normal,
                send,
            });
    }

    #[test]
    fn warning_counts_states_and_rooms() {
        let mut app = crate::test_util::app_80x24();
        let states = [
            ("general", SendState::Queued { reconcile: false }),
            ("random", SendState::Sending),
            ("general", SendState::Failed),
            ("random", SendState::Unconfirmed),
            ("general", SendState::Sent),
            ("random", SendState::Sent),
        ];
        for (n, (room, send)) in states.into_iter().enumerate() {
            push_message(&mut app, room, n, send);
        }
        press(&mut app, ":q<CR>");
        assert!(!app.should_quit);
        assert_eq!(
            status_text(&app),
            "4 messages not delivered: :q again or :q! to quit"
        );
    }

    #[test]
    fn other_command_disarms() {
        let mut app = app_with_failed();
        press(&mut app, ":q<CR>");
        assert!(app.quit_warned);
        press(&mut app, ":foo<CR>");
        press(&mut app, ":q<CR>");
        assert!(!app.should_quit, "an unknown command disarms the warning");
        assert_eq!(
            status_text(&app),
            "1 message not delivered: :q again or :q! to quit"
        );

        let mut app = app_with_failed();
        press(&mut app, ":q<CR>");
        press(&mut app, ":reconnect<CR>");
        press(&mut app, ":q<CR>");
        assert!(!app.should_quit, ":reconnect disarms the warning");
        press(&mut app, ":q<CR>");
        assert!(app.should_quit);
    }

    #[test]
    fn empty_line_keeps_warning_armed() {
        let mut app = app_with_failed();
        press(&mut app, ":q<CR>");
        press(&mut app, ":<CR>");
        press(&mut app, ":q<CR>");
        assert!(app.should_quit);
    }

    #[test]
    fn warning_holds_no_text() {
        let mut app = crate::test_util::app_80x24();
        app.rooms[0].name = "SENTINEL-ROOM".to_string();
        let id = app.rooms[0].id.clone();
        app.messages.get_mut(&id).expect("room").push(Message {
            edited: false,
            plain_text: true,
            id: "s-1".to_string(),
            room_id: id,
            author: "SENTINEL-AUTHOR-2b9d".to_string(),
            ts: 0,
            body: "SENTINEL-BODY-7f3a".to_string(),
            kind: MessageKind::Normal,
            send: SendState::Unconfirmed,
        });
        press(&mut app, ":q<CR>");
        let shown = status_text(&app);
        assert!(shown.contains("not delivered"), "{shown}");
        for sentinel in [
            "SENTINEL-BODY-7f3a",
            "SENTINEL-AUTHOR-2b9d",
            "SENTINEL-ROOM",
        ] {
            assert!(!shown.contains(sentinel), "{shown}");
        }
    }

    #[test]
    fn quit_warning_leaves_sends() {
        let mut app = app_with_failed();
        push_message(
            &mut app,
            "general",
            1,
            SendState::Queued { reconcile: true },
        );
        push_message(&mut app, "random", 2, SendState::Sending);
        let before = app.messages.clone();
        let effects = app.effects.len();
        press(&mut app, ":q<CR>");
        assert!(!app.should_quit);
        assert_eq!(app.messages, before, "no state, order or row changes");
        assert_eq!(app.effects.len(), effects, "no effect is recorded");
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
