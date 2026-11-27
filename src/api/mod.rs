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
/// Largest custom emoji image accepted. Real ones are a few kilobytes; the server's own limit for
/// an upload is far above what a message-sized image needs.
pub const MAX_EMOJI_BYTES: usize = 512 * 1024;
/// Largest original image downloaded for the full-size view.
pub const MAX_VIEW_BYTES: usize = 32 * 1024 * 1024;
/// Largest attachment image downloaded. A reported size over it is skipped without a request.
pub const MAX_ATTACHMENT_BYTES: usize = 8 * 1024 * 1024;

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
    let body = read_capped(request, client_error, MAX_BODY_BYTES).await?;
    serde_json::from_slice(&body).map_err(|_| ApiError::BadResponse)
}

/// Send `request`, check the status (a redirect is an error, never followed) and read the body,
/// refusing one over `limit` bytes both by its announced length and while reading.
async fn read_capped(
    request: reqwest::RequestBuilder,
    client_error: ApiError,
    limit: usize,
) -> Result<Vec<u8>, ApiError> {
    let mut response = request.send().await.map_err(|e| send_error(&e))?;
    if let Some(error) = status_error(response.status()) {
        return Err(if error == ApiError::BadResponse {
            client_error
        } else {
            error
        });
    }
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err(ApiError::TooLarge);
    }
    let mut body: Vec<u8> = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| send_error(&e))? {
        if body.len() + chunk.len() > limit {
            return Err(ApiError::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
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
    /// The parent of the thread this message replies to.
    #[serde(skip_serializing_if = "Option::is_none")]
    tmid: Option<&'a str>,
    /// A thread reply that is also shown in the channel.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    tshow: bool,
}

/// The thread a sent message replies to: its parent's id and whether it is also shown in the channel.
#[derive(Clone, Copy)]
pub struct ThreadTarget<'a> {
    pub parent_id: &'a str,
    pub also_in_channel: bool,
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

/// The body of `subscriptions.read`: the room id.
#[derive(serde::Serialize)]
struct ReadBody<'a> {
    rid: &'a str,
}

/// The body of `chat.delete`: the room id and the message id.
#[derive(serde::Serialize)]
struct DeleteBody<'a> {
    #[serde(rename = "roomId")]
    room_id: &'a str,
    #[serde(rename = "msgId")]
    msg_id: &'a str,
}

/// The body of `chat.react`: the message id, the emoji as `:name:` and whether to add the reaction
/// (`true`) or take the user's own back (`false`). Always explicit, never left to the server to toggle.
#[derive(serde::Serialize)]
struct ReactBody<'a> {
    #[serde(rename = "messageId")]
    message_id: &'a str,
    emoji: &'a str,
    #[serde(rename = "shouldReact")]
    should_react: bool,
}

/// The body of `chat.followMessage` and `chat.unfollowMessage`: the id of the thread's parent.
#[derive(serde::Serialize)]
struct FollowBody<'a> {
    mid: &'a str,
}

/// The body of `chat.readThread`: the id of the thread's parent. Measured: a `rid` next to it is
/// refused as invalid params on both servers.
#[derive(serde::Serialize)]
struct ReadThreadBody<'a> {
    tmid: &'a str,
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

/// One page of search results, newest first, with the raw server timestamp of each result (`None`
/// when it could not be read). The raw value is what a context request sends as `latest`: the
/// whole seconds of `Message.ts` are not enough to cut history exactly at the result.
#[derive(Clone, PartialEq)]
pub struct SearchHits {
    pub messages: Vec<Message>,
    pub raw_ts: Vec<Option<String>>,
}

impl From<Vec<Message>> for SearchHits {
    /// Results with no raw timestamps, which can be listed but not opened.
    fn from(messages: Vec<Message>) -> Self {
        let raw_ts = vec![None; messages.len()];
        SearchHits { messages, raw_ts }
    }
}

/// Redacting `Debug`: the number of results only, never their text.
impl fmt::Debug for SearchHits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SearchHits(<{} messages>)", self.messages.len())
    }
}

/// One page of a thread's replies, oldest first, and the number of replies the thread has in all.
#[derive(Clone, PartialEq)]
pub struct ThreadPage {
    pub messages: Vec<Message>,
    pub total: u32,
}

/// Redacting `Debug`: the counts only, never the text.
impl fmt::Debug for ThreadPage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ThreadPage(<{} of {} messages>)",
            self.messages.len(),
            self.total
        )
    }
}

/// Which threads `chat.getThreadsList` lists. `All` sends no `type` (measured on 8.9.1 and 9.0.0:
/// `type=all` is a 400).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ThreadFilter {
    #[default]
    All,
    Following,
    Unread,
}

impl ThreadFilter {
    /// The `type` query value, `None` for all threads.
    fn query_value(self) -> Option<&'static str> {
        match self {
            ThreadFilter::All => None,
            ThreadFilter::Following => Some("following"),
            ThreadFilter::Unread => Some("unread"),
        }
    }

    /// The next filter, wrapping: all, following, unread.
    pub fn next(self) -> ThreadFilter {
        match self {
            ThreadFilter::All => ThreadFilter::Following,
            ThreadFilter::Following => ThreadFilter::Unread,
            ThreadFilter::Unread => ThreadFilter::All,
        }
    }

    /// The previous filter, wrapping.
    pub fn prev(self) -> ThreadFilter {
        match self {
            ThreadFilter::All => ThreadFilter::Unread,
            ThreadFilter::Following => ThreadFilter::All,
            ThreadFilter::Unread => ThreadFilter::Following,
        }
    }
}

/// Longest first line a thread row keeps, in characters. The list holds little text.
pub const THREAD_ROW_TEXT_CHARS: usize = 200;

/// One thread of the thread list: the parent's author, the bounded first line of its text, the
/// reply count, the time of the last reply (whole seconds) and the ids that follow it. The text is
/// unsanitized server text: the drawing code sanitizes it. Not a `Message`.
#[derive(Clone, PartialEq)]
pub struct ThreadRow {
    pub id: String,
    pub author: String,
    pub first_line: String,
    pub replies: u32,
    pub last_reply: Option<i64>,
    pub followers: Vec<String>,
}

impl ThreadRow {
    /// Whether user `own_id` follows the thread.
    pub fn followed_by(&self, own_id: &str) -> bool {
        self.followers.iter().any(|id| id == own_id)
    }
}

/// Redacting `Debug`: the id and the counts, never the text.
impl fmt::Debug for ThreadRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ThreadRow({}, {} replies)", self.id, self.replies)
    }
}

/// One page of the thread list, in the server's order, and the number of threads that match in all.
#[derive(Clone, PartialEq)]
pub struct ThreadListPage {
    pub rows: Vec<ThreadRow>,
    pub total: u32,
}

/// Redacting `Debug`: the counts only.
impl fmt::Debug for ThreadListPage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ThreadListPage(<{} of {} threads>)",
            self.rows.len(),
            self.total
        )
    }
}

/// The room's history around one message, oldest first. `selected` is the index of that message;
/// `newest_raw` is the raw timestamp of the newest message held, the `oldest` value of the page
/// after the window; `reaches_newest` is true when nothing newer than the window exists;
/// `oldest_raw` is the raw timestamp of the oldest message held, the `latest` value of the next
/// older page.
#[derive(Clone, PartialEq)]
pub struct ContextWindow {
    pub messages: Vec<Message>,
    pub selected: usize,
    pub newest_raw: Option<String>,
    pub reaches_newest: bool,
    pub oldest_raw: Option<String>,
}

