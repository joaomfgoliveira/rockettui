//! The Rocket.Chat REST client (AUTH-01, ROOM-01). Requests carry the Personal Access Token in two
//! headers, never follow redirects, and fail with a typed `ApiError` whose text is fixed.
//!
//! Security rules this module keeps (T-03-01, T-03-02, T-03-04, T-03-05):
//! - both header values are marked sensitive and the token never leaves `Client`;
//! - `redirect::Policy::none()`: reqwest would forward the custom auth headers to another host;
//! - the default rustls TLS verification is the only mode, there is no switch to weaken it;
//! - a `reqwest::Error` is never formatted (its text holds the request URL), it is mapped to an
//!   `ApiError` in one place;
//! - every body is capped at `MAX_BODY_BYTES` and decoded leniently;
//! - the only requests are GETs and the one `chat.sendMessage` POST (Phase 4).

pub mod map;
pub mod wire;

use crate::config::connection::Connection;
use crate::data::{Folder, Message, Room, RoomId, RoomKind};
use reqwest::StatusCode;
use reqwest::Url;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;
use std::fmt;
use std::time::Duration;

/// How long to wait for the TCP/TLS connection.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a whole request may take.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// Largest response body accepted.
pub const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;

/// Why a request failed. A small `Copy` enum with no payload text, so it can sit in `App` and in
/// an `Action` and can never carry a token, URL or message body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiError {
    Unauthorized,
    Forbidden,
    NotFound,
    RateLimited,
    Redirected,
    Server(u16),
    Unreachable,
    Timeout,
    BadResponse,
    TooLarge,
    Client,
    /// A send-path 4xx other than 401, 403, 404 and 429 (read-only or muted room, text too long):
    /// the server refused the request outright.
    Rejected,
    /// The connection broke after it was made, so the request may or may not have been processed.
    ConnectionLost,
}

/// One fixed, actionable line per error (D-06). It is the status-bar text.
impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Unauthorized => write!(f, "unauthorized: check the user id and token"),
            ApiError::Forbidden => write!(f, "forbidden: the token lacks permission"),
            ApiError::NotFound => {
                write!(f, "not found: check server_url (the Rocket.Chat base URL)")
            }
            ApiError::RateLimited => {
                write!(
                    f,
                    "rate limited by the server: wait a minute, then :reconnect"
                )
            }
            ApiError::Redirected => {
                write!(
                    f,
                    "server redirected: check server_url (http vs https, path)"
                )
            }
            ApiError::Server(code) => {
                write!(f, "server error (HTTP {code}): try :reconnect later")
            }
            ApiError::Unreachable => write!(
                f,
                "cannot connect (unreachable or TLS error): check server_url"
            ),
            ApiError::Timeout => write!(f, "request timed out: check the network"),
            ApiError::BadResponse => write!(
                f,
                "unexpected response: is server_url a Rocket.Chat server?"
            ),
            ApiError::TooLarge => write!(f, "server response too large"),
            ApiError::Client => write!(
                f,
                "cannot build the HTTP client: check the user id and token characters"
            ),
            ApiError::Rejected => write!(
                f,
                "server rejected the request: read-only or muted room, or text too long"
            ),
            ApiError::ConnectionLost => {
                write!(f, "connection lost during the request: check the network")
            }
        }
    }
}

impl ApiError {
    /// D-07 for the send path: true when the server may have stored the message although the
    /// client got no clean answer (timeout, 5xx per D-18, lost connection, an answer that could
    /// not be read). Such a message is `Unconfirmed` and is reconciled before it is posted again.
    pub fn outcome_unknown(self) -> bool {
        matches!(
            self,
            ApiError::Timeout
                | ApiError::Server(_)
                | ApiError::ConnectionLost
                | ApiError::BadResponse
                | ApiError::TooLarge
        )
    }
}

/// The HTTP status first, before any body text (RESEARCH Pattern 7). `None` for a success.
fn status_error(status: StatusCode) -> Option<ApiError> {
    if status.is_redirection() {
        return Some(ApiError::Redirected);
    }
    match status.as_u16() {
        401 => Some(ApiError::Unauthorized),
        403 => Some(ApiError::Forbidden),
        404 => Some(ApiError::NotFound),
        429 => Some(ApiError::RateLimited),
        code if status.is_server_error() => Some(ApiError::Server(code)),
        _ if status.is_client_error() => Some(ApiError::BadResponse),
        _ => None,
    }
}

/// The one place a `reqwest::Error` is looked at. Its text is never formatted (Pitfall 2).
fn send_error(e: &reqwest::Error) -> ApiError {
    if e.is_timeout() {
        ApiError::Timeout
    } else if e.is_connect() {
        ApiError::Unreachable
    } else if e.is_builder() {
        ApiError::Client
    } else if e.is_decode() || e.is_body() {
        ApiError::BadResponse
    } else {
        // A transport error after the connection was made (a reset, a close before the answer).
        ApiError::ConnectionLost
    }
}

/// Send `request`, check the HTTP status, read the body under the size cap and decode it. Shared
/// by every GET and by the one `chat.sendMessage` POST, so they all keep the same rules: the status
/// first (a redirect is an error, never followed), then the capped body, then lenient decoding.
///
/// `client_error` is what a 4xx status other than 401, 403, 404 and 429 becomes: `BadResponse` for
/// a GET, `Rejected` for the send (D-12). A body that cannot be decoded stays `BadResponse` on both.
async fn read_json<T: DeserializeOwned>(
    request: reqwest::RequestBuilder,
    client_error: ApiError,
) -> Result<T, ApiError> {
    let mut response = request.send().await.map_err(|e| send_error(&e))?;
    if let Some(error) = status_error(response.status()) {
        return Err(if error == ApiError::BadResponse {
            client_error
        } else {
            error
        });
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_BODY_BYTES as u64)
    {
        return Err(ApiError::TooLarge);
    }
    let mut body: Vec<u8> = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| send_error(&e))? {
        if body.len() + chunk.len() > MAX_BODY_BYTES {
            return Err(ApiError::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|_| ApiError::BadResponse)
}

/// The body of `chat.sendMessage`: exactly the client `_id`, the room id and the text. Built from
/// `Serialize` structs, never by hand, so the text is always escaped as JSON. It carries no
/// server-owned field (`ts`, `u`, ...): Rocket.Chat 9.0.0 rejects them. No `Debug`: it holds text.
#[derive(serde::Serialize)]
struct SendBody<'a> {
    message: SendMsg<'a>,
}

#[derive(serde::Serialize)]
struct SendMsg<'a> {
    #[serde(rename = "_id")]
    id: &'a str,
    rid: &'a str,
    msg: &'a str,
}

/// The body of `chat.update`: the room id, the message id and the new text. Built from `Serialize`
/// structs so the text is always escaped as JSON. No `Debug`: it holds text.
#[derive(serde::Serialize)]
struct UpdateBody<'a> {
    #[serde(rename = "roomId")]
    room_id: &'a str,
    #[serde(rename = "msgId")]
    msg_id: &'a str,
    text: &'a str,
}

