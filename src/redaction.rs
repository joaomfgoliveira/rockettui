//! The single D-07 redaction sweep (AUTH-03, T-03-24, T-03-25, T-03-26).
//!
//! Compiled only under `cfg(test)`. It builds one value of every Phase 3 type that holds a secret
//! or message text, with sentinel contents, through the real constructors, and checks that no
//! `Debug` or `Display` output shows a sentinel. It also draws hostile server text and checks that
//! no control or bidi character reaches a cell.
//!
//! Every new type that holds a secret or message text must be added to `sentinel_sweep` here.

use crate::action::Action;
use crate::api::map;
use crate::api::wire::HistoryResp;
use crate::api::{ApiError, Client, HistoryPage};
use crate::app::App;
use crate::app::update::update;
use crate::config::connection::{
    ConnEnv, Connection, EnvValue, FileConn, Secret, parse_file, parse_server_url,
};
use crate::config::{Sources, load_from_sources};
use crate::data::{
    Folder, FolderId, Message, MessageKind, Room, RoomId, RoomKind, SendState, Unread,
};
use crate::effect::{Effect, SendText};
use crate::test_util::{press, render_to_string};
use ratatui::layout::Size;

const TOKEN: &str = "SENTINEL-TOKEN-9c41";
const USER: &str = "SENTINEL-USER";
const PASS: &str = "SENTINEL-PASS";
const BODY: &str = "SENTINEL-BODY-7f3a";
const SENTINELS: [&str; 4] = [TOKEN, USER, PASS, BODY];
/// The user name and password of the one URL that carries credentials.
const USERINFO_URL: &str = "https://SENTINEL-USER:SENTINEL-PASS@h/";

fn env_set(value: &str) -> EnvValue {
    EnvValue::Set(Secret::new(value.to_string()))
}

fn conn_env(url: &str) -> ConnEnv {
    ConnEnv {
        server_url: env_set(url),
        user_id: env_set(USER),
        token: env_set(TOKEN),
    }
}

/// A resolved connection holding the sentinel user id and token.
fn connection() -> Connection {
    let sources = Sources {
        env: conn_env("http://127.0.0.1:3100"),
        require_connection: true,
        ..Sources::default()
    };
    load_from_sources(&sources)
        .unwrap_or_else(|report| panic!("unexpected config errors: {report}"))
        .connection
        .expect("a connection")
}

/// `Sources` whose config.toml holds all three credentials and `extra` lines.
fn sources_with(extra: &str) -> Sources {
    Sources {
        config: Some(format!(
            "server_url = \"http://127.0.0.1:3100\"\nuser_id = \"{USER}\"\ntoken = \"{TOKEN}\"\n{extra}"
        )),
        require_connection: true,
        ..Sources::default()
    }
}

/// A config that fails to load, as the report the binary prints on stderr.
fn failing_report(sources: &Sources) -> crate::config::Report {
    match load_from_sources(sources) {
        Ok(_) => panic!("the config was expected to fail"),
        Err(report) => report,
    }
}

fn no_sentinel(label: &str, shown: &str) {
    for sentinel in SENTINELS {
        assert!(
            !shown.contains(sentinel),
            "{label} shows {sentinel}: {shown}"
        );
    }
}

/// Check both `Debug` and `Display` of a value that has both.
macro_rules! sweep_both {
    ($label:expr, $value:expr) => {{
        no_sentinel(concat!($label, " (Debug)"), &format!("{:?}", $value));
        no_sentinel(concat!($label, " (Display)"), &format!("{}", $value));
    }};
}

fn message_with_body() -> Message {
    message_in_state(SendState::Sent)
}

/// A message holding the sentinel body, in the given send state.
fn message_in_state(send: SendState) -> Message {
    Message {
        edited: false,
        plain_text: true,
        tmid: None,
        tshow: false,
        tcount: 0,
        tlm: None,
        followers: None,
        refs: None,
        reactions: Vec::new(),
        id: "m1".to_string(),
        room_id: RoomId("r-general".to_string()),
        author: "alice".to_string(),
        ts: 0,
        body: BODY.to_string(),
        kind: MessageKind::Normal,
        send,
    }
}

/// Every `SendState` value; a message in any of them must keep its text out of `Debug` (D-14).
const ALL_SEND_STATES: [SendState; 6] = [
    SendState::Sent,
    SendState::Queued { reconcile: false },
    SendState::Queued { reconcile: true },
    SendState::Sending,
    SendState::Failed,
    SendState::Unconfirmed,
];

const ALL_ERRORS: [ApiError; 13] = [
    ApiError::Unauthorized,
    ApiError::Forbidden,
    ApiError::NotFound,
    ApiError::RateLimited,
    ApiError::Redirected,
    ApiError::Server(502),
    ApiError::Unreachable,
    ApiError::Timeout,
    ApiError::BadResponse,
    ApiError::TooLarge,
    ApiError::Client,
    ApiError::Rejected,
    ApiError::ConnectionLost,
];

