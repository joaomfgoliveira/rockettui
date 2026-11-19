//! Effects: the IO the reducer asks for. The reducer stays pure; it pushes an `Effect` onto
//! `App.effects`, and only `run` (the loop in `lib.rs`) drains them and spawns the task that does
//! the request and sends `Action`s back on the one channel (the same shape as `event.rs`).

use crate::action::Action;
use crate::api::{ApiError, Client};
use crate::data::{RoomId, RoomKind};
use tokio::sync::mpsc::Sender;

/// The text of a message being sent, exactly as typed. Its `Debug` prints only the byte length, so
/// the derived `Debug` of `Effect` can never print message text (D-14, T-04-01).
#[derive(Clone, PartialEq)]
pub struct SendText(String);

impl SendText {
    pub fn new(text: String) -> Self {
        SendText(text)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SendText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SendText(<{}> bytes)", self.0.len())
    }
}

/// One IO intent: a fetch, or one send attempt.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Load the joined rooms and the sidebar folders.
    LoadRooms,
    /// Load a page of a room's history: the newest `count` messages, or, with `before`, the page
    /// strictly older than that raw server timestamp.
    LoadHistory {
        room: RoomId,
        kind: RoomKind,
        private: bool,
        count: u16,
        before: Option<String>,
    },
    /// One attempt to send one pending message to `room`: `POST chat.sendMessage` with the client
    /// `id` as the message `_id` (D-07, D-15). `reconcile` selects the `chat.getMessage`-first
    /// branch of a retry (plan 04-03); this plan POSTs in every case. The answer comes back as
    /// `Action::MessageSent` or `Action::SendFailed`, both addressed by `room` and `id`.
    SendMessage {
        room: RoomId,
        id: String,
        text: SendText,
        reconcile: bool,
    },
}

/// Perform `effect` and return the Actions that report the outcome. `client` is `Err` when the
/// HTTP client could not be built, which fails every effect with that error.
pub async fn run(client: Result<Client, ApiError>, effect: Effect) -> Vec<Action> {
    let client = match client {
        Ok(client) => client,
        Err(error) => {
            return vec![match effect {
                Effect::LoadRooms => Action::LoadFailed(error),
                Effect::LoadHistory { room, before, .. } => Action::HistoryFailed {
                    room,
                    older: before.is_some(),
                    error,
                },
                Effect::SendMessage { room, id, .. } => Action::SendFailed { room, id, error },
            }];
        }
    };
    match effect {
        Effect::LoadRooms => match client.load_rooms().await {
            Ok(list) => vec![
                Action::FoldersLoaded(list.folders),
                Action::RoomsLoaded(list.rooms),
            ],
            Err(error) => vec![Action::LoadFailed(error)],
        },
        Effect::LoadHistory {
            room,
            kind,
            private,
            count,
            before,
        } => {
            let older = before.is_some();
            match client
                .load_history(&room, kind, private, count, before.as_deref())
                .await
            {
                Ok(page) => vec![Action::HistoryLoaded {
                    room,
                    messages: page.messages,
                    oldest_ts: page.oldest_ts,
                    older,
                }],
                Err(error) => vec![Action::HistoryFailed { room, older, error }],
            }
        }
        Effect::SendMessage { room, id, text, .. } => {
            match client.send_message(&room, &id, text.as_str()).await {
                Ok(message) => vec![Action::MessageSent { room, id, message }],
                Err(error) => vec![Action::SendFailed { room, id, error }],
            }
        }
    }
}