/// The body of `chat.delete`: the room id and the message id.
#[derive(serde::Serialize)]
struct DeleteBody<'a> {
    #[serde(rename = "roomId")]
    room_id: &'a str,
    #[serde(rename = "msgId")]
    msg_id: &'a str,
}

/// The joined rooms and the sidebar folders built from one load.
pub struct RoomList {
    pub rooms: Vec<Room>,
    pub folders: Vec<Folder>,
}

/// One page of history as oldest-first messages. `oldest_ts` is the raw server timestamp string of
/// the oldest message, the `latest` value of the next older page; `None` for an empty page.
#[derive(Clone, PartialEq)]
pub struct HistoryPage {
    pub messages: Vec<Message>,
    pub oldest_ts: Option<String>,
}

/// Redacting `Debug`: the number of messages only, never their text.
impl fmt::Debug for HistoryPage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HistoryPage(<{} messages>)", self.messages.len())
    }
}

/// The history endpoint of a room. It follows the server room type, never the name: a direct
/// message is `im.history`, a room whose server type is `p` (private groups, and private
/// discussions and teams) is `groups.history`, every other room is `channels.history`.
pub fn history_method(kind: RoomKind, private: bool) -> &'static str {
    if kind == RoomKind::DirectMessage {
        "im.history"
    } else if private {
        "groups.history"
    } else {
        "channels.history"
    }
}

/// An authenticated client for one server. Cheap to clone. It is the only holder of the token.
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base: Url,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Client {{ .. }}")
    }
}

impl Client {
    /// Build the client. A user id or token with a control character cannot be a header value:
    /// that is `ApiError::Client` with a fixed message, never the value.
    pub fn new(conn: &Connection) -> Result<Client, ApiError> {
        Client::with_timeouts(conn, CONNECT_TIMEOUT, REQUEST_TIMEOUT)
    }