#[test]
fn sentinel_sweep() {
    // Config layer: the credentials in every wrapper they pass through.
    no_sentinel("Secret", &format!("{:?}", Secret::new(TOKEN.to_string())));
    no_sentinel("EnvValue::Set", &format!("{:?}", env_set(TOKEN)));
    no_sentinel("ConnEnv", &format!("{:?}", conn_env("http://h/")));
    no_sentinel("Connection", &format!("{:?}", connection()));
    let sources = sources_with("");
    no_sentinel("Sources", &format!("{sources:?}"));
    let mut sink = Vec::new();
    let file: FileConn = parse_file(sources.config.as_deref(), &mut sink);
    assert!(file.token.is_some(), "the file token was read");
    no_sentinel("FileConn", &format!("{file:?}"));

    // A report from a config holding the token plus an error, and the wrong-type and userinfo
    // variants of the credential keys.
    for extra in [
        "sidebar_width = 99\n",
        "history_size = \"many\"\n",
        "sidebar_width = 99\nunknown_key_with_text = 1\n",
    ] {
        let report = failing_report(&sources_with(extra));
        sweep_both!("Report", report);
    }
    let wrong_types = Sources {
        config: Some(format!("user_id = 7\ntoken = 8\n# {TOKEN}\n")),
        require_connection: true,
        ..Sources::default()
    };
    sweep_both!("Report (wrong types)", failing_report(&wrong_types));
    let userinfo_in_file = Sources {
        config: Some(format!(
            "server_url = \"{USERINFO_URL}\"\nuser_id = \"{USER}\"\ntoken = \"{TOKEN}\"\n"
        )),
        require_connection: true,
        ..Sources::default()
    };
    sweep_both!("Report (userinfo URL)", failing_report(&userinfo_in_file));

    // API layer.
    for error in ALL_ERRORS {
        sweep_both!("ApiError", error);
    }
    let client = Client::new(&connection()).expect("client");
    no_sentinel("api::Client", &format!("{client:?}"));
    let page = HistoryPage {
        messages: vec![message_with_body()],
        oldest_ts: Some("2026-11-16T09:59:00.000Z".to_string()),
    };
    no_sentinel("HistoryPage", &format!("{page:?}"));
    let thread_row = crate::api::ThreadRow {
        id: "t1".to_string(),
        author: "ana".to_string(),
        first_line: BODY.to_string(),
        replies: 1,
        last_reply: None,
        followers: Vec::new(),
    };
    no_sentinel("ThreadRow", &format!("{thread_row:?}"));
    let list_page = crate::api::ThreadListPage {
        rows: vec![thread_row],
        total: 1,
    };
    no_sentinel("ThreadListPage", &format!("{list_page:?}"));
    no_sentinel("Message", &format!("{:?}", message_with_body()));
    for (kind, edited) in [
        (MessageKind::Normal, true),
        (MessageKind::Deleted, false),
        (MessageKind::Deleted, true),
    ] {
        let message = Message {
            kind,
            edited,
            ..message_with_body()
        };
        no_sentinel("Message (edited or deleted)", &format!("{message:?}"));
        no_sentinel(
            "RtMessage (edited or deleted)",
            &format!("{:?}", Action::RtMessage(message)),
        );
    }
    for state in ALL_SEND_STATES {
        no_sentinel(
            "Message (send state)",
            &format!("{:?}", message_in_state(state)),
        );
    }

    // Realtime layer: the login frame holds the token and a stream frame holds message text.
    let login = crate::realtime::ddp::LoginFrame::new(&Secret::new(TOKEN.to_string()));
    no_sentinel("LoginFrame", &format!("{login:?}"));
    let rt = crate::realtime::RtConfig::from_connection(&connection()).expect("realtime config");
    no_sentinel("RtConfig", &format!("{rt:?}"));
    no_sentinel(
        "Action::RtMessage",
        &format!("{:?}", Action::RtMessage(message_with_body())),
    );
    let frame = crate::realtime::ddp::parse(&format!(
        "{{\"msg\":\"changed\",\"collection\":\"c\",\"fields\":{{\"eventName\":\"e\",\"args\":[{{\"msg\":\"{BODY}\"}}]}}}}"
    ))
    .expect("a frame");
    no_sentinel("ServerFrame", &format!("{frame:?}"));

    // Effects and actions.
    let effect = Effect::LoadHistory {
        room: RoomId("r-general".to_string()),
        kind: RoomKind::Channel,
        private: false,
        count: 50,
        before: Some("2026-11-16T09:59:00.000Z".to_string()),
    };
    no_sentinel("Effect::LoadHistory", &format!("{effect:?}"));
    no_sentinel("Effect::LoadRooms", &format!("{:?}", Effect::LoadRooms));
    no_sentinel("Effect::LoadMe", &format!("{:?}", Effect::LoadMe));
    // The send path: the text of a message being sent, and its effect and results (D-14).
    no_sentinel(
        "SendText",
        &format!("{:?}", SendText::new(BODY.to_string())),
    );
    let send = Effect::SendMessage {
        room: RoomId("r-general".to_string()),
        id: "rt-test-1".to_string(),
        text: SendText::new(BODY.to_string()),
        reconcile: true,
        tmid: None,
        tshow: false,
    };
    no_sentinel("Effect::SendMessage", &format!("{send:?}"));
    // Edit and delete: the new text, their effects and every result Action.
    let edit = Effect::EditMessage {
        room: RoomId("r-general".to_string()),
        id: "m1".to_string(),
        text: SendText::new(BODY.to_string()),
    };
    no_sentinel("Effect::EditMessage", &format!("{edit:?}"));
    let delete = Effect::DeleteMessage {
        room: RoomId("r-general".to_string()),
        id: "m1".to_string(),
    };
    no_sentinel("Effect::DeleteMessage", &format!("{delete:?}"));
    // Reacting: the shortcode, the effects and every result Action. A reaction name and a custom
    // emoji name are never printed.
    let react = Effect::React {
        id: "m1".to_string(),
        emoji: crate::effect::EmojiName::new(BODY.to_string()),
        add: true,
    };
    no_sentinel("Effect::React", &format!("{react:?} {react:#?}"));
    no_sentinel(
        "Effect::LoadCustomEmojis",
        &format!("{:?}", Effect::LoadCustomEmojis),
    );
    no_sentinel(
        "Action::ReactFailed",
        &format!(
            "{:?}",
            Action::ReactFailed {
                error: crate::api::ApiError::Rejected
            }
        ),
    );
    let custom = vec![crate::data::CustomEmoji {
        name: BODY.to_string(),
        aliases: vec![BODY.to_string()],
        ..Default::default()
    }];
    no_sentinel(
        "Action::CustomEmojisLoaded",
        &format!("{:?}", Action::CustomEmojisLoaded(custom)),
    );
    let image = crate::effect::EmojiRef {
        name: BODY.to_string(),
        etag: BODY.to_string(),
        ext: BODY.to_string(),
    };
    no_sentinel(
        "Effect::FetchEmojiImage",
        &format!(
            "{:?} {:#?}",
            Effect::FetchEmojiImage(image.clone()),
            Effect::FetchEmojiImage(image.clone())
        ),
    );
    no_sentinel(
        "Action::EmojiImageReady",
        &format!(
            "{:?}",
            Action::EmojiImageReady {
                emoji: image.clone(),
                image: crate::emoji_image::Rgba {
                    width: 1,
                    height: 1,
                    pixels: vec![0, 0, 0, 0].into(),
                    ..Default::default()
                },
            }
        ),
    );
    no_sentinel(
        "Action::EmojiImageFailed",
        &format!("{:?}", Action::EmojiImageFailed(image)),
    );
    no_sentinel(
        "Action::CustomEmojisFailed",
        &format!("{:?}", Action::CustomEmojisFailed),
    );
    no_sentinel("Action::EditMessage", &format!("{:?}", Action::EditMessage));
    no_sentinel(
        "Action::DeleteMessage",
        &format!("{:?}", Action::DeleteMessage),
    );
    no_sentinel(
        "Action::EditDone",
        &format!(
            "{:?}",
            Action::EditDone {
                room: RoomId("r-general".to_string()),
                id: "m1".to_string(),
                message: message_with_body(),
            }
        ),
    );
    no_sentinel(
        "Action::DeleteDone",
        &format!(
            "{:?}",
            Action::DeleteDone {
                room: RoomId("r-general".to_string()),
                id: "m1".to_string(),
            }
        ),
    );
    let actions = [
        Action::LoadFailed(ApiError::Unauthorized),
        Action::HistoryLoaded {
            room: RoomId("r-general".to_string()),
            messages: vec![message_with_body()],
            oldest_ts: Some("2026-11-16T09:59:00.000Z".to_string()),
            older: false,
        },
        Action::HistoryFailed {
            room: RoomId("r-general".to_string()),
            older: true,
            error: ApiError::Timeout,
        },
        Action::RoomsLoaded(vec![Room {
            id: RoomId("r-general".to_string()),
            name: "general".to_string(),
            kind: RoomKind::Channel,
            encrypted: false,
            private: false,
            unread: Unread::default(),
            last_seen: None,
            last_activity: 0,
            threads: Default::default(),
        }]),
        Action::FoldersLoaded(vec![Folder {
            id: FolderId("f".to_string()),
            name: "Work".to_string(),
            room_ids: vec![RoomId("r-general".to_string())],
        }]),
        Action::MessagesLoaded(RoomId("r-general".to_string()), vec![message_with_body()]),
        Action::MeLoaded(BODY.to_string()),
        Action::MessageSent {
            room: RoomId("r-general".to_string()),
            id: "rt-test-1".to_string(),
            message: message_with_body(),
        },
    ];
    for action in &actions {
        no_sentinel("Action", &format!("{action:?}"));
    }
    for error in ALL_ERRORS {
        let failed = Action::SendFailed {
            room: RoomId("r-general".to_string()),
            id: "rt-test-1".to_string(),
            error,
        };
        no_sentinel("Action::SendFailed", &format!("{failed:?}"));
        let edit_failed = Action::EditFailed {
            room: RoomId("r-general".to_string()),
            id: "m1".to_string(),
            error,
        };
        no_sentinel("Action::EditFailed", &format!("{edit_failed:?}"));
        let delete_failed = Action::DeleteFailed {
            room: RoomId("r-general".to_string()),
            id: "m1".to_string(),
            error,
        };
        no_sentinel("Action::DeleteFailed", &format!("{delete_failed:?}"));
    }
}