/// Run `effect` on its own task and send each resulting Action on `tx`. The only place an effect
/// starts IO.
pub fn spawn(client: Result<Client, ApiError>, tx: Sender<Action>, effect: Effect) {
    tokio::spawn(async move {
        for action in run(client, effect).await {
            if tx.send(action).await.is_err() {
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::connection::{ConnEnv, EnvValue, Secret};
    use crate::config::{Sources, load_from_sources};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const HISTORY: &str = include_str!("api/fixtures/history.json");

    fn client_for(server: &MockServer) -> Client {
        let env = |v: &str| EnvValue::Set(Secret::new(v.to_string()));
        let sources = Sources {
            env: ConnEnv {
                server_url: env(&server.uri()),
                user_id: env("u1"),
                token: env("SENTINEL-TOKEN-9c41"),
            },
            require_connection: true,
            ..Sources::default()
        };
        let config = load_from_sources(&sources)
            .unwrap_or_else(|report| panic!("unexpected config errors: {report}"));
        Client::new(&config.connection.expect("connection")).expect("client")
    }

    fn load_history_effect() -> Effect {
        Effect::LoadHistory {
            room: RoomId("r-devteam".to_string()),
            kind: RoomKind::PrivateGroup,
            private: true,
            count: 50,
            before: None,
        }
    }

    #[tokio::test]
    async fn load_history_actions() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/groups.history"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(HISTORY.to_string(), "application/json"),
            )
            .mount(&server)
            .await;
        let actions = run(Ok(client_for(&server)), load_history_effect()).await;
        assert_eq!(actions.len(), 1, "{actions:?}");
        match &actions[0] {
            Action::HistoryLoaded {
                room,
                messages,
                oldest_ts,
                older,
            } => {
                assert_eq!(room.0, "r-devteam");
                assert_eq!(messages.len(), 4);
                assert_eq!(oldest_ts.as_deref(), Some("2026-11-16T09:59:00.000Z"));
                assert!(!older);
            }
            other => panic!("expected HistoryLoaded, got {other:?}"),
        }

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(401).set_body_string("{\"success\":false}"))
            .mount(&server)
            .await;
        let actions = run(Ok(client_for(&server)), load_history_effect()).await;
        assert_eq!(
            actions,
            [Action::HistoryFailed {
                room: RoomId("r-devteam".to_string()),
                older: false,
                error: ApiError::Unauthorized,
            }]
        );
    }

    #[tokio::test]
    async fn load_history_without_client_fails_closed() {
        let actions = run(Err(ApiError::Client), load_history_effect()).await;
        assert_eq!(
            actions,
            [Action::HistoryFailed {
                room: RoomId("r-devteam".to_string()),
                older: false,
                error: ApiError::Client,
            }]
        );
    }

    const SENTINEL: &str = "SENTINEL-BODY-7f3a";

    fn general() -> RoomId {
        RoomId("general".to_string())
    }

    fn send_effect() -> Effect {
        Effect::SendMessage {
            room: general(),
            id: "rt-test-1".to_string(),
            text: SendText::new(SENTINEL.to_string()),
            reconcile: false,
        }
    }

    /// The server's answer to a send of `text` under `id`: the stored message.
    fn answer(id: &str, text: &str) -> String {
        serde_json::json!({
            "message": {
                "_id": id, "rid": "general", "msg": text,
                "ts": "2026-11-17T10:00:00.123Z",
                "u": {"_id": "u1", "username": "smoke"}
            },
            "success": true
        })
        .to_string()
    }

    async fn mount_send_ok(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/api/v1/chat.sendMessage"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(answer("rt-test-1", SENTINEL), "application/json"),
            )
            .mount(server)
            .await;
    }

    async fn mount_send_status(server: &MockServer, status: u16) {
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(status).set_body_string("{\"success\":false}"))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn send_message_actions() {
        let server = MockServer::start().await;
        mount_send_ok(&server).await;
        let actions = run(Ok(client_for(&server)), send_effect()).await;
        assert_eq!(actions.len(), 1, "{actions:?}");
        match &actions[0] {
            Action::MessageSent { room, id, message } => {
                assert_eq!(*room, general());
                assert_eq!(id, "rt-test-1");
                assert_eq!(message.id, "rt-test-1");
                assert_eq!(message.author, "smoke");
                assert_eq!(message.body, SENTINEL);
            }
            other => panic!("expected MessageSent, got {other:?}"),
        }

        let server = MockServer::start().await;
        mount_send_status(&server, 401).await;
        let actions = run(Ok(client_for(&server)), send_effect()).await;
        assert_eq!(
            actions,
            [Action::SendFailed {
                room: general(),
                id: "rt-test-1".to_string(),
                error: ApiError::Unauthorized,
            }]
        );
    }

    #[tokio::test]
    async fn send_without_client_fails_closed() {
        let actions = run(Err(ApiError::Client), send_effect()).await;
        assert_eq!(
            actions,
            [Action::SendFailed {
                room: general(),
                id: "rt-test-1".to_string(),
                error: ApiError::Client,
            }]
        );
    }

    #[test]
    fn send_text_debug_redacts() {
        let text = SendText::new(SENTINEL.to_string());
        assert_eq!(text.as_str(), SENTINEL);
        let shown = format!("{text:?}");
        assert_eq!(shown, format!("SendText(<{}> bytes)", SENTINEL.len()));
        assert!(!shown.contains(SENTINEL));
        let effect = format!("{:?}", send_effect());
        assert!(!effect.contains(SENTINEL), "{effect}");
        assert!(effect.contains("rt-test-1"), "{effect}");
    }

    #[tokio::test]
    async fn send_round_trip_swaps_in_place() {
        use crate::app::update::update;
        use crate::data::{Message, MessageKind, SendState};
        use crate::test_util::app_80x24;

        let pending = Message {
            id: "rt-test-1".to_string(),
            room_id: general(),
            author: "me".to_string(),
            ts: 0,
            body: SENTINEL.to_string(),
            kind: MessageKind::Normal,
            send: SendState::Sending,
        };
        let seeded = || {
            let mut app = app_80x24();
            app.open_room(general());
            app.messages
                .get_mut(&general())
                .expect("general has messages")
                .push(pending.clone());
            app
        };

        // 200: the server's copy replaces the pending message in place; no history reload.
        let server = MockServer::start().await;
        mount_send_ok(&server).await;
        let mut app = seeded();
        let len = app.messages[&general()].len();
        let k = len - 1;
        for action in run(Ok(client_for(&server)), send_effect()).await {
            update(&mut app, action);
        }
        assert_eq!(app.messages[&general()].len(), len);
        let swapped = &app.messages[&general()][k];
        assert_eq!(swapped.id, "rt-test-1");
        assert_eq!(swapped.author, "smoke");
        assert_eq!(swapped.ts, 1_794_909_600);
        assert!(
            !app.effects
                .iter()
                .any(|e| matches!(e, Effect::LoadHistory { .. })),
            "{:?}",
            app.effects
        );
        let requests = server.received_requests().await.expect("recorded");
        assert_eq!(requests.len(), 1, "one send is one request");

        // 401: the message is unchanged and the status bar shows the fixed reason.
        let server = MockServer::start().await;
        mount_send_status(&server, 401).await;
        let mut app = seeded();
        let before = app.messages[&general()].clone();
        for action in run(Ok(client_for(&server)), send_effect()).await {
            update(&mut app, action);
        }
        assert_eq!(app.messages[&general()], before);
        assert_eq!(app.messages[&general()][k].author, "me");
        let Some(crate::app::StatusMsg::Error(text)) = &app.status_msg else {
            panic!("no error status");
        };
        assert!(text.starts_with("message not sent: "), "{text}");
        assert!(!text.contains(SENTINEL), "{text}");
    }
}