    /// `new` with explicit connect and whole-request deadlines; the tests use short ones. Nothing
    /// else about the TLS or redirect setup can be changed from here (D-08).
    pub(crate) fn with_timeouts(
        conn: &Connection,
        connect: Duration,
        total: Duration,
    ) -> Result<Client, ApiError> {
        let mut user_id =
            HeaderValue::from_str(conn.user_id.expose()).map_err(|_| ApiError::Client)?;
        let mut token = HeaderValue::from_str(conn.token.expose()).map_err(|_| ApiError::Client)?;
        user_id.set_sensitive(true);
        token.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert("X-User-Id", user_id);
        headers.insert("X-Auth-Token", token);
        let http = reqwest::Client::builder()
            .default_headers(headers)
            // The token is a custom header and reqwest forwards those across hosts on a redirect.
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(connect)
            .timeout(total)
            .user_agent(concat!("rockettui/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| ApiError::Client)?;
        Ok(Client {
            http,
            base: conn.server_url.clone(),
        })
    }

    /// `<base>/api/v1/<method>`; a base with a sub-path keeps it.
    fn endpoint(&self, method: &str) -> Url {
        let mut url = self.base.clone();
        if let Ok(mut segments) = url.path_segments_mut() {
            segments.pop_if_empty().extend(["api", "v1", method]);
        }
        url
    }

    /// GET `method` with `query`, then `read_json`.
    async fn get_json<T: DeserializeOwned>(
        &self,
        method: &str,
        query: &[(&str, String)],
    ) -> Result<T, ApiError> {
        let mut request = self.http.get(self.endpoint(method));
        if !query.is_empty() {
            request = request.query(query);
        }
        read_json(request, ApiError::BadResponse).await
    }

    /// The user's own username from `GET /api/v1/me` (D-17), `None` when the answer holds none or
    /// an empty one. Best effort: the caller ignores every error and keeps its fallback author.
    pub async fn me(&self) -> Result<Option<String>, ApiError> {
        let resp: wire::MeResp = self.get_json("me", &[]).await?;
        Ok(resp.username.filter(|name| !name.is_empty()))
    }

    /// Ask the server for message `id` of `room` with `GET chat.getMessage` (D-15): the reconcile
    /// step before a retried `Unconfirmed` message is posted again. Any error means the answer is
    /// not known; the caller treats every non-200 as "not stored".
    pub async fn get_message(&self, room: &RoomId, id: &str) -> Result<Message, ApiError> {
        let resp: wire::MessageResp = self
            .get_json("chat.getMessage", &[("msgId", id.to_string())])
            .await?;
        Ok(map::message(room, resp.message))
    }

    /// Load the user's joined rooms and sidebar folders: `subscriptions.get`, `rooms.get` and
    /// `users.getPreferences` run concurrently. The rooms are joined on the room id and the load is
    /// all or nothing: if `subscriptions.get` or `rooms.get` fails the whole load fails, so a room
    /// list without its `encrypted` flags is never applied. The preferences call is best effort:
    /// any failure of it (status, transport, decode, size) means no custom categories, and the
    /// folders degrade to Favorites only (RESEARCH Pitfall 13).
    pub async fn load_rooms(&self) -> Result<RoomList, ApiError> {
        let (subs, records, prefs) = tokio::join!(
            self.get_json::<wire::SubscriptionsResp>("subscriptions.get", &[]),
            self.get_json::<wire::RoomsResp>("rooms.get", &[]),
            self.get_json::<wire::PreferencesResp>("users.getPreferences", &[]),
        );
        let subs = subs?.update;
        let records = records?.update;
        let prefs = prefs.ok().map(|p| p.preferences);
        let rooms = map::rooms(&subs, &records);
        let folders = map::folders(&subs, &rooms, prefs.as_ref());
        Ok(RoomList { rooms, folders })
    }

    /// One page of a room's history, newest `count` messages, or the `count` messages strictly
    /// older than `before` (a raw server timestamp from `HistoryPage::oldest_ts`, sent verbatim,
    /// Pitfall 7). The query is `roomId` (never the room name) and `count`; the thread-replies flag
    /// is not sent, so thread replies stay out of the main flow (Pitfall 11, D-15).
    pub async fn load_history(
        &self,
        room: &RoomId,
        kind: RoomKind,
        private: bool,
        count: u16,
        before: Option<&str>,
    ) -> Result<HistoryPage, ApiError> {
        let mut query = vec![("roomId", room.0.clone()), ("count", count.to_string())];
        if let Some(latest) = before {
            query.push(("latest", latest.to_string()));
        }
        let resp: wire::HistoryResp = self.get_json(history_method(kind, private), &query).await?;
        Ok(map::history(room, resp.messages))
    }

    /// One page of the server-side search of `room`: `GET chat.search` with `roomId`, `searchText`,
    /// `count` and `offset`. The results keep the server's order, newest first. Measured on 8.9.1
    /// and 9.0.0: an empty text or a room the user cannot search is an HTTP 400, which is
    /// `ApiError::Rejected`. The query is never part of an error.
    pub async fn search_room(
        &self,
        room: &RoomId,
        query: &str,
        offset: u32,
        count: u16,
    ) -> Result<Vec<Message>, ApiError> {
        let params = [
            ("roomId", room.0.clone()),
            ("searchText", query.to_string()),
            ("count", count.to_string()),
            ("offset", offset.to_string()),
        ];
        let request = self.http.get(self.endpoint("chat.search")).query(&params);
        let resp: wire::HistoryResp = read_json(request, ApiError::Rejected).await?;
        Ok(map::search_page(room, resp.messages))
    }

    /// Send `text` to `room` with `POST chat.sendMessage`, the client-chosen `id` as the message
    /// `_id` (D-07, D-15): the server stores the message under that id, so the answer and every
    /// later copy of the message carry it. The body is the exact text, no trim and no change
    /// (D-12). The answer is the message as stored (server time and author), mapped like history.
    /// Redirects stay disabled, so a 3xx is `ApiError::Redirected` and neither the text nor the
    /// credentials reach the redirect target.
    pub async fn send_message(
        &self,
        room: &RoomId,
        id: &str,
        text: &str,
    ) -> Result<Message, ApiError> {
        let body = SendBody {
            message: SendMsg {
                id,
                rid: &room.0,
                msg: text,
            },
        };
        let request = self
            .http
            .post(self.endpoint("chat.sendMessage"))
            .json(&body);
        let resp: wire::MessageResp = read_json(request, ApiError::Rejected).await?;
        Ok(map::message(room, resp.message))
    }

    /// Replace the text of message `id` of `room` with `POST chat.update` and return the stored
    /// copy. Measured on 8.9.1 and 9.0.0: every refusal (an unknown id, someone else's message, the
    /// settings off, the edit time limit) is an HTTP 400, which is `ApiError::Rejected`. No server
    /// text reaches the error.
    pub async fn edit_message(
        &self,
        room: &RoomId,
        id: &str,
        text: &str,
    ) -> Result<Message, ApiError> {
        let body = UpdateBody {
            room_id: &room.0,
            msg_id: id,
            text,
        };
        let request = self.http.post(self.endpoint("chat.update")).json(&body);
        let resp: wire::MessageResp = read_json(request, ApiError::Rejected).await?;
        Ok(map::message(room, resp.message))
    }

    /// Delete message `id` of `room` with `POST chat.delete`. The answer body is not read beyond
    /// being valid JSON; a refusal is `ApiError::Rejected` like for `edit_message`.
    pub async fn delete_message(&self, room: &RoomId, id: &str) -> Result<(), ApiError> {
        let body = DeleteBody {
            room_id: &room.0,
            msg_id: id,
        };
        let request = self.http.post(self.endpoint("chat.delete")).json(&body);
        let _: serde::de::IgnoredAny = read_json(request, ApiError::Rejected).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::app::App;
    use crate::app::update::update;
    use crate::config::connection::{ConnEnv, EnvValue, Secret};
    use crate::config::{Sources, load_from_sources};
    use crate::data::{MessageKind, SendState};
    use crate::effect::{self, Effect};
    use crate::test_util::render_to_string;
    use ratatui::layout::Size;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const SUBS: &str = include_str!("fixtures/subscriptions.json");
    const ROOMS: &str = include_str!("fixtures/rooms.json");
    const PREFS: &str = include_str!("fixtures/preferences.json");
    const USER: &str = "u1";
    const TOKEN: &str = "SENTINEL-TOKEN-9c41";

    fn env_for(url: &str, user: &str, token: &str) -> ConnEnv {
        ConnEnv {
            server_url: EnvValue::Set(Secret::new(url.to_string())),
            user_id: EnvValue::Set(Secret::new(user.to_string())),
            token: EnvValue::Set(Secret::new(token.to_string())),
        }
    }

    fn connection(url: &str, user: &str, token: &str) -> Connection {
        let sources = Sources {
            env: env_for(url, user, token),
            require_connection: true,
            ..Sources::default()
        };
        match load_from_sources(&sources) {
            Ok(config) => config.connection.expect("connection resolved"),
            Err(report) => panic!("unexpected config errors: {report}"),
        }
    }

    fn client_for(server: &MockServer) -> Client {
        Client::new(&connection(&server.uri(), USER, TOKEN)).expect("client")
    }

    /// Mount `body` for `GET /api/v1/<name>` when both auth headers are right.
    async fn mount_ok(server: &MockServer, name: &str, body: &str) {
        Mock::given(method("GET"))
            .and(path(format!("/api/v1/{name}")))
            .and(header("X-User-Id", USER))
            .and(header("X-Auth-Token", TOKEN))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(body.to_string(), "application/json"),
            )
            .mount(server)
            .await;
    }

    async fn mount_status(server: &MockServer, name: &str, status: u16) {
        Mock::given(method("GET"))
            .and(path(format!("/api/v1/{name}")))
            .respond_with(ResponseTemplate::new(status).set_body_string("{\"success\":false}"))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn tracer_rooms_end_to_end() {
        let server = MockServer::start().await;
        mount_ok(&server, "subscriptions.get", SUBS).await;
        mount_ok(&server, "rooms.get", ROOMS).await;
        let client = client_for(&server);

        let actions = effect::run(Ok(client), Effect::LoadRooms).await;
        assert_eq!(actions.len(), 2, "{actions:?}");
        assert!(matches!(actions[0], Action::FoldersLoaded(_)));
        assert!(matches!(actions[1], Action::RoomsLoaded(_)));

        let mut app = App::new(Size::new(80, 24));
        for action in actions {
            update(&mut app, action);
        }
        let screen = render_to_string(&app, 80, 24);
        for shown in ["general", "dev-team", "alice"] {
            assert!(screen.contains(shown), "{shown} missing:\n{screen}");
        }
        assert!(!screen.contains("carol"), "{screen}");
        assert!(!screen.contains("SENTINEL-LASTMSG"), "{screen}");

        let requests = server.received_requests().await.expect("recorded");
        assert!(!requests.is_empty());
        assert!(requests.iter().all(|r| r.method.as_str() == "GET"));
    }

    #[tokio::test]
    async fn wrong_credentials_are_unauthorized() {
        let server = MockServer::start().await;
        mount_ok(&server, "subscriptions.get", SUBS).await;
        mount_ok(&server, "rooms.get", ROOMS).await;
        let wrong = Client::new(&connection(&server.uri(), USER, "other-token")).expect("client");
        let actions = effect::run(Ok(wrong), Effect::LoadRooms).await;
        // wiremock answers 404 to a request no mock matches; the point is that nothing loads.
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::LoadFailed(_)));
    }

    #[tokio::test]
    async fn rooms_load_all_or_nothing() {
        let server = MockServer::start().await;
        mount_ok(&server, "subscriptions.get", SUBS).await;
        mount_status(&server, "rooms.get", 500).await;
        let actions = effect::run(Ok(client_for(&server)), Effect::LoadRooms).await;
        assert_eq!(actions, [Action::LoadFailed(ApiError::Server(500))]);

        let server = MockServer::start().await;
        mount_status(&server, "subscriptions.get", 401).await;
        mount_ok(&server, "rooms.get", ROOMS).await;
        let actions = effect::run(Ok(client_for(&server)), Effect::LoadRooms).await;
        assert_eq!(actions, [Action::LoadFailed(ApiError::Unauthorized)]);
    }

    #[tokio::test]
    async fn effect_run_without_client_fails_closed() {
        let actions = effect::run(Err(ApiError::Client), Effect::LoadRooms).await;
        assert_eq!(actions, [Action::LoadFailed(ApiError::Client)]);
    }

    #[tokio::test]
    async fn me_username() {
        let me = |body: &str| {
            let body = body.to_string();
            async move {
                let server = MockServer::start().await;
                mount_ok(&server, "me", &body).await;
                client_for(&server).me().await
            }
        };
        let full = "{\"_id\":\"u1\",\"username\":\"smoke\",\"success\":true,\"roles\":[\"user\"]}";
        assert_eq!(me(full).await, Ok(Some("smoke".to_string())));
        assert_eq!(me("{\"_id\":\"u1\",\"success\":true}").await, Ok(None));
        assert_eq!(me("{\"username\":\"\"}").await, Ok(None));
        assert_eq!(me("{\"username\":null}").await, Ok(None));
        assert_eq!(
            me("<html>not json</html>").await,
            Err(ApiError::BadResponse)
        );

        // `mount_ok` only answers when both auth headers are right, so a 200 above proves the
        // request carried them; a 401 is Unauthorized.
        let server = MockServer::start().await;
        mount_status(&server, "me", 401).await;
        assert_eq!(client_for(&server).me().await, Err(ApiError::Unauthorized));
        let requests = server.received_requests().await.expect("recorded");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method.as_str(), "GET");
    }