#[test]
fn userinfo_url_not_echoed() {
    let error = parse_server_url(USERINFO_URL).expect_err("credentials in the URL are rejected");
    no_sentinel("parse_server_url error", error);

    let sources = Sources {
        env: conn_env(USERINFO_URL),
        require_connection: true,
        ..Sources::default()
    };
    let report = failing_report(&sources);
    assert!(
        !report.errors.is_empty(),
        "the URL with credentials is a config error"
    );
    sweep_both!("Report (env URL)", report);
}

/// Hostile text: an OSC title-set, a BEL, and a right-to-left override, between letters.
const HOSTILE: &str = "a\u{1b}]0;x\u{7}b\u{202e}c";
/// What `sanitize_line` leaves of `HOSTILE`.
const HOSTILE_CLEAN: &str = "a]0;xbc";

fn is_hostile_char(c: char) -> bool {
    c.is_control() || matches!(c as u32, 0x202a..=0x202e | 0x2066..=0x2069)
}

#[test]
fn hostile_server_text() {
    let id = RoomId("r-hostile".to_string());
    let history: HistoryResp = serde_json::from_value(serde_json::json!({
        "messages": [{
            "_id": "m-hostile",
            "msg": HOSTILE,
            "ts": "2026-11-16T09:00:00.000Z",
            "u": { "_id": "u1", "username": HOSTILE },
        }]
    }))
    .expect("history parses");
    let page = map::history(&id, history.messages);
    assert_eq!(page.messages.len(), 1);

    for (w, h) in [(80, 24), (120, 30)] {
        let mut app = App::new(Size::new(w, h));
        update(
            &mut app,
            Action::RoomsLoaded(vec![Room {
                id: id.clone(),
                name: HOSTILE.to_string(),
                kind: RoomKind::Channel,
                encrypted: false,
                private: false,
                unread: Unread::flag(),
                last_seen: None,
                last_activity: 1,
                threads: Default::default(),
            }]),
        );
        update(
            &mut app,
            Action::FoldersLoaded(vec![Folder {
                id: FolderId("f-hostile".to_string()),
                name: HOSTILE.to_string(),
                room_ids: vec![id.clone()],
            }]),
        );
        // Open the room: move to a room row and press Enter (a folder row would only fold).
        press(&mut app, "gg");
        for _ in 0..6 {
            if app.current_room.is_some() {
                break;
            }
            press(&mut app, "<CR>");
            if app.current_room.is_none() {
                press(&mut app, "j");
            }
        }
        update(
            &mut app,
            Action::HistoryLoaded {
                room: id.clone(),
                messages: page.messages.clone(),
                oldest_ts: page.oldest_ts.clone(),
                older: false,
            },
        );
        assert_eq!(app.current_room, Some(id.clone()), "the room is open");

        let screen = render_to_string(&app, w, h);
        for (row, line) in screen.split('\n').enumerate() {
            if let Some(c) = line.chars().find(|c| is_hostile_char(*c)) {
                panic!("{w}x{h} row {row} holds U+{:04X}: {line}", c as u32);
            }
        }
        assert!(
            screen.contains(HOSTILE_CLEAN),
            "{w}x{h}: the visible text of the hostile name is kept:\n{screen}"
        );
        assert!(
            screen.matches(HOSTILE_CLEAN).count() >= 2,
            "{w}x{h}: name and message both drawn:\n{screen}"
        );
    }
}
