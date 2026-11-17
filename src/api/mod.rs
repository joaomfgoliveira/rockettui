//! The Rocket.Chat REST client (AUTH-01, ROOM-01). Requests carry the Personal Access Token in two
//! headers, never follow redirects, and fail with a typed `ApiError` whose text is fixed.
//!
//! Security rules this module keeps (T-03-01, T-03-02, T-03-04, T-03-05):
//! - both header values are marked sensitive and the token never leaves `Client`;
//! - `redirect::Policy::none()`: reqwest would forward the custom auth headers to another host;
//! - the default rustls TLS verification is the only mode, there is no switch to weaken it;
//! - a `reqwest::Error` is never formatted (its text holds the request URL), it is mapped to an
//!   `ApiError` in one place;
//! - every body is capped at `MAX_BODY_BYTES` and decoded leniently.

pub mod map;
pub mod wire;

use crate::config::connection::Connection;
use crate::data::{Folder, Room};
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
        }
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
        ApiError::Unreachable
    }
}

/// The joined rooms and the sidebar folders built from one load.
pub struct RoomList {
    pub rooms: Vec<Room>,
    pub folders: Vec<Folder>,
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
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
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

    /// GET `method` with `query`, check the HTTP status, read the body under the size cap and
    /// decode it. Only GET is ever sent in Phase 3.
    async fn get_json<T: DeserializeOwned>(
        &self,
        method: &str,
        query: &[(&str, String)],
    ) -> Result<T, ApiError> {
        let mut request = self.http.get(self.endpoint(method));
        if !query.is_empty() {
            request = request.query(query);
        }
        let mut response = request.send().await.map_err(|e| send_error(&e))?;
        if let Some(error) = status_error(response.status()) {
            return Err(error);
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

    /// Load the user's joined rooms: `subscriptions.get` and `rooms.get` run concurrently and are
    /// joined on the room id. All or nothing: if either fails the whole load fails, so a room
    /// list without its `encrypted` flags is never applied.
    pub async fn load_rooms(&self) -> Result<RoomList, ApiError> {
        let (subs, records) = tokio::join!(
            self.get_json::<wire::SubscriptionsResp>("subscriptions.get", &[]),
            self.get_json::<wire::RoomsResp>("rooms.get", &[]),
        );
        let subs = subs?.update;
        let records = records?.update;
        let rooms = map::rooms(&subs, &records);
        Ok(RoomList {
            rooms,
            folders: Vec::new(),
        })
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
        ] {
            assert!(!format!("{error:?}").contains(TOKEN));
            assert!(!error.to_string().contains(TOKEN));
        }
    }

    /// All three endpoints answer; `prefs` mounts users.getPreferences with the given body.
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
}