    #[test]
    fn token_control_chars() {
        let token = "SENTINEL-TOKEN-9c41\nX-Evil: 1";
        let conn = connection("http://127.0.0.1:3100", USER, token);
        let err = Client::new(&conn).expect_err("control characters are rejected");
        assert_eq!(err, ApiError::Client);
        assert!(!err.to_string().contains("SENTINEL-TOKEN-9c41"));
        assert!(!format!("{err:?}").contains("SENTINEL-TOKEN-9c41"));
        // The same holds for the user id.
        let conn = connection("http://127.0.0.1:3100", "u\n1", TOKEN);
        assert_eq!(Client::new(&conn).expect_err("rejected"), ApiError::Client);
    }

    #[test]
    fn status_mapping() {
        let code = |n: u16| status_error(StatusCode::from_u16(n).unwrap());
        assert_eq!(code(200), None);
        assert_eq!(code(204), None);
        assert_eq!(code(301), Some(ApiError::Redirected));
        assert_eq!(code(307), Some(ApiError::Redirected));
        assert_eq!(code(400), Some(ApiError::BadResponse));
        assert_eq!(code(401), Some(ApiError::Unauthorized));
        assert_eq!(code(403), Some(ApiError::Forbidden));
        assert_eq!(code(404), Some(ApiError::NotFound));
        assert_eq!(code(429), Some(ApiError::RateLimited));
        assert_eq!(code(500), Some(ApiError::Server(500)));
        assert_eq!(code(503), Some(ApiError::Server(503)));
    }

