//! Lenient serde types for the REST responses. Every field is optional or defaulted so a server
//! version that omits one still loads.
//!
//! These types derive `Deserialize` only, never `Debug`: a wire struct can hold room names and,
//! for `rooms.get`, a `lastMessage` body, so they are never printed (RESEARCH Pitfall 8). The
//! `lastMessage` field is deliberately not declared, so serde skips it.

use serde::Deserialize;

/// A server date: normally an ISO string, sometimes the EJSON `{ "$date": millis }` form.
#[derive(Deserialize)]
#[serde(untagged)]
pub enum ApiDate {
    Iso(String),
    Ejson {
        #[serde(rename = "$date")]
        ms: i64,
    },
}

impl ApiDate {
    /// Milliseconds since the Unix epoch, `None` for an unparsable ISO string.
    pub fn millis(&self) -> Option<i64> {
        match self {
            ApiDate::Iso(text) => chrono::DateTime::parse_from_rfc3339(text)
                .ok()
                .map(|d| d.timestamp_millis()),
            ApiDate::Ejson { ms } => Some(*ms),
        }
    }

    /// Whole seconds since the Unix epoch (floor), `None` for an unparsable ISO string.
    pub fn secs(&self) -> Option<i64> {
        self.millis().map(|ms| ms.div_euclid(1000))
    }

    /// The date as an ISO 8601 string with milliseconds, for the `latest` query of paging: an ISO
    /// date is returned verbatim (never recomputed from seconds, Pitfall 7), an EJSON date is
    /// converted from its milliseconds. `None` when the milliseconds are out of range.
    pub fn raw_iso(&self) -> Option<String> {
        match self {
            ApiDate::Iso(text) => Some(text.clone()),
            ApiDate::Ejson { ms } => chrono::DateTime::<chrono::Utc>::from_timestamp_millis(*ms)
                .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
pub struct SubscriptionsResp {
    #[serde(default)]
    pub update: Vec<WireSub>,
}

/// One entry of `subscriptions.get`. `open` defaults to true: some server versions omit it, and
/// only an explicit false hides a room (D-09).
#[derive(Deserialize)]
pub struct WireSub {
    pub rid: String,
    #[serde(default)]
    pub t: String,
    pub name: Option<String>,
    pub fname: Option<String>,
    #[serde(default = "default_true")]
    pub open: bool,
    #[serde(default)]
    pub f: bool,
    #[serde(default)]
    pub unread: u32,
    pub prid: Option<String>,
    pub category: Option<String>,
    pub status: Option<String>,
    pub ts: Option<ApiDate>,
}

#[derive(Deserialize)]
pub struct RoomsResp {
    #[serde(default)]
    pub update: Vec<WireRoom>,
}

/// One entry of `rooms.get`: the fields `subscriptions.get` lacks (`encrypted`, `lm`, `teamMain`).
#[derive(Deserialize)]
pub struct WireRoom {
    #[serde(rename = "_id")]
    pub id: String,
    pub encrypted: Option<bool>,
    #[serde(rename = "teamMain")]
    pub team_main: Option<bool>,
    pub prid: Option<String>,
    pub lm: Option<ApiDate>,
}

#[derive(Deserialize)]
pub struct PreferencesResp {
    pub preferences: WirePrefs,
}

/// The part of `users.getPreferences` the sidebar folders need.
#[derive(Deserialize)]
pub struct WirePrefs {
    #[serde(default, rename = "sidebarCategories")]
    pub sidebar_categories: Vec<WireCategory>,
}

/// A user-defined sidebar category. The built-in entries carry `default: true`.
#[derive(Deserialize)]
pub struct WireCategory {
    #[serde(rename = "_id")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub default: bool,
}

/// The part of `GET /api/v1/me` the client reads: the user's own username (D-17). The rest of the
/// large document (email, roles, settings, services) is not declared, so serde skips it.
#[derive(Deserialize)]
pub struct MeResp {
    #[serde(default)]
    pub username: Option<String>,
}

#[derive(Deserialize)]
pub struct HistoryResp {
    #[serde(default)]
    pub messages: Vec<WireMsg>,
}

/// The success body of `chat.sendMessage` and of `chat.getMessage`: the one message, as stored.
#[derive(Deserialize)]
pub struct MessageResp {
    pub message: WireMsg,
}

/// One message of a `*.history` response. Message text is untrusted and never printed, so there
/// is no `Debug`. `content` is the end-to-end payload of an encrypted message: it is only checked
/// for presence and never read, copied or printed.
#[derive(Deserialize)]
pub struct WireMsg {
    #[serde(rename = "_id")]
    pub id: String,
    #[serde(default)]
    pub msg: String,
    pub ts: ApiDate,
    pub u: WireUser,
    pub t: Option<String>,
    pub file: Option<WireFile>,
    #[serde(default)]
    pub files: Vec<WireFile>,
    #[serde(default)]
    pub attachments: Vec<WireAttach>,
    pub content: Option<serde_json::Value>,
}

/// An uploaded file of a message; only its name is used.
#[derive(Deserialize)]
pub struct WireFile {
    pub name: Option<String>,
}

/// An attachment of a message; only its title is used.
#[derive(Deserialize)]
pub struct WireAttach {
    pub title: Option<String>,
}

#[derive(Deserialize)]
pub struct WireUser {
    pub username: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_date_forms() {
        let iso: ApiDate = serde_json::from_str("\"2026-11-16T10:00:00.123Z\"").unwrap();
        assert_eq!(iso.millis(), Some(1_794_823_200_123));
        assert_eq!(iso.secs(), Some(1_794_823_200));
        let ejson: ApiDate = serde_json::from_str("{\"$date\": 1794823200123}").unwrap();
        assert_eq!(ejson.millis(), Some(1_794_823_200_123));
        let bad: ApiDate = serde_json::from_str("\"not a date\"").unwrap();
        assert_eq!(bad.millis(), None);
        assert_eq!(bad.secs(), None);
    }

    #[test]
    fn raw_iso_forms() {
        let iso: ApiDate = serde_json::from_str("\"2026-11-16T10:00:00.300Z\"").unwrap();
        assert_eq!(iso.raw_iso().as_deref(), Some("2026-11-16T10:00:00.300Z"));
        let ejson: ApiDate = serde_json::from_str("{\"$date\": 1794823200123}").unwrap();
        assert_eq!(ejson.raw_iso().as_deref(), Some("2026-11-16T10:00:00.123Z"));
        let bad: ApiDate = serde_json::from_str("\"not a date\"").unwrap();
        // An unparsable ISO string is still returned verbatim; the caller decides what to do.
        assert_eq!(bad.raw_iso().as_deref(), Some("not a date"));
    }

    #[test]
    fn open_defaults_to_true() {
        let sub: WireSub = serde_json::from_str("{\"rid\":\"r\",\"t\":\"c\"}").unwrap();
        assert!(sub.open);
        let closed: WireSub =
            serde_json::from_str("{\"rid\":\"r\",\"t\":\"c\",\"open\":false}").unwrap();
        assert!(!closed.open);
    }
}