/// Redacting `Debug`: counts and flags only.
impl fmt::Debug for ContextWindow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ContextWindow(<{} messages>, selected={}, reaches_newest={})",
            self.messages.len(),
            self.selected,
            self.reaches_newest
        )
    }
}

/// One page of messages after a point in time, oldest first. `newest_raw` is the raw timestamp of
/// the newest message in it (`None` for an empty page); `reached_newest` is true when no message
/// newer than the page exists.
#[derive(Clone, PartialEq)]
pub struct NewerPage {
    pub messages: Vec<Message>,
    pub newest_raw: Option<String>,
    pub reached_newest: bool,
}

/// Redacting `Debug`: the count and the flag only.
impl fmt::Debug for NewerPage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "NewerPage(<{} messages>, reached_newest={})",
            self.messages.len(),
            self.reached_newest
        )
    }
}

/// The most the history endpoint returns for one request.
const HISTORY_MAX: u16 = 100;

/// The most one-message probes a newer page may spend finding how many messages follow a point.
/// Doubling then bisecting needs about `2 * log2(M)`, so 24 covers any room the server can hold; a
/// server that keeps answering "more" is cut off as a bad response.
const NEWER_PROBE_LIMIT: u32 = 24;

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
        Ok(self.me_with_id().await?.map(|(name, _)| name))
    }

    /// The same `GET /api/v1/me` with the user's own id as well, which is what a thread's follower
    /// list is compared with. The id is `None` when the server does not send it.
    pub async fn me_with_id(&self) -> Result<Option<(String, Option<String>)>, ApiError> {
        let resp: wire::MeResp = self.get_json("me", &[]).await?;
        let id = resp.id.filter(|id| !id.is_empty());
        Ok(resp
            .username
            .filter(|name| !name.is_empty())
            .map(|name| (name, id)))
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
    ) -> Result<SearchHits, ApiError> {
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

    /// One page of the replies of thread `tmid`: `GET chat.getThreadMessages` with `tmid`, `count`
    /// and `offset`. Measured on 8.9.1 and 9.0.0: the replies come oldest first, `total` is the
    /// whole thread, and the parent is not among them. A refused request is `ApiError::Rejected`
    /// and no server text reaches the error.
    pub async fn load_thread(
        &self,
        room: &RoomId,
        tmid: &str,
        offset: u32,
        count: u16,
    ) -> Result<ThreadPage, ApiError> {
        let params = [
            ("tmid", tmid.to_string()),
            ("count", count.to_string()),
            ("offset", offset.to_string()),
        ];
        let request = self
            .http
            .get(self.endpoint("chat.getThreadMessages"))
            .query(&params);
        let resp: wire::ThreadResp = read_json(request, ApiError::Rejected).await?;
        Ok(ThreadPage {
            messages: resp
                .messages
                .into_iter()
                .map(|msg| map::message(room, msg))
                .collect(),
            total: resp.total,
        })
    }

    /// One page of the threads of `room`: `GET chat.getThreadsList` with `rid`, `offset`, `count`,
    /// `type` (`following` or `unread`, absent for all) and `text` when `text` is not empty. The
    /// server matches the text literally (measured on 8.9.1 and 9.0.0), so it is sent as typed. A
    /// refused request is `ApiError::Rejected`; the search text never reaches an error.
    pub async fn list_threads(
        &self,
        room: &RoomId,
        filter: ThreadFilter,
        text: &str,
        offset: u32,
        count: u16,
    ) -> Result<ThreadListPage, ApiError> {
        let mut params = vec![
            ("rid", room.0.clone()),
            ("count", count.to_string()),
            ("offset", offset.to_string()),
        ];
        if let Some(kind) = filter.query_value() {
            params.push(("type", kind.to_string()));
        }
        if !text.is_empty() {
            params.push(("text", text.to_string()));
        }
        let request = self
            .http
            .get(self.endpoint("chat.getThreadsList"))
            .query(&params);
        let resp: wire::ThreadsListResp = read_json(request, ApiError::Rejected).await?;
        Ok(ThreadListPage {
            rows: map::thread_rows(resp.threads),
            total: resp.total,
        })
    }

    /// One `*.history` request for the messages after `raw_ts` (`oldest`, exclusive), newest first
    /// on the wire. `offset` skips that many of the newest ones, which is how the messages next to
    /// `raw_ts` are reached (measured on 8.9.1 and 9.0.0).
    async fn history_after(
        &self,
        method: &str,
        room: &RoomId,
        raw_ts: &str,
        count: u16,
        offset: u32,
    ) -> Result<Vec<wire::WireMsg>, ApiError> {
        let mut query = vec![
            ("roomId", room.0.clone()),
            ("oldest", raw_ts.to_string()),
            ("count", count.to_string()),
        ];
        if offset > 0 {
            query.push(("offset", offset.to_string()));
        }
        let resp: wire::HistoryResp = self.get_json(method, &query).await?;
        Ok(resp.messages)
    }

    /// The nearest `n` (at most 100) messages after `raw_ts`, oldest first. The server returns the
    /// newest messages after a time, so the ones next to it are found by counting: the first
    /// request asks for 100, and a short answer is everything after `raw_ts`. Otherwise `M`, how
    /// many follow, is found by doubling an offset and bisecting with one-message requests, and
    /// the page is read at `offset = M - n`. More than `NEWER_PROBE_LIMIT` probes is
    /// `ApiError::BadResponse`.
    pub async fn load_newer(
        &self,
        room: &RoomId,
        kind: RoomKind,
        private: bool,
        raw_ts: &str,
        n: u16,
    ) -> Result<NewerPage, ApiError> {
        let method = history_method(kind, private);
        let n = n.clamp(1, HISTORY_MAX);
        let first = self
            .history_after(method, room, raw_ts, HISTORY_MAX, 0)
            .await?;
        let (taken, reached_newest) = if first.len() < usize::from(HISTORY_MAX) {
            // Every message after `raw_ts` is in the answer.
            let all = map::history_with_raw(room, first);
            let reached = all.len() <= usize::from(n);
            (
                all.into_iter().take(usize::from(n)).collect::<Vec<_>>(),
                reached,
            )
        } else {
            // At least 100 follow. Item `lo` exists; find the first offset with no item: that
            // offset is M.
            let mut probes = 0u32;
            let mut exists = async |offset: u32| -> Result<bool, ApiError> {
                probes += 1;
                if probes > NEWER_PROBE_LIMIT {
                    return Err(ApiError::BadResponse);
                }
                Ok(!self
                    .history_after(method, room, raw_ts, 1, offset)
                    .await?
                    .is_empty())
            };
            let mut lo = u32::from(HISTORY_MAX) - 1;
            let mut hi = (lo + 1) * 2 - 1;
            while exists(hi).await? {
                lo = hi;
                hi = (lo + 1) * 2 - 1;
            }
            while hi - lo > 1 {
                let mid = lo + (hi - lo) / 2;
                if exists(mid).await? {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let total = hi;
            let take = u32::from(n).min(total);
            let page = self
                .history_after(method, room, raw_ts, take as u16, total - take)
                .await?;
            (map::history_with_raw(room, page), total <= u32::from(n))
        };
        let newest_raw = taken.last().and_then(|(_, raw)| raw.clone());
        Ok(NewerPage {
            messages: taken.into_iter().map(|(m, _)| m).collect(),
            newest_raw,
            reached_newest,
        })
    }

    /// The history around message `id` of `room`, whose raw server timestamp is `raw_ts`: the
    /// message with up to `before` messages before it (`latest=raw_ts&inclusive=true`) and up to
    /// `after` messages after it (`load_newer`), however many newer ones exist. The two sides run
    /// together. The message must be in the older answer, else it is gone (`ApiError::NotFound`);
    /// a failure of either side fails the whole load.
    #[allow(clippy::too_many_arguments)] // one request: where, which message, how much
    pub async fn load_context(
        &self,
        room: &RoomId,
        kind: RoomKind,
        private: bool,
        id: &str,
        raw_ts: &str,
        before: u16,
        after: u16,
    ) -> Result<ContextWindow, ApiError> {
        let older_query = [
            ("roomId", room.0.clone()),
            ("latest", raw_ts.to_string()),
            ("inclusive", "true".to_string()),
            ("count", (u32::from(before) + 1).to_string()),
        ];
        let (older, newer) = tokio::join!(
            self.get_json::<wire::HistoryResp>(history_method(kind, private), &older_query),
            self.load_newer(room, kind, private, raw_ts, after),
        );
        let older = map::history(room, older?.messages);
        let newer = newer?;
        let mut messages = older.messages;
        let Some(at) = messages.iter().position(|m| m.id == id) else {
            return Err(ApiError::NotFound);
        };
        // The answer is `latest`-bounded, so nothing should follow the message; cut anyway.
        messages.truncate(at + 1);
        let newest_raw = newer.newest_raw.or_else(|| Some(raw_ts.to_string()));
        messages.extend(newer.messages.into_iter().filter(|m| m.id != id));
        Ok(ContextWindow {
            messages,
            selected: at,
            newest_raw,
            reaches_newest: newer.reached_newest,
            oldest_raw: older.oldest_ts,
        })
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
        thread: Option<ThreadTarget<'_>>,
    ) -> Result<Message, ApiError> {
        let body = SendBody {
            message: SendMsg {
                id,
                rid: &room.0,
                msg: text,
                tmid: thread.map(|t| t.parent_id),
                tshow: thread.is_some_and(|t| t.also_in_channel),
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

    /// Mark `room` read for the user with `POST subscriptions.read`, the call the official clients
    /// make. The answer body is not read beyond being valid JSON; any failure is an `ApiError`
    /// and the caller ignores it (the room just stays unread).
    pub async fn mark_read(&self, room: &RoomId) -> Result<(), ApiError> {
        let body = ReadBody { rid: &room.0 };
        let request = self
            .http
            .post(self.endpoint("subscriptions.read"))
            .json(&body);
        let _: serde::de::IgnoredAny = read_json(request, ApiError::Rejected).await?;
        Ok(())
    }

    /// Mark thread `tmid` (the id of its parent message) read for the user with
    /// `POST chat.readThread`. It exists on 8.9.1 and 9.0.0 (it replaces the DDP method
    /// `readThreads`, removed in 9.0.0). The answer body is not read beyond being valid JSON; any
    /// failure is an `ApiError` without server text, and the caller ignores it (the thread just
    /// stays unread).
    pub async fn mark_thread_read(&self, tmid: &str) -> Result<(), ApiError> {
        let body = ReadThreadBody { tmid };
        let request = self.http.post(self.endpoint("chat.readThread")).json(&body);
        let _: serde::de::IgnoredAny = read_json(request, ApiError::Rejected).await?;
        Ok(())
    }

    /// The server's custom emojis with `GET emoji-custom.list`: one request for the whole list. Any
    /// error means the list is not known, and the caller carries on without custom emojis.
    pub async fn custom_emojis(&self) -> Result<Vec<crate::data::CustomEmoji>, ApiError> {
        let resp: wire::CustomEmojiResp = self.get_json("emoji-custom.list", &[]).await?;
        Ok(map::custom_emojis(resp))
    }

    /// The image of the custom emoji `name`, with `GET <server>/emoji-custom/<name>.<ext>` and the
    /// same credentials as every other request (this is the path the web client uses; whether it
    /// needs them is checked in `design.md`). `ext` must be one of the formats the client decodes
    /// and `name` must be one path segment, so a hostile list entry cannot steer the request
    /// elsewhere; either way the answer is `BadResponse` without a request. `etag` is only a cache
    /// key for the server. A redirect is `Redirected` and never followed. The body is read under
    /// `MAX_EMOJI_BYTES`. Nothing but the emoji's own name and extension is sent, never message
    /// text, a room or a user.
    pub async fn emoji_image(
        &self,
        name: &str,
        ext: &str,
        etag: &str,
    ) -> Result<Vec<u8>, ApiError> {
        let safe_name = !name.is_empty()
            && name != "."
            && name != ".."
            && !name.contains(['/', '\\', '?', '#', '%'])
            && !name.chars().any(char::is_control);
        if !safe_name || !matches!(ext, "png" | "gif" | "webp" | "jpeg" | "jpg" | "ico") {
            return Err(ApiError::BadResponse);
        }
        let mut url = self.base.clone();
        if let Ok(mut segments) = url.path_segments_mut() {
            segments
                .pop_if_empty()
                .extend(["emoji-custom", &format!("{name}.{ext}")]);
        }
        let mut request = self.http.get(url);
        if !etag.is_empty() {
            request = request.query(&[("etag", etag)]);
        }
        read_capped(request, ApiError::BadResponse, MAX_EMOJI_BYTES).await
    }

    /// The image at the server's own `path` (`/file-upload/<id>/<name>`, as the message lists it),
    /// with `GET <server><path>` and the same credentials as every other request. The URL is the
    /// configured server address joined to `path`, never an address the server supplied, so a
    /// message cannot send the credentials to another host; a `path` that is not a plain
    /// `/file-upload/` path (`map::fetchable_path`) is `BadResponse` without a request. A redirect
    /// is `Redirected` and never followed. The body is read under `MAX_ATTACHMENT_BYTES`. Nothing
    /// but the path is sent, never message text, a room or a user.
    pub async fn attachment_image(&self, path: &str) -> Result<Vec<u8>, ApiError> {
        self.attachment_bytes(path, MAX_ATTACHMENT_BYTES).await
    }

    /// `attachment_image` with another cap: the full-size view reads originals, which are larger
    /// than the inline thumbnails.
    pub async fn attachment_bytes(&self, path: &str, limit: usize) -> Result<Vec<u8>, ApiError> {
        if !map::fetchable_path(path) {
            return Err(ApiError::BadResponse);
        }
        let mut url = self.base.clone();
        let prefix = url.path().trim_end_matches('/').to_string();
        url.set_path(&format!("{prefix}{path}"));
        read_capped(self.http.get(url), ApiError::BadResponse, limit).await
    }

    /// Add (`add`) or take back the user's reaction `emoji` to message `id` with `POST chat.react`.
    /// `emoji` is the shortcode without colons; the request carries it as `:name:`. The answer body
    /// is not read beyond being valid JSON. A name the server does not know (measured: HTTP 400) is
    /// `ApiError::Rejected`, like a refused edit; no server text reaches the error.
    pub async fn react(&self, id: &str, emoji: &str, add: bool) -> Result<(), ApiError> {
        let emoji = format!(":{emoji}:");
        let body = ReactBody {
            message_id: id,
            emoji: &emoji,
            should_react: add,
        };
        let request = self.http.post(self.endpoint("chat.react")).json(&body);
        let _: serde::de::IgnoredAny = read_json(request, ApiError::Rejected).await?;
        Ok(())
    }

    /// Follow thread `mid` (the id of its parent message) with `POST chat.followMessage`. The
    /// answer body is not read beyond being valid JSON; a refusal is `ApiError::Rejected` and no
    /// server text reaches the error.
    pub async fn follow_thread(&self, mid: &str) -> Result<(), ApiError> {
        self.follow_call("chat.followMessage", mid).await
    }

    /// Stop following thread `mid` with `POST chat.unfollowMessage`, answered like `follow_thread`.
    pub async fn unfollow_thread(&self, mid: &str) -> Result<(), ApiError> {
        self.follow_call("chat.unfollowMessage", mid).await
    }

    async fn follow_call(&self, endpoint: &str, mid: &str) -> Result<(), ApiError> {
        let body = FollowBody { mid };
        let request = self.http.post(self.endpoint(endpoint)).json(&body);
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
    use wiremock::matchers::{header, method, path, query_param};
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
            .send_message(&room, "rt-test-1", SEND_TEXT, None)
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
    async fn a_thread_reply_carries_tmid_and_tshow_only_when_set() {
        let room = RoomId("r-general".to_string());
        for (also, expected) in [
            (
                false,
                serde_json::json!({"message": {
                    "_id": "rt-test-1", "rid": "r-general", "msg": "hi", "tmid": "parent-1"
                }}),
            ),
            (
                true,
                serde_json::json!({"message": {
                    "_id": "rt-test-1", "rid": "r-general", "msg": "hi",
                    "tmid": "parent-1", "tshow": true
                }}),
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/api/v1/chat.sendMessage"))
                .and(wiremock::matchers::body_json(expected))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_raw(send_answer("rt-test-1", "hi"), "application/json"),
                )
                .expect(1)
                .mount(&server)
                .await;
            let thread = ThreadTarget {
                parent_id: "parent-1",
                also_in_channel: also,
            };
            client_for(&server)
                .send_message(&room, "rt-test-1", "hi", Some(thread))
                .await
                .expect("a reply is sent");
        }
    }

    #[tokio::test]
    async fn a_rejected_thread_reply_is_a_rejection() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_string("{\"success\":false}"))
            .mount(&server)
            .await;
        let thread = ThreadTarget {
            parent_id: "gone",
            also_in_channel: false,
        };
        let error = client_for(&server)
            .send_message(&RoomId("r-general".to_string()), "x", "hi", Some(thread))
            .await
            .expect_err("rejected");
        assert_eq!(error, ApiError::Rejected);
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
            .send_message(&room, "rt-test-1", SEND_TEXT, None)
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
            .send_message(&room, "rt-test-1", SEND_TEXT, None)
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
            .send_message(&RoomId("r-general".to_string()), "rt-test-1", "hi", None)
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
            .send_message(&RoomId("r-general".to_string()), "rt-test-1", "hi", None)
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
            .send_message(&RoomId("r-general".to_string()), "rt-test-1", "hi", None)
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

    #[tokio::test]
    async fn mark_read_contract() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/subscriptions.read"))
            .and(header("X-User-Id", USER))
            .and(header("X-Auth-Token", TOKEN))
            .and(wiremock::matchers::body_json(
                serde_json::json!({"rid": "r-general"}),
            ))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw("{\"success\":true}", "application/json"),
            )
            .expect(1)
            .mount(&server)
            .await;
        client_for(&server)
            .mark_read(&room_general())
            .await
            .expect("mark read works");
    }

    #[tokio::test]
    async fn mark_read_errors_carry_no_server_text() {
        for status in [400u16, 401, 500] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(
                    ResponseTemplate::new(status)
                        .set_body_string("{\"success\":false,\"error\":\"SENTINEL-SERVER-TEXT\"}"),
                )
                .mount(&server)
                .await;
            let err = client_for(&server)
                .mark_read(&room_general())
                .await
                .expect_err("an error status fails");
            assert!(!format!("{err:?} {err}").contains("SENTINEL"), "{status}");
        }
    }

    #[tokio::test]
    async fn mark_thread_read_contract() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/chat.readThread"))
            .and(header("X-User-Id", USER))
            .and(header("X-Auth-Token", TOKEN))
            .and(wiremock::matchers::body_json(
                serde_json::json!({"tmid": "parent-1"}),
            ))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw("{\"success\":true}", "application/json"),
            )
            .expect(1)
            .mount(&server)
            .await;
        client_for(&server)
            .mark_thread_read("parent-1")
            .await
            .expect("mark thread read works");
    }

    #[tokio::test]
    async fn mark_thread_read_errors_carry_no_server_text() {
        for (status, want) in [
            (400u16, ApiError::Rejected),
            (401, ApiError::Unauthorized),
            (403, ApiError::Forbidden),
            (500, ApiError::Server(500)),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(
                    ResponseTemplate::new(status)
                        .set_body_string("{\"success\":false,\"error\":\"SENTINEL-SERVER-TEXT\"}"),
                )
                .mount(&server)
                .await;
            let err = client_for(&server)
                .mark_thread_read("parent-1")
                .await
                .expect_err("an error status fails");
            assert_eq!(err, want, "{status}");
            assert!(!format!("{err:?} {err}").contains("SENTINEL"), "{status}");
        }
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
    async fn custom_emojis_contract() {
        let server = MockServer::start().await;
        mount_ok(
            &server,
            "emoji-custom.list",
            include_str!("fixtures/docker/emoji_custom_list.json"),
        )
        .await;
        let list = client_for(&server)
            .custom_emojis()
            .await
            .expect("the list loads");
        let names: Vec<&str> = list.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["rtshipit", "rtplain"]);
        assert_eq!(list[0].aliases, ["rtship", "rt_ship"]);
    }

    #[tokio::test]
    async fn custom_emojis_errors_leave_no_list() {
        for (status, want) in [
            (401, ApiError::Unauthorized),
            (403, ApiError::Forbidden),
            (503, ApiError::Server(503)),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v1/emoji-custom.list"))
                .respond_with(ResponseTemplate::new(status).set_body_raw("{}", "application/json"))
                .mount(&server)
                .await;
            let error = client_for(&server)
                .custom_emojis()
                .await
                .expect_err("refused");
            assert_eq!(error, want, "status {status}");
        }
    }

    /// PNG magic and padding: the client only reads bytes here, decoding is elsewhere.
    const FAKE_IMAGE: &[u8] = b"\x89PNG\r\n\x1a\nrest-of-the-image";

    #[tokio::test]
    async fn emoji_image_is_an_authenticated_get_of_the_emoji_path() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emoji-custom/rtshipit.png"))
            .and(header("X-User-Id", USER))
            .and(header("X-Auth-Token", TOKEN))
            .and(wiremock::matchers::query_param("etag", "46961c"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(FAKE_IMAGE))
            .expect(1)
            .mount(&server)
            .await;
        let bytes = client_for(&server)
            .emoji_image("rtshipit", "png", "46961c")
            .await
            .expect("the image loads");
        assert_eq!(bytes, FAKE_IMAGE);
        // Nothing but the path and the etag: no message text, room or user in the request.
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url.query(), Some("etag=46961c"));
        assert_eq!(requests[0].body.len(), 0);
    }

    #[tokio::test]
    async fn emoji_image_over_the_limit_is_refused_while_reading() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emoji-custom/big.png"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0u8; MAX_EMOJI_BYTES + 1]))
            .mount(&server)
            .await;
        assert_eq!(
            client_for(&server).emoji_image("big", "png", "").await,
            Err(ApiError::TooLarge)
        );
        // Exactly the limit is fine.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0u8; MAX_EMOJI_BYTES]))
            .mount(&server)
            .await;
        let ok = client_for(&server).emoji_image("big", "png", "").await;
        assert_eq!(ok.map(|b| b.len()), Ok(MAX_EMOJI_BYTES));
    }

    #[tokio::test]
    async fn emoji_image_statuses_keep_their_kinds_and_a_redirect_is_not_followed() {
        for (status, want) in [
            (404, ApiError::NotFound),
            (401, ApiError::Unauthorized),
            (503, ApiError::Server(503)),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(status))
                .mount(&server)
                .await;
            assert_eq!(
                client_for(&server).emoji_image("x", "png", "").await,
                Err(want),
                "status {status}"
            );
        }
        let server = MockServer::start().await;
        let elsewhere = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("Location", format!("{}/img.png", elsewhere.uri())),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(FAKE_IMAGE))
            .mount(&elsewhere)
            .await;
        assert_eq!(
            client_for(&server).emoji_image("x", "png", "").await,
            Err(ApiError::Redirected)
        );
        assert!(
            elsewhere.received_requests().await.unwrap().is_empty(),
            "the credentials never reach the redirect target"
        );
    }

    #[tokio::test]
    async fn emoji_image_refuses_a_name_or_extension_that_could_steer_the_request() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(FAKE_IMAGE))
            .mount(&server)
            .await;
        let client = client_for(&server);
        for (name, ext) in [
            ("../x", "png"),
            ("..", "png"),
            ("a/b", "png"),
            ("a?b", "png"),
            ("", "png"),
            ("ok", "svg"),
            ("ok", "png/../x"),
            ("ok", ""),
        ] {
            assert_eq!(
                client.emoji_image(name, ext, "").await,
                Err(ApiError::BadResponse),
                "{name:?}.{ext:?}"
            );
        }
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn attachment_image_is_an_authenticated_get_of_the_file_upload_path() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/file-upload/f1/photo.png"))
            .and(header("X-User-Id", USER))
            .and(header("X-Auth-Token", TOKEN))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(FAKE_IMAGE))
            .expect(1)
            .mount(&server)
            .await;
        let bytes = client_for(&server)
            .attachment_image("/file-upload/f1/photo.png")
            .await
            .expect("the image loads");
        assert_eq!(bytes, FAKE_IMAGE);
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].url.query(),
            None,
            "no query: nothing but the path"
        );
        assert_eq!(requests[0].body.len(), 0);
    }

    #[tokio::test]
    async fn attachment_image_keeps_an_encoded_name_as_the_server_wrote_it() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/file-upload/f1/my%20photo.png"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(FAKE_IMAGE))
            .expect(1)
            .mount(&server)
            .await;
        let got = client_for(&server)
            .attachment_image("/file-upload/f1/my%20photo.png")
            .await;
        assert_eq!(got.map(|b| b.len()), Ok(FAKE_IMAGE.len()));
    }

    #[tokio::test]
    async fn attachment_image_over_the_cap_is_refused_while_reading() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200).set_body_bytes(vec![0u8; MAX_ATTACHMENT_BYTES + 1]),
            )
            .mount(&server)
            .await;
        assert_eq!(
            client_for(&server)
                .attachment_image("/file-upload/f1/big.png")
                .await,
            Err(ApiError::TooLarge)
        );
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200).set_body_bytes(vec![0u8; MAX_ATTACHMENT_BYTES]),
            )
            .mount(&server)
            .await;
        let ok = client_for(&server)
            .attachment_image("/file-upload/f1/big.png")
            .await;
        assert_eq!(ok.map(|b| b.len()), Ok(MAX_ATTACHMENT_BYTES));
    }

    #[tokio::test]
    async fn attachment_image_statuses_keep_their_kinds_and_a_redirect_is_not_followed() {
        for (status, want) in [
            (404, ApiError::NotFound),
            (401, ApiError::Unauthorized),
            (403, ApiError::Forbidden),
            (503, ApiError::Server(503)),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(status))
                .mount(&server)
                .await;
            assert_eq!(
                client_for(&server)
                    .attachment_image("/file-upload/a/b.png")
                    .await,
                Err(want),
                "status {status}"
            );
        }
        let server = MockServer::start().await;
        let elsewhere = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("Location", format!("{}/img.png", elsewhere.uri())),
            )
            .mount(&server)
            .await;
        assert_eq!(
            client_for(&server)
                .attachment_image("/file-upload/a/b.png")
                .await,
            Err(ApiError::Redirected)
        );
        assert!(
            elsewhere.received_requests().await.unwrap().is_empty(),
            "the credentials never reach the redirect target"
        );
    }

    #[tokio::test]
    async fn attachment_image_refuses_an_address_that_is_not_a_file_upload_path() {
        let server = MockServer::start().await;
        let elsewhere = MockServer::start().await;
        for any in [&server, &elsewhere] {
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(FAKE_IMAGE))
                .mount(any)
                .await;
        }
        let client = client_for(&server);
        let absolute = format!("{}/file-upload/a/b.png", elsewhere.uri());
        for bad in [
            absolute.as_str(),
            "//evil.example/file-upload/a/b.png",
            "/api/v1/me",
            "/file-upload/",
            "/file-upload/../api/v1/me",
            "/file-upload/a/%2e%2e/%2e%2e/api/v1/me",
            "/file-upload/a/b.png?x=1",
            "file-upload/a/b.png",
            "",
        ] {
            assert_eq!(
                client.attachment_image(bad).await,
                Err(ApiError::BadResponse),
                "{bad:?}"
            );
        }
        assert!(server.received_requests().await.unwrap().is_empty());
        assert!(elsewhere.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn react_contract() {
        // The exact body, for both directions, with the colons added around the name.
        for (add, name) in [(true, "tada"), (false, "+1")] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/api/v1/chat.react"))
                .and(header("X-User-Id", USER))
                .and(header("X-Auth-Token", TOKEN))
                .and(wiremock::matchers::body_json(serde_json::json!({
                    "messageId": "m1",
                    "emoji": format!(":{name}:"),
                    "shouldReact": add,
                })))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_raw("{\"success\":true}", "application/json"),
                )
                .expect(1)
                .mount(&server)
                .await;
            client_for(&server)
                .react("m1", name, add)
                .await
                .expect("react works");
        }
    }

    #[tokio::test]
    async fn follow_and_unfollow_contract() {
        for (follow, endpoint) in [
            (true, "chat.followMessage"),
            (false, "chat.unfollowMessage"),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path(format!("/api/v1/{endpoint}")))
                .and(header("X-User-Id", USER))
                .and(header("X-Auth-Token", TOKEN))
                .and(wiremock::matchers::body_json(
                    serde_json::json!({"mid": "parent-1"}),
                ))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_raw("{\"success\":true}", "application/json"),
                )
                .expect(1)
                .mount(&server)
                .await;
            let client = client_for(&server);
            if follow {
                client.follow_thread("parent-1").await.expect("follows");
            } else {
                client.unfollow_thread("parent-1").await.expect("unfollows");
            }
        }
    }

    #[tokio::test]
    async fn follow_error_kinds_carry_no_server_text() {
        for (status, want) in [
            (400, ApiError::Rejected),
            (401, ApiError::Unauthorized),
            (403, ApiError::Forbidden),
            (503, ApiError::Server(503)),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(status).set_body_raw(
                    "{\"success\":false,\"error\":\"SENTINEL-TEXT\"}",
                    "application/json",
                ))
                .mount(&server)
                .await;
            let error = client_for(&server)
                .follow_thread("parent-1")
                .await
                .expect_err("refused");
            assert_eq!(error, want, "status {status}");
            let shown = format!("{error:?} {error}");
            assert!(!shown.contains("SENTINEL"), "{shown}");
        }
        // A slow server is a timeout, whose outcome is unknown.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw("{\"success\":true}", "application/json")
                    .set_delay(Duration::from_millis(1500)),
            )
            .mount(&server)
            .await;
        let error = quick_client(&server.uri())
            .unfollow_thread("parent-1")
            .await
            .expect_err("times out");
        assert!(error.outcome_unknown(), "{error:?}");
    }

    #[tokio::test]
    async fn react_error_kinds() {
        // Measured on 8.9.1: a name the server does not know is a 400 and is Rejected; the other
        // statuses keep their kinds, and no error holds the emoji name or any server text.
        for (status, want) in [
            (400, ApiError::Rejected),
            (401, ApiError::Unauthorized),
            (403, ApiError::Forbidden),
            (404, ApiError::NotFound),
            (503, ApiError::Server(503)),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/api/v1/chat.react"))
                .respond_with(ResponseTemplate::new(status).set_body_raw(
                    "{\"success\":false,\"error\":\"SENTINEL-EMOJI-NAME not valid\"}",
                    "application/json",
                ))
                .mount(&server)
                .await;
            let error = client_for(&server)
                .react("m1", "SENTINEL-EMOJI-NAME", true)
                .await
                .expect_err("refused");
            assert_eq!(error, want, "status {status}");
            let shown = format!("{error:?} {error}");
            assert!(!shown.contains("SENTINEL"), "{shown}");
        }
        // An answer that is not JSON leaves the outcome unknown.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/chat.react"))
            .respond_with(ResponseTemplate::new(200).set_body_raw("not json", "text/plain"))
            .mount(&server)
            .await;
        let error = client_for(&server)
            .react("m1", "tada", true)
            .await
            .expect_err("unreadable");
        assert!(error.outcome_unknown(), "{error:?}");
    }

    #[tokio::test]
    async fn a_dropped_connection_while_reacting_leaves_the_outcome_unknown() {
        let (url, handle) = dropping_server();
        let error = Client::new(&connection(&url, USER, TOKEN))
            .expect("client")
            .react("m1", "tada", true)
            .await
            .expect_err("the connection is dropped");
        handle.join().expect("server thread");
        assert_eq!(error, ApiError::ConnectionLost);
        assert!(error.outcome_unknown());
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
            .send_message(&RoomId("r-general".to_string()), "rt-test-1", "hi", None)
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
        let ids: Vec<&str> = found.messages.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["m4", "m3", "m2", "m1"], "server order, newest first");
        assert_eq!(found.raw_ts.len(), 4);
        assert!(
            found.raw_ts.iter().all(Option::is_some),
            "raw timestamps kept"
        );

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

    /// The timestamp of message `n` of the simulated room: one second apart.
    fn ts_of(n: u32) -> String {
        format!("2026-01-01T00:{:02}:{:02}.000Z", n / 60, n % 60)
    }

    fn num_of(ts: &str) -> u32 {
        let minutes: u32 = ts[14..16].parse().expect("minutes");
        let seconds: u32 = ts[17..19].parse().expect("seconds");
        minutes * 60 + seconds
    }

    /// A `*.history` answer holding messages `ns`, in the order given (newest first on the wire).
    fn history_of(ns: impl IntoIterator<Item = u32>) -> String {
        let msgs: Vec<String> = ns
            .into_iter()
            .map(|n| {
                format!(
                    r#"{{"_id":"m{n}","rid":"r","msg":"t{n}","ts":"{}","u":{{"_id":"u","username":"bob"}}}}"#,
                    ts_of(n)
                )
            })
            .collect();
        format!(r#"{{"messages":[{}],"success":true}}"#, msgs.join(","))
    }

    /// A simulated room `m1..=m<total>` that answers `*.history` the way the server was measured
    /// to: `latest` (with `inclusive`) gives the newest `count` messages up to it, `oldest` gives
    /// the newest `count` messages after it after skipping `offset` of the newest. `fail` makes
    /// every request with that parameter a 500, `stuck` makes every one-message request say "more".
    struct Room {
        total: u32,
        fail: Option<&'static str>,
        stuck: bool,
    }

    impl wiremock::Respond for Room {
        fn respond(&self, request: &wiremock::Request) -> ResponseTemplate {
            let q = query_of(request);
            let get = |key: &str| q.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
            if self.fail.is_some_and(|key| get(key).is_some()) {
                return ResponseTemplate::new(500);
            }
            let count: u32 = get("count").and_then(|c| c.parse().ok()).unwrap_or(20);
            let body = if let Some(latest) = get("latest") {
                let t = num_of(&latest).min(self.total);
                history_of((1..=t).rev().take(count as usize))
            } else {
                let t = num_of(&get("oldest").expect("oldest"));
                let offset: u32 = get("offset").and_then(|o| o.parse().ok()).unwrap_or(0);
                if self.stuck && count == 1 {
                    history_of([t + 1])
                } else {
                    history_of(
                        ((t + 1)..=self.total)
                            .rev()
                            .skip(offset as usize)
                            .take(count as usize),
                    )
                }
            };
            ResponseTemplate::new(200).set_body_raw(body, "application/json")
        }
    }

    async fn room_server(total: u32, fail: Option<&'static str>, stuck: bool) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/channels.history"))
            .respond_with(Room { total, fail, stuck })
            .mount(&server)
            .await;
        server
    }

    async fn newer(server: &MockServer, t: u32, n: u16) -> Result<NewerPage, ApiError> {
        client_for(server)
            .load_newer(
                &RoomId("r".to_string()),
                RoomKind::Channel,
                false,
                &ts_of(t),
                n,
            )
            .await
    }

    async fn context(server: &MockServer, t: u32) -> Result<ContextWindow, ApiError> {
        client_for(server)
            .load_context(
                &RoomId("r".to_string()),
                RoomKind::Channel,
                false,
                &format!("m{t}"),
                &ts_of(t),
                25,
                25,
            )
            .await
    }

    fn ids_of(messages: &[Message]) -> Vec<String> {
        messages.iter().map(|m| m.id.clone()).collect()
    }

    fn range_ids(r: std::ops::RangeInclusive<u32>) -> Vec<String> {
        r.map(|n| format!("m{n}")).collect()
    }

    #[tokio::test]
    async fn newer_with_nothing_after() {
        let server = room_server(30, None, false).await;
        let page = newer(&server, 30, 50).await.expect("page");
        assert!(page.messages.is_empty() && page.reached_newest);
        assert_eq!(page.newest_raw, None);
    }

    #[tokio::test]
    async fn newer_short_answer_is_everything_after() {
        let server = room_server(90, None, false).await;
        let page = newer(&server, 30, 25).await.expect("page");
        assert_eq!(ids_of(&page.messages), range_ids(31..=55));
        assert!(!page.reached_newest, "35 more follow");
        assert_eq!(page.newest_raw, Some(ts_of(55)));
        let page = newer(&server, 80, 25).await.expect("page");
        assert_eq!(ids_of(&page.messages), range_ids(81..=90));
        assert!(page.reached_newest);
        assert_eq!(server.received_requests().await.expect("recorded").len(), 2);
    }

    #[tokio::test]
    async fn newer_finds_the_nearest_page_however_far_the_end_is() {
        for after in [100u32, 101, 150, 199, 200, 1000] {
            let server = room_server(30 + after, None, false).await;
            let page = newer(&server, 30, 50).await.expect("page");
            assert_eq!(ids_of(&page.messages), range_ids(31..=80), "{after} newer");
            assert_eq!(page.newest_raw, Some(ts_of(80)));
            assert!(!page.reached_newest, "{after} newer");
            let requests = server.received_requests().await.expect("recorded").len();
            assert!(requests <= 26, "{after} newer took {requests} requests");
        }
    }

    #[tokio::test]
    async fn newer_exactly_one_page_from_the_end_reaches_it() {
        let server = room_server(30 + 150, None, false).await;
        let page = newer(&server, 130, 50).await.expect("page");
        assert_eq!(ids_of(&page.messages), range_ids(131..=180));
        assert!(page.reached_newest);
        // Exactly 100 after the point is also found by probing, not mistaken for "many".
        let server = room_server(130, None, false).await;
        let page = newer(&server, 30, 100).await.expect("page");
        assert_eq!(ids_of(&page.messages), range_ids(31..=130));
        assert!(page.reached_newest);
    }

    #[tokio::test]
    async fn newer_errors_and_runaway_probing_fail_the_page() {
        let server = room_server(500, Some("oldest"), false).await;
        assert_eq!(
            newer(&server, 30, 50).await.expect_err("500"),
            ApiError::Server(500)
        );
        let server = room_server(500, None, true).await;
        assert_eq!(
            newer(&server, 30, 50).await.expect_err("stuck"),
            ApiError::BadResponse
        );
    }

    #[tokio::test]
    async fn context_has_both_sides() {
        let server = room_server(40, None, false).await;
        let window = context(&server, 30).await.expect("window");
        assert_eq!(ids_of(&window.messages), range_ids(5..=40));
        assert_eq!(window.selected, 25);
        assert!(window.reaches_newest);
        assert_eq!(window.oldest_raw, Some(ts_of(5)));
        assert_eq!(window.newest_raw, Some(ts_of(40)));
    }

    #[tokio::test]
    async fn context_takes_the_nearest_newer_messages_however_many_exist() {
        let server = room_server(1500, None, false).await;
        let window = context(&server, 30).await.expect("window");
        assert_eq!(ids_of(&window.messages), range_ids(5..=55));
        assert!(!window.reaches_newest);
        assert_eq!(window.newest_raw, Some(ts_of(55)));
    }

    #[tokio::test]
    async fn context_at_the_newest_message_reaches_it() {
        let server = room_server(30, None, false).await;
        let window = context(&server, 30).await.expect("window");
        assert_eq!(*ids_of(&window.messages).last().expect("last"), "m30");
        assert!(window.reaches_newest);
        assert_eq!(window.newest_raw, Some(ts_of(30)), "the result's own time");
    }

    #[tokio::test]
    async fn context_result_missing_is_not_found() {
        // The room's older side ends before the result: it was deleted.
        let server = room_server(20, None, false).await;
        assert_eq!(
            context(&server, 30).await.expect_err("gone"),
            ApiError::NotFound
        );
    }

    #[tokio::test]
    async fn context_error_from_either_side_fails_the_load() {
        for failing in ["latest", "oldest"] {
            let server = room_server(500, Some(failing), false).await;
            assert_eq!(
                context(&server, 30).await.expect_err("fails"),
                ApiError::Server(500),
                "{failing}"
            );
        }
    }

    #[test]
    fn context_debug_prints_no_text() {
        let window = ContextWindow {
            messages: vec![Message {
                edited: false,
                plain_text: true,
                tmid: None,
                tshow: false,
                tcount: 0,
                tlm: None,
                followers: None,
                refs: None,
                attachments: Vec::new(),
                reactions: Vec::new(),
                id: "m1".to_string(),
                room_id: RoomId("r".to_string()),
                author: "secret-author".to_string(),
                ts: 0,
                body: "secret-body".to_string(),
                kind: MessageKind::Normal,
                send: SendState::Sent,
            }],
            selected: 0,
            newest_raw: Some("secret-raw".to_string()),
            reaches_newest: false,
            oldest_raw: Some("secret-raw".to_string()),
        };
        let text = format!("{window:?} {:?}", SearchHits::from(window.messages.clone()));
        assert!(!text.contains("secret"), "{text}");
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
        assert!(found.messages.is_empty());

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
                tmid: None,
                tshow: false,
                tcount: 0,
                tlm: None,
                followers: None,
                refs: None,
                attachments: Vec::new(),
                reactions: Vec::new(),
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

    #[tokio::test]
    async fn load_thread_pages_by_offset_on_both_servers() {
        for (full, page2, label) in [
            (
                include_str!("fixtures/docker/thread_messages.json"),
                include_str!("fixtures/docker/thread_page2.json"),
                "8.9.1",
            ),
            (
                include_str!("fixtures/docker/thread_messages_9.json"),
                include_str!("fixtures/docker/thread_page2_9.json"),
                "9.0.0",
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v1/chat.getThreadMessages"))
                .and(header("X-User-Id", USER))
                .and(header("X-Auth-Token", TOKEN))
                .and(query_param("tmid", "parent-1"))
                .and(query_param("count", "50"))
                .and(query_param("offset", "0"))
                .respond_with(ResponseTemplate::new(200).set_body_raw(full, "application/json"))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/api/v1/chat.getThreadMessages"))
                .and(query_param("count", "2"))
                .and(query_param("offset", "2"))
                .respond_with(ResponseTemplate::new(200).set_body_raw(page2, "application/json"))
                .expect(1)
                .mount(&server)
                .await;
            let client = client_for(&server);
            let room = RoomId("r1".to_string());

            let all = client
                .load_thread(&room, "parent-1", 0, 50)
                .await
                .expect(label);
            assert_eq!((all.messages.len(), all.total), (3, 3), "{label}");
            assert!(
                all.messages
                    .iter()
                    .all(|m| m.tmid.is_some() && m.room_id == room)
            );
            assert!(
                all.messages.windows(2).all(|w| w[0].ts <= w[1].ts),
                "{label}"
            );

            let rest = client
                .load_thread(&room, "parent-1", 2, 2)
                .await
                .expect(label);
            assert_eq!((rest.messages.len(), rest.total), (1, 3), "{label}");
            assert_eq!(rest.messages[0].id, all.messages[2].id, "{label}");
            assert!(rest.messages[0].tshow, "{label}");
        }
    }

    #[tokio::test]
    async fn load_thread_errors_carry_no_message_text() {
        for (status, want) in [
            (400, ApiError::Rejected),
            (401, ApiError::Unauthorized),
            (403, ApiError::Forbidden),
            (404, ApiError::NotFound),
            (503, ApiError::Server(503)),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v1/chat.getThreadMessages"))
                .respond_with(ResponseTemplate::new(status).set_body_raw(
                    "{\"success\":false,\"error\":\"SENTINEL-THREAD-TEXT\"}",
                    "application/json",
                ))
                .mount(&server)
                .await;
            let error = client_for(&server)
                .load_thread(&RoomId("r1".to_string()), "SENTINEL-TMID", 0, 50)
                .await
                .expect_err("refused");
            assert_eq!(error, want, "status {status}");
            let shown = format!("{error:?} {error}");
            assert!(!shown.contains("SENTINEL"), "{shown}");
        }
        let page = ThreadPage {
            messages: vec![],
            total: 4,
        };
        assert!(!format!("{page:?}").contains("body"));
    }

    #[tokio::test]
    async fn list_threads_by_filter_text_and_offset_on_both_servers() {
        for (all, following, unread, text, page2, empty, label) in [
            (
                include_str!("fixtures/docker/thread_list_all.json"),
                include_str!("fixtures/docker/thread_list_following.json"),
                include_str!("fixtures/docker/thread_list_unread.json"),
                include_str!("fixtures/docker/thread_list_text.json"),
                include_str!("fixtures/docker/thread_list_page2.json"),
                include_str!("fixtures/docker/thread_list_empty.json"),
                "8.9.1",
            ),
            (
                include_str!("fixtures/docker/thread_list_all_9.json"),
                include_str!("fixtures/docker/thread_list_following_9.json"),
                include_str!("fixtures/docker/thread_list_unread_9.json"),
                include_str!("fixtures/docker/thread_list_text_9.json"),
                include_str!("fixtures/docker/thread_list_page2_9.json"),
                include_str!("fixtures/docker/thread_list_empty_9.json"),
                "9.0.0",
            ),
        ] {
            let server = MockServer::start().await;
            let mount = |extra: Vec<(&'static str, &'static str)>, body: &'static str| {
                let mut builder = Mock::given(method("GET"))
                    .and(path("/api/v1/chat.getThreadsList"))
                    .and(header("X-User-Id", USER))
                    .and(header("X-Auth-Token", TOKEN))
                    .and(query_param("rid", "r1"));
                for (k, v) in extra {
                    builder = builder.and(query_param(k, v));
                }
                builder
                    .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
            };
            mount(vec![("count", "50"), ("offset", "0")], all)
                .up_to_n_times(1)
                .mount(&server)
                .await;
            mount(vec![("type", "following")], following)
                .mount(&server)
                .await;
            mount(vec![("type", "unread")], unread).mount(&server).await;
            mount(vec![("text", "deploy")], text).mount(&server).await;
            mount(vec![("count", "1"), ("offset", "1")], page2)
                .mount(&server)
                .await;
            let client = client_for(&server);
            let room = RoomId("r1".to_string());

            let page = client
                .list_threads(&room, ThreadFilter::All, "", 0, 50)
                .await
                .expect(label);
            assert_eq!(page.rows.len() as u32, page.total, "{label}");
            assert!(
                page.rows
                    .iter()
                    .all(|r| !r.id.is_empty() && !r.author.is_empty())
            );
            let first = client
                .list_threads(&room, ThreadFilter::Following, "", 0, 50)
                .await
                .expect(label);
            assert!(first.total >= 1, "{label}");
            client
                .list_threads(&room, ThreadFilter::Unread, "", 0, 50)
                .await
                .expect(label);
            let hits = client
                .list_threads(&room, ThreadFilter::All, "deploy", 0, 50)
                .await
                .expect(label);
            assert!(
                hits.rows.iter().all(|r| r.first_line.contains("deploy")),
                "{label}"
            );
            let rest = client
                .list_threads(&room, ThreadFilter::All, "", 1, 1)
                .await
                .expect(label);
            assert_eq!(rest.rows.len(), 1, "{label}");
            assert!(rest.total > 1, "{label}");

            let requests = server.received_requests().await.expect("recorded");
            // `All` sends no `type`, and an empty text sends no `text`.
            let all_query = query_of(&requests[0]);
            assert!(
                all_query.iter().all(|(k, _)| k != "type" && k != "text"),
                "{label}"
            );

            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v1/chat.getThreadsList"))
                .respond_with(ResponseTemplate::new(200).set_body_raw(empty, "application/json"))
                .mount(&server)
                .await;
            let none = client_for(&server)
                .list_threads(&room, ThreadFilter::All, "", 0, 50)
                .await
                .expect(label);
            assert_eq!((none.rows.len(), none.total), (0, 0), "{label}");
        }
    }

    #[tokio::test]
    async fn list_threads_sends_special_text_unchanged() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/chat.getThreadsList"))
            .and(query_param("text", "(a[b].c"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                "{\"threads\":[],\"total\":0,\"success\":true}",
                "application/json",
            ))
            .expect(1)
            .mount(&server)
            .await;
        client_for(&server)
            .list_threads(
                &RoomId("r1".to_string()),
                ThreadFilter::All,
                "(a[b].c",
                0,
                50,
            )
            .await
            .expect("answered");
    }

    #[tokio::test]
    async fn list_threads_errors_carry_no_search_text() {
        for (status, want) in [
            (400, ApiError::Rejected),
            (401, ApiError::Unauthorized),
            (403, ApiError::Forbidden),
            (404, ApiError::NotFound),
            (503, ApiError::Server(503)),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v1/chat.getThreadsList"))
                .respond_with(ResponseTemplate::new(status).set_body_raw(
                    "{\"success\":false,\"error\":\"SENTINEL-LIST-TEXT\"}",
                    "application/json",
                ))
                .mount(&server)
                .await;
            let error = client_for(&server)
                .list_threads(
                    &RoomId("r1".to_string()),
                    ThreadFilter::All,
                    "SENTINEL-QUERY",
                    0,
                    50,
                )
                .await
                .expect_err("refused");
            assert_eq!(error, want, "status {status}");
            let shown = format!("{error:?} {error}");
            assert!(!shown.contains("SENTINEL"), "{shown}");
        }
        let page = ThreadListPage {
            rows: vec![],
            total: 2,
        };
        assert!(!format!("{page:?}").contains("body"));
    }

    #[test]
    fn thread_filter_cycles_both_ways() {
        let mut f = ThreadFilter::All;
        for want in [
            ThreadFilter::Following,
            ThreadFilter::Unread,
            ThreadFilter::All,
        ] {
            f = f.next();
            assert_eq!(f, want);
        }
        assert_eq!(ThreadFilter::All.prev(), ThreadFilter::Unread);
        assert_eq!(ThreadFilter::Unread.prev(), ThreadFilter::Following);
    }
}