    #[tokio::test]
    async fn redirect_is_not_followed() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(307).insert_header("Location", "http://127.0.0.1:9/x"),
            )
            .mount(&server)
            .await;
        let actions = effect::run(Ok(client_for(&server)), Effect::LoadRooms).await;
        assert_eq!(actions, [Action::LoadFailed(ApiError::Redirected)]);
    }

    #[tokio::test]
    async fn undecodable_body_is_bad_response() {
        let server = MockServer::start().await;
        mount_ok(&server, "subscriptions.get", "<html>not json</html>").await;
        mount_ok(&server, "rooms.get", ROOMS).await;
        let actions = effect::run(Ok(client_for(&server)), Effect::LoadRooms).await;
        assert_eq!(actions, [Action::LoadFailed(ApiError::BadResponse)]);
    }

    #[tokio::test]
    async fn oversized_body_is_too_large() {
        let server = MockServer::start().await;
        let huge = format!(
            "{{\"update\":[],\"pad\":\"{}\"}}",
            "x".repeat(MAX_BODY_BYTES + 16)
        );
        mount_ok(&server, "subscriptions.get", &huge).await;
        mount_ok(&server, "rooms.get", ROOMS).await;
        let actions = effect::run(Ok(client_for(&server)), Effect::LoadRooms).await;
        assert_eq!(actions, [Action::LoadFailed(ApiError::TooLarge)]);
    }

    #[tokio::test]
    async fn unreachable_server_is_unreachable() {
        // Bind an ephemeral port, then release it: nothing listens there, so the connect is refused.
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
            listener.local_addr().expect("addr").port()
        };
        let url = format!("http://127.0.0.1:{port}");
        let client = Client::new(&connection(&url, USER, TOKEN)).expect("client");
        let actions = effect::run(Ok(client), Effect::LoadRooms).await;
        assert_eq!(actions, [Action::LoadFailed(ApiError::Unreachable)]);
    }

    #[test]
    fn debug_and_display_never_hold_the_token() {
        let conn = connection("http://127.0.0.1:3100", USER, TOKEN);
        let client = Client::new(&conn).expect("client");
        assert_eq!(format!("{client:?}"), "Client { .. }");
        for error in [
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
        ] {
            assert!(!format!("{error:?}").contains(TOKEN));
            assert!(!error.to_string().contains(TOKEN));
        }
    }

    /// Mount subscriptions.get and rooms.get with the fixtures.
    async fn mount_rooms(server: &MockServer) {
        mount_ok(server, "subscriptions.get", SUBS).await;
        mount_ok(server, "rooms.get", ROOMS).await;
    }

    #[tokio::test]
    async fn preferences_failure_falls_back() {
        // A 400 (the server has no default preferences), a 500 and a body that is not JSON.
        for broken in [
            ResponseTemplate::new(400).set_body_string("{\"success\":false}"),
            ResponseTemplate::new(500).set_body_string("oops"),
            ResponseTemplate::new(200).set_body_raw("<html>no</html>", "text/html"),
        ] {
            let server = MockServer::start().await;
            mount_rooms(&server).await;
            Mock::given(method("GET"))
                .and(path("/api/v1/users.getPreferences"))
                .respond_with(broken)
                .mount(&server)
                .await;
            let list = client_for(&server).load_rooms().await.expect("rooms load");
            assert_eq!(list.rooms.len(), 7);
            let names: Vec<&str> = list.folders.iter().map(|f| f.name.as_str()).collect();
            assert_eq!(names, ["Favorites"]);
        }
    }

    #[tokio::test]
    async fn preferences_requested() {
        let server = MockServer::start().await;
        mount_rooms(&server).await;
        mount_ok(&server, "users.getPreferences", PREFS).await;
        let list = client_for(&server).load_rooms().await.expect("rooms load");
        let names: Vec<&str> = list.folders.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["Favorites", "Work"]);

        let requests = server.received_requests().await.expect("recorded");
        let prefs: Vec<_> = requests
            .iter()
            .filter(|r| r.url.path() == "/api/v1/users.getPreferences")
            .collect();
        assert_eq!(prefs.len(), 1);
        assert_eq!(prefs[0].method.as_str(), "GET");
        assert_eq!(prefs[0].headers.get("x-user-id").unwrap(), USER);
        assert_eq!(prefs[0].headers.get("x-auth-token").unwrap(), TOKEN);
    }

    /// The text of the send tests: a double quote, a newline, a non-ASCII letter and an emoji.
    const SEND_TEXT: &str = "say \"hi\"\nca\u{e9} \u{1F680}";

    /// What the server answers to a send: the stored message with server time and author.
    fn send_answer(id: &str, text: &str) -> String {
        serde_json::json!({
            "message": {
                "_id": id, "rid": "r-general", "msg": text,
                "ts": "2026-11-17T10:00:00.123Z",
                "u": {"_id": "u1", "username": "smoke", "name": "Smoke"},
                "md": [], "urls": []
            },
            "success": true
        })
        .to_string()
    }

    #[tokio::test]
    async fn send_message_contract() {
        let server = MockServer::start().await;
        let expected = serde_json::json!({
            "message": {"_id": "rt-test-1", "rid": "r-general", "msg": SEND_TEXT}
        });
        Mock::given(method("POST"))
            .and(path("/api/v1/chat.sendMessage"))
            .and(header("X-User-Id", USER))
            .and(header("X-Auth-Token", TOKEN))
            .and(wiremock::matchers::body_json(expected))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(send_answer("rt-test-1", SEND_TEXT), "application/json"),
            )
            .expect(1)
            .mount(&server)
            .await;
        let room = RoomId("r-general".to_string());
        let message = client_for(&server)
            .send_message(&room, "rt-test-1", SEND_TEXT)
            .await
            .expect("send works");
        assert_eq!(message.id, "rt-test-1");
        assert_eq!(message.room_id, room);
        assert_eq!(message.author, "smoke");
        assert_eq!(message.ts, 1_794_909_600);
        assert_eq!(message.body, SEND_TEXT);

        // One request, a POST: nothing else was sent to the server.
        let requests = server.received_requests().await.expect("recorded");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method.as_str(), "POST");
        assert_eq!(requests[0].url.path(), "/api/v1/chat.sendMessage");
    }

    #[tokio::test]
    async fn send_message_errors() {
        let room = RoomId("r-general".to_string());
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_string("{\"success\":false}"))
            .mount(&server)
            .await;
        let error = client_for(&server)
            .send_message(&room, "rt-test-1", SEND_TEXT)
            .await
            .expect_err("401 fails");
        assert_eq!(error, ApiError::Unauthorized);

        // A redirect is never followed: the target receives neither the text nor the credentials.
        let target = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(send_answer("rt-test-1", SEND_TEXT), "application/json"),
            )
            .mount(&target)
            .await;
        let origin = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(307).insert_header(
                "Location",
                format!("{}/api/v1/chat.sendMessage", target.uri()),
            ))
            .mount(&origin)
            .await;
        let error = client_for(&origin)
            .send_message(&room, "rt-test-1", SEND_TEXT)
            .await
            .expect_err("307 fails");
        assert_eq!(error, ApiError::Redirected);
        let received = target.received_requests().await.expect("recorded");
        assert!(received.is_empty(), "the redirect target got a request");
    }

    /// A POST of `send_message` answered with `status`, as the error it gives.
    async fn send_status_error(status: u16) -> ApiError {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(status).set_body_string("{\"success\":false}"))
            .mount(&server)
            .await;
        client_for(&server)
            .send_message(&RoomId("r-general".to_string()), "rt-test-1", "hi")
            .await
            .expect_err("a non-2xx status fails")
    }

    #[tokio::test]
    async fn send_rejected() {
        // Every send-path 4xx other than 401, 403, 404 and 429 is Rejected (D-12).
        for status in [400, 409, 413, 418] {
            assert_eq!(
                send_status_error(status).await,
                ApiError::Rejected,
                "{status}"
            );
        }
        assert_eq!(send_status_error(401).await, ApiError::Unauthorized);
        assert_eq!(send_status_error(403).await, ApiError::Forbidden);
        assert_eq!(send_status_error(404).await, ApiError::NotFound);
        assert_eq!(send_status_error(429).await, ApiError::RateLimited);
        assert_eq!(send_status_error(500).await, ApiError::Server(500));
        assert_eq!(send_status_error(503).await, ApiError::Server(503));
    }

    #[tokio::test]
    async fn send_undecodable_answer_is_bad_response() {
        // A 200 whose body is not a message stays BadResponse: only a 4xx status is Rejected.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_raw("not json", "application/json"))
            .mount(&server)
            .await;
        let error = client_for(&server)
            .send_message(&RoomId("r-general".to_string()), "rt-test-1", "hi")
            .await
            .expect_err("an undecodable answer fails");
        assert_eq!(error, ApiError::BadResponse);
    }

    #[tokio::test]
    async fn send_timeout_is_unconfirmed() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("{}")
                    .set_delay(Duration::from_millis(1500)),
            )
            .mount(&server)
            .await;
        let error = quick_client(&server.uri())
            .send_message(&RoomId("r-general".to_string()), "rt-test-1", "hi")
            .await
            .expect_err("the request times out");
        assert_eq!(error, ApiError::Timeout);
        assert!(error.outcome_unknown());
    }

    const EDIT_TEXT: &str = "SENTINEL-EDIT-4d1e";

    fn room_general() -> RoomId {
        RoomId("r-general".to_string())
    }

    #[tokio::test]
    async fn edit_message_contract() {
        let server = MockServer::start().await;
        let expected = serde_json::json!({
            "roomId": "r-general", "msgId": "m1", "text": EDIT_TEXT
        });
        let mut answer: serde_json::Value =
            serde_json::from_str(&send_answer("m1", EDIT_TEXT)).expect("json");
        answer["message"]["editedAt"] = serde_json::json!("2026-11-17T10:05:00.000Z");
        Mock::given(method("POST"))
            .and(path("/api/v1/chat.update"))
            .and(header("X-User-Id", USER))
            .and(header("X-Auth-Token", TOKEN))
            .and(wiremock::matchers::body_json(expected))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(answer.to_string(), "application/json"),
            )
            .expect(1)
            .mount(&server)
            .await;
        let message = client_for(&server)
            .edit_message(&room_general(), "m1", EDIT_TEXT)
            .await
            .expect("edit works");
        assert_eq!(message.id, "m1");
        assert_eq!(message.body, EDIT_TEXT);
        assert!(message.edited);
        assert_eq!(message.send, SendState::Sent);
        let requests = server.received_requests().await.expect("recorded");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url.path(), "/api/v1/chat.update");
    }

    #[tokio::test]
    async fn delete_message_contract() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/chat.delete"))
            .and(header("X-User-Id", USER))
            .and(header("X-Auth-Token", TOKEN))
            .and(wiremock::matchers::body_json(
                serde_json::json!({"roomId": "r-general", "msgId": "m1"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                "{\"_id\":\"m1\",\"ts\":1,\"message\":{\"_id\":\"m1\"},\"success\":true}",
                "application/json",
            ))
            .expect(1)
            .mount(&server)
            .await;
        client_for(&server)
            .delete_message(&room_general(), "m1")
            .await
            .expect("delete works");
    }

    /// The error of `chat.update` and of `chat.delete` for a POST answered with `status`.
    async fn edit_delete_status_errors(status: u16) -> (ApiError, ApiError) {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(status)
                    .set_body_string("{\"success\":false,\"error\":\"SENTINEL-SERVER-TEXT\"}"),
            )
            .mount(&server)
            .await;
        let client = client_for(&server);
        let edit = client
            .edit_message(&room_general(), "m1", EDIT_TEXT)
            .await
            .expect_err("a non-2xx status fails");
        let delete = client
            .delete_message(&room_general(), "m1")
            .await
            .expect_err("a non-2xx status fails");
        (edit, delete)
    }

    #[tokio::test]
    async fn edit_delete_error_kinds() {
        // Measured: every refusal is a 400 and is Rejected; 401, 403 and 404 keep their kinds.
        assert_eq!(
            edit_delete_status_errors(400).await,
            (ApiError::Rejected, ApiError::Rejected)
        );
        assert_eq!(
            edit_delete_status_errors(403).await,
            (ApiError::Forbidden, ApiError::Forbidden)
        );
        assert_eq!(
            edit_delete_status_errors(404).await,
            (ApiError::NotFound, ApiError::NotFound)
        );
        assert_eq!(
            edit_delete_status_errors(401).await,
            (ApiError::Unauthorized, ApiError::Unauthorized)
        );
        assert_eq!(
            edit_delete_status_errors(503).await,
            (ApiError::Server(503), ApiError::Server(503))
        );
        for (edit, delete) in [edit_delete_status_errors(400).await] {
            // An error is a payload-free enum: neither the server text nor the typed text is in it.
            for error in [edit, delete] {
                let shown = format!("{error:?} {error}");
                assert!(!shown.contains("SENTINEL"), "{shown}");
            }
        }
    }

    #[tokio::test]
    async fn edit_delete_timeout_and_lost_connection() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("{}")
                    .set_delay(Duration::from_millis(1500)),
            )
            .mount(&server)
            .await;
        let client = quick_client(&server.uri());
        assert_eq!(
            client
                .edit_message(&room_general(), "m1", EDIT_TEXT)
                .await
                .expect_err("times out"),
            ApiError::Timeout
        );
        assert_eq!(
            client
                .delete_message(&room_general(), "m1")
                .await
                .expect_err("times out"),
            ApiError::Timeout
        );

        let (url, handle) = dropping_server();
        let error = Client::new(&connection(&url, USER, TOKEN))
            .expect("client")
            .edit_message(&room_general(), "m1", EDIT_TEXT)
            .await
            .expect_err("the connection is dropped");
        handle.join().expect("server thread");
        assert_eq!(error, ApiError::ConnectionLost);
        let (url, handle) = dropping_server();
        let error = Client::new(&connection(&url, USER, TOKEN))
            .expect("client")
            .delete_message(&room_general(), "m1")
            .await
            .expect_err("the connection is dropped");
        handle.join().expect("server thread");
        assert_eq!(error, ApiError::ConnectionLost);
    }

    #[test]
    fn outcome_unknown_table() {
        let unknown = [
            ApiError::Timeout,
            ApiError::Server(502),
            ApiError::Server(500),
            ApiError::ConnectionLost,
            ApiError::BadResponse,
            ApiError::TooLarge,
        ];
        let known = [
            ApiError::Unauthorized,
            ApiError::Forbidden,
            ApiError::NotFound,
            ApiError::RateLimited,
            ApiError::Redirected,
            ApiError::Rejected,
            ApiError::Unreachable,
            ApiError::Client,
        ];
        for error in unknown {
            assert!(error.outcome_unknown(), "{error:?}");
        }
        for error in known {
            assert!(!error.outcome_unknown(), "{error:?}");
        }
    }

    /// A one-shot server that accepts a connection, reads the request and closes it without any
    /// answer: the connection breaks after it was made.
    fn dropping_server() -> (String, std::thread::JoinHandle<()>) {
        use std::io::Read;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut head = [0u8; 4096];
            let _ = stream.read(&mut head);
            // Dropping the stream closes it with no response.
        });
        (format!("http://127.0.0.1:{port}"), handle)
    }

    #[tokio::test]
    async fn connection_reset_is_connection_lost() {
        // On the send path and on a GET alike; a connect failure stays Unreachable (error_mapping).
        let (url, handle) = dropping_server();
        let error = Client::new(&connection(&url, USER, TOKEN))
            .expect("client")
            .send_message(&RoomId("r-general".to_string()), "rt-test-1", "hi")
            .await
            .expect_err("the connection is dropped");
        handle.join().expect("server thread");
        assert_eq!(error, ApiError::ConnectionLost);
        assert!(error.outcome_unknown());

        let (url, handle) = dropping_server();
        let client = Client::new(&connection(&url, USER, TOKEN)).expect("client");
        let error = mapped_error(&client).await;
        handle.join().expect("server thread");
        assert_eq!(error, ApiError::ConnectionLost);
    }

    #[tokio::test]
    async fn get_message_found_and_absent() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/chat.getMessage"))
            .and(wiremock::matchers::query_param("msgId", "m1"))
            .and(header("X-User-Id", USER))
            .and(header("X-Auth-Token", TOKEN))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(send_answer("m1", "stored"), "application/json"),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/chat.getMessage"))
            .and(wiremock::matchers::query_param("msgId", "gone"))
            .respond_with(ResponseTemplate::new(400).set_body_string("{\"success\":false}"))
            .mount(&server)
            .await;
        let room = RoomId("r-general".to_string());
        let client = client_for(&server);
        let message = client.get_message(&room, "m1").await.expect("found");
        assert_eq!(message.id, "m1");
        assert_eq!(message.room_id, room);
        assert_eq!(message.author, "smoke");
        assert_eq!(message.body, "stored");
        assert_eq!(message.send, SendState::Sent);

        let error = client.get_message(&room, "gone").await.expect_err("absent");
        assert_eq!(error, ApiError::BadResponse, "a GET 400 stays BadResponse");

        let requests = server.received_requests().await.expect("recorded");
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|r| r.method.as_str() == "GET"));
        assert_eq!(
            query_of(&requests[0]),
            [("msgId".to_string(), "m1".to_string())]
        );
    }

    const HISTORY: &str = include_str!("fixtures/history.json");

    #[test]
    fn history_method_by_kind() {
        assert_eq!(history_method(RoomKind::Channel, false), "channels.history");
        assert_eq!(
            history_method(RoomKind::PrivateGroup, true),
            "groups.history"
        );
        assert_eq!(history_method(RoomKind::DirectMessage, false), "im.history");
        assert_eq!(history_method(RoomKind::Discussion, true), "groups.history");
        assert_eq!(history_method(RoomKind::Team, false), "channels.history");
    }

    #[tokio::test]
    async fn history_query() {
        let server = MockServer::start().await;
        mount_ok(&server, "groups.history", HISTORY).await;
        let room = RoomId("r-devteam".to_string());
        let page = client_for(&server)
            .load_history(&room, RoomKind::PrivateGroup, true, 50, None)
            .await
            .expect("history loads");
        let ids: Vec<&str> = page.messages.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["m1", "m2", "m3", "m4"]);
        assert_eq!(page.oldest_ts.as_deref(), Some("2026-11-16T09:59:00.000Z"));

        let requests = server.received_requests().await.expect("recorded");
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        assert_eq!(request.method.as_str(), "GET");
        assert_eq!(request.url.path(), "/api/v1/groups.history");
        let query: Vec<(String, String)> = request
            .url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        assert_eq!(
            query,
            [
                ("roomId".to_string(), "r-devteam".to_string()),
                ("count".to_string(), "50".to_string())
            ]
        );
        assert_eq!(request.headers.get("x-user-id").unwrap(), USER);
        assert_eq!(request.headers.get("x-auth-token").unwrap(), TOKEN);
    }

    #[tokio::test]
    async fn search_room_query_and_order() {
        let server = MockServer::start().await;
        mount_ok(&server, "chat.search", HISTORY).await;
        let room = RoomId("r-devteam".to_string());
        let found = client_for(&server)
            .search_room(&room, "from:bob  two", 50, 50)
            .await
            .expect("search loads");
        let ids: Vec<&str> = found.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["m4", "m3", "m2", "m1"], "server order, newest first");

        let requests = server.received_requests().await.expect("recorded");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url.path(), "/api/v1/chat.search");
        assert_eq!(
            query_of(&requests[0]),
            [
                ("roomId".to_string(), "r-devteam".to_string()),
                ("searchText".to_string(), "from:bob  two".to_string()),
                ("count".to_string(), "50".to_string()),
                ("offset".to_string(), "50".to_string()),
            ]
        );
        assert_eq!(requests[0].headers.get("x-auth-token").unwrap(), TOKEN);
    }

    #[tokio::test]
    async fn search_room_empty_and_errors() {
        let server = MockServer::start().await;
        mount_ok(&server, "chat.search", "{\"messages\":[],\"success\":true}").await;
        let room = RoomId("r-devteam".to_string());
        let found = client_for(&server)
            .search_room(&room, "zzz", 0, 50)
            .await
            .expect("empty is fine");
        assert!(found.is_empty());

        let refused = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/chat.search"))
            .respond_with(ResponseTemplate::new(400).set_body_string("{\"success\":false}"))
            .mount(&refused)
            .await;
        let error = client_for(&refused)
            .search_room(&room, "x", 0, 50)
            .await
            .expect_err("400");
        assert_eq!(error, ApiError::Rejected);
    }

    #[tokio::test]
    async fn history_older_page_sends_latest_verbatim() {
        let server = MockServer::start().await;
        mount_ok(&server, "channels.history", HISTORY).await;
        let room = RoomId("r-general".to_string());
        client_for(&server)
            .load_history(
                &room,
                RoomKind::Channel,
                false,
                25,
                Some("2026-11-16T09:59:00.000Z"),
            )
            .await
            .expect("history loads");
        let requests = server.received_requests().await.expect("recorded");
        let latest = requests[0]
            .url
            .query_pairs()
            .find(|(k, _)| k == "latest")
            .map(|(_, v)| v.into_owned());
        assert_eq!(latest.as_deref(), Some("2026-11-16T09:59:00.000Z"));
    }

    fn query_of(request: &wiremock::Request) -> Vec<(String, String)> {
        request
            .url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect()
    }

    #[tokio::test]
    async fn latest_param() {
        let server = MockServer::start().await;
        mount_ok(&server, "channels.history", HISTORY).await;
        let room = RoomId("r-general".to_string());
        client_for(&server)
            .load_history(
                &room,
                RoomKind::Channel,
                false,
                50,
                Some("2026-11-16T10:00:00.300Z"),
            )
            .await
            .expect("history loads");
        let requests = server.received_requests().await.expect("recorded");
        assert_eq!(requests.len(), 1);
        assert_eq!(
            query_of(&requests[0]),
            [
                ("roomId".to_string(), "r-general".to_string()),
                ("count".to_string(), "50".to_string()),
                ("latest".to_string(), "2026-11-16T10:00:00.300Z".to_string()),
            ]
        );
        assert!(
            query_of(&requests[0]).iter().all(|(k, _)| k != "inclusive"),
            "an older page never asks for the boundary message again"
        );
    }

    #[tokio::test]
    async fn count_above_cap() {
        // A server that caps the page at 100 answers a count of 500 with 100 messages.
        let body = serde_json::json!({
            "success": true,
            "messages": (0..100)
                .map(|i| serde_json::json!({
                    "_id": format!("m{i:03}"),
                    "msg": "x",
                    "ts": format!("2026-11-16T09:{:02}:{:02}.000Z", i / 60, i % 60),
                    "u": { "_id": "u1", "username": "alice" },
                }))
                .collect::<Vec<_>>(),
        })
        .to_string();
        let server = MockServer::start().await;
        mount_ok(&server, "channels.history", &body).await;
        let room = RoomId("r-general".to_string());
        let page = client_for(&server)
            .load_history(&room, RoomKind::Channel, false, 500, None)
            .await
            .expect("history loads");
        let requests = server.received_requests().await.expect("recorded");
        assert!(query_of(&requests[0]).contains(&("count".to_string(), "500".to_string())));
        assert_eq!(
            page.messages.len(),
            100,
            "the short page is returned in full"
        );

        // A shorter-than-asked page is not the end of history; only an empty one is.
        let mut app = App::new(Size::new(80, 24));
        app.history
            .insert(room.clone(), crate::app::history::HistoryState::Loading);
        update(
            &mut app,
            Action::HistoryLoaded {
                room: room.clone(),
                messages: page.messages,
                oldest_ts: page.oldest_ts,
                older: false,
            },
        );
        assert!(matches!(
            app.history.get(&room),
            Some(crate::app::history::HistoryState::Loaded {
                exhausted: false,
                ..
            })
        ));
    }

    /// A request path segment that must never come back in an error.
    const ROOM_SENTINEL: &str = "SENTINEL-ROOM-5d1e";

    /// A client with a 200 ms connect and total deadline, for the failure-class tests.
    fn quick_client(url: &str) -> Client {
        let conn = connection(url, USER, TOKEN);
        let short = Duration::from_millis(200);
        Client::with_timeouts(&conn, short, short).expect("client")
    }

    /// One GET of `subscriptions.get` carrying the sentinel in the query, as the error it gives.
    async fn mapped_error(client: &Client) -> ApiError {
        client
            .get_json::<wire::SubscriptionsResp>(
                "subscriptions.get",
                &[("roomId", ROOM_SENTINEL.into())],
            )
            .await
            .map(|_| ())
            .expect_err("the request must fail")
    }

    async fn respond_with(template: ResponseTemplate) -> (MockServer, Client) {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/subscriptions.get"))
            .respond_with(template)
            .mount(&server)
            .await;
        let client = quick_client(&server.uri());
        (server, client)
    }

    #[tokio::test]
    async fn error_mapping() {
        let json = |code: u16| ResponseTemplate::new(code).set_body_string("{\"success\":false}");
        let nine_mib = "x".repeat(9 * 1024 * 1024);
        let cases: Vec<(&str, ResponseTemplate, ApiError, &str)> = vec![
            (
                "401",
                json(401),
                ApiError::Unauthorized,
                "unauthorized: check the user id and token",
            ),
            (
                "403",
                json(403),
                ApiError::Forbidden,
                "forbidden: the token lacks permission",
            ),
            (
                "404",
                json(404),
                ApiError::NotFound,
                "not found: check server_url (the Rocket.Chat base URL)",
            ),
            (
                "429",
                json(429),
                ApiError::RateLimited,
                "rate limited by the server: wait a minute, then :reconnect",
            ),
            (
                "307",
                ResponseTemplate::new(307).insert_header("Location", "http://127.0.0.1:9/x"),
                ApiError::Redirected,
                "server redirected: check server_url (http vs https, path)",
            ),
            (
                "500",
                json(500),
                ApiError::Server(500),
                "server error (HTTP 500): try :reconnect later",
            ),
            (
                "502",
                json(502),
                ApiError::Server(502),
                "server error (HTTP 502): try :reconnect later",
            ),
            (
                "418",
                json(418),
                ApiError::BadResponse,
                "unexpected response: is server_url a Rocket.Chat server?",
            ),
            (
                "200 not json",
                ResponseTemplate::new(200).set_body_raw("not json", "application/json"),
                ApiError::BadResponse,
                "unexpected response: is server_url a Rocket.Chat server?",
            ),
            (
                "200 nine MiB",
                ResponseTemplate::new(200).set_body_raw(nine_mib, "application/json"),
                ApiError::TooLarge,
                "server response too large",
            ),
            (
                "slow",
                ResponseTemplate::new(200)
                    .set_body_string("{}")
                    .set_delay(Duration::from_millis(1500)),
                ApiError::Timeout,
                "request timed out: check the network",
            ),
        ];
        for (label, template, expected, text) in cases {
            let (_server, client) = respond_with(template).await;
            let error = mapped_error(&client).await;
            assert_eq!(error, expected, "{label}");
            assert_eq!(error.to_string(), text, "{label}");
            for shown in [error.to_string(), format!("{error:?}")] {
                assert!(!shown.contains(ROOM_SENTINEL), "{label}: {shown}");
                assert!(!shown.contains(TOKEN), "{label}: {shown}");
            }
        }

        // A port nothing listens on: the connection is refused.
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
            listener.local_addr().expect("addr").port()
        };
        let client = quick_client(&format!("http://127.0.0.1:{port}"));
        let error = mapped_error(&client).await;
        assert_eq!(error, ApiError::Unreachable);
        assert_eq!(
            error.to_string(),
            "cannot connect (unreachable or TLS error): check server_url"
        );
        assert!(!format!("{error:?}").contains(ROOM_SENTINEL));
    }

    /// A body of more than the cap with no Content-Length (chunked) is cut off as TooLarge too.
    #[tokio::test]
    async fn chunked_oversized_body_is_too_large() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut head = [0u8; 2048];
            let _ = stream.read(&mut head);
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n",
            );
            let chunk = vec![b'x'; 1024 * 1024];
            for _ in 0..10 {
                if stream
                    .write_all(format!("{:x}\r\n", chunk.len()).as_bytes())
                    .is_err()
                    || stream.write_all(&chunk).is_err()
                    || stream.write_all(b"\r\n").is_err()
                {
                    return; // the client gave up at the cap, as intended
                }
            }
            let _ = stream.write_all(b"0\r\n\r\n");
        });
        let client = Client::with_timeouts(
            &connection(&format!("http://127.0.0.1:{port}"), USER, TOKEN),
            Duration::from_secs(2),
            Duration::from_secs(10),
        )
        .expect("client");
        assert_eq!(mapped_error(&client).await, ApiError::TooLarge);
        server.join().expect("server thread");
    }

    #[tokio::test]
    async fn no_redirect_follow() {
        let origin_b = MockServer::start().await;
        Mock::given(wiremock::matchers::any())
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .expect(0)
            .mount(&origin_b)
            .await;
        let origin_a = MockServer::start().await;
        let target = format!("{}/api/v1/subscriptions.get", origin_b.uri());
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(307).insert_header("Location", target.as_str()))
            .mount(&origin_a)
            .await;
        let result = client_for(&origin_a).load_rooms().await;
        assert!(matches!(result, Err(ApiError::Redirected)));
        let received = origin_b.received_requests().await.expect("recorded");
        assert!(
            received.is_empty(),
            "the second origin got {} request(s): the token must never leave server_url",
            received.len()
        );
        origin_b.verify().await;
    }

    #[tokio::test]
    async fn only_get_requests() {
        let server = MockServer::start().await;
        mount_rooms(&server).await;
        mount_ok(&server, "users.getPreferences", PREFS).await;
        mount_ok(&server, "channels.history", HISTORY).await;
        let client = client_for(&server);
        client.load_rooms().await.expect("rooms");
        client
            .load_history(
                &RoomId("r-general".to_string()),
                RoomKind::Channel,
                false,
                50,
                None,
            )
            .await
            .expect("history");
        let requests = server.received_requests().await.expect("recorded");
        assert!(requests.len() >= 4, "{}", requests.len());
        assert!(
            requests.iter().all(|r| r.method.as_str() == "GET"),
            "Phase 3 sends GET only"
        );
    }

    #[test]
    fn client_debug_redacts() {
        let conn = connection("http://127.0.0.1:3100", "SENTINEL-USER-ID", TOKEN);
        let client = Client::new(&conn).expect("client");
        let shown = format!("{client:?}");
        assert!(!shown.contains(TOKEN), "{shown}");
        assert!(!shown.contains("SENTINEL-USER-ID"), "{shown}");
        assert!(!shown.contains("3100"), "{shown}");
    }

    #[test]
    fn history_page_debug_redacts() {
        let page = HistoryPage {
            messages: vec![Message {
                edited: false,
                plain_text: true,
                id: "m".to_string(),
                room_id: RoomId("r".to_string()),
                author: "a".to_string(),
                ts: 0,
                body: "SENTINEL-BODY-7f3a".to_string(),
                kind: MessageKind::Normal,
                send: SendState::Sent,
            }],
            oldest_ts: Some("2026-11-16T09:59:00.000Z".to_string()),
        };
        let shown = format!("{page:?}");
        assert!(!shown.contains("SENTINEL-BODY-7f3a"), "{shown}");
        assert!(shown.contains('1'));
    }
}
