//! Effects: the IO the reducer asks for. The reducer stays pure; it pushes an `Effect` onto
//! `App.effects`, and only `run` (the loop in `lib.rs`) drains them and spawns the task that does
//! the request and sends `Action`s back on the one channel (the same shape as `event.rs`).

use crate::action::Action;
use crate::api::{ApiError, Client};
use crate::data::{RoomId, RoomKind};
use tokio::sync::mpsc::Sender;

/// One fetch intent.
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
}
