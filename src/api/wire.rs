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
    /// The server's alert flag: the room stays unread with a zero count.
    #[serde(default)]
    pub alert: bool,
    #[serde(default, rename = "userMentions")]
    pub user_mentions: u32,
    #[serde(default, rename = "groupMentions")]
    pub group_mentions: u32,
    /// Ids of the threads with replies the user has not read, and the subsets with a mention of
    /// the user (`tunreadUser`) or of a group (`tunreadGroup`). Absent when there are none.
    #[serde(default)]
    pub tunread: Vec<String>,
    #[serde(default, rename = "tunreadUser")]
    pub tunread_user: Vec<String>,
    #[serde(default, rename = "tunreadGroup")]
    pub tunread_group: Vec<String>,
    pub prid: Option<String>,
    pub category: Option<String>,
    pub status: Option<String>,
    pub ts: Option<ApiDate>,
    /// When the user last read the room; absent for a subscription that was never read.
    pub ls: Option<ApiDate>,
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
    /// The user's own id, to tell their follows from the others'.
    #[serde(default, rename = "_id")]
    pub id: Option<String>,
}

#[derive(Deserialize)]
pub struct HistoryResp {
    #[serde(default)]
    pub messages: Vec<WireMsg>,
}

/// The success body of `chat.getThreadMessages`: the replies oldest first, and how many the thread
/// has in all (the paging bound).
#[derive(Deserialize)]
pub struct ThreadResp {
    #[serde(default)]
    pub messages: Vec<WireMsg>,
    #[serde(default)]
    pub total: u32,
}

/// The success body of `chat.getThreadsList`: the thread parents newest activity first, and how
/// many threads match in all (the paging bound).
#[derive(Deserialize)]
pub struct ThreadsListResp {
    #[serde(default)]
    pub threads: Vec<WireMsg>,
    #[serde(default)]
    pub total: u32,
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
    /// The room id: present on stream messages (history knows its room from the request).
    pub rid: Option<String>,
    pub ts: ApiDate,
    pub u: WireUser,
    pub t: Option<String>,
    pub file: Option<WireFile>,
    #[serde(default)]
    pub files: Vec<WireFile>,
    #[serde(default)]
    pub attachments: Vec<WireAttach>,
    pub content: Option<serde_json::Value>,
    /// Present when the message was edited; only its presence is read.
    #[serde(rename = "editedAt")]
    pub edited_at: Option<serde_json::Value>,
    /// The reactions in the order the server lists them. Absent, `null` and anything that is not
    /// an object give none: the server drops the key when the last reaction is taken back.
    #[serde(default)]
    pub reactions: WireReactions,
    /// The thread parent's id on a reply.
    #[serde(default)]
    pub tmid: Option<String>,
    /// A reply that was also sent to the channel.
    #[serde(default)]
    pub tshow: bool,
    /// On a thread parent: the number of replies.
    #[serde(default)]
    pub tcount: Option<u32>,
    /// On a thread parent: the time of the last reply, ISO string (REST) or `$date` (stream).
    #[serde(default)]
    pub tlm: Option<ApiDate>,
    /// On a thread parent: the ids of the users who follow the thread. Anything that is not a list
    /// of strings is read as absent.
    #[serde(default)]
    pub replies: Option<serde_json::Value>,
    /// The users the text mentions: a list of objects with a `username`. Read leniently, like
    /// `replies`: entries that do not fit are skipped. Never printed.
    #[serde(default)]
    pub mentions: Option<serde_json::Value>,
    /// The channels the text references: a list of objects with a `name`. Read like `mentions`.
    #[serde(default)]
    pub channels: Option<serde_json::Value>,
}

/// One reaction of a message: the shortcode as the server writes it (`:+1:`, with the colons) and
/// the usernames that gave it. Never printed: usernames are private.
pub struct WireReaction {
    pub name: String,
    pub usernames: Vec<String>,
}

/// The `reactions` object of a message, kept as a `Vec` in the order the server wrote the keys.
///
/// Order depends on two things. Here, the visitor reads the entries one by one, so a typed parse
/// straight from text (history) keeps server order. The realtime stream first decodes a whole DDP
/// frame into a `serde_json::Value`, and a `Value` map is a `BTreeMap` (sorted keys) unless
/// serde_json's `preserve_order` feature is on, which Cargo.toml enables; without it live
/// reactions would come out alphabetical and then jump when a refetch brought server order.
/// `reactions_keep_server_order_through_a_json_value` and the captured-stream test in
/// `app::realtime` guard it.
///
/// The visitor is also what keeps the type lenient like the rest of this file: an entry that is
/// not an object, has no `usernames` array or has no usernames is skipped, and a non-string
/// username is dropped, so one odd entry never fails a whole page.
#[derive(Default)]
pub struct WireReactions(pub Vec<WireReaction>);

impl<'de> Deserialize<'de> for WireReactions {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = WireReactions;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a reactions object")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut reactions = Vec::new();
                while let Some((name, body)) = map.next_entry::<String, serde_json::Value>()? {
                    let usernames: Vec<String> = body
                        .get("usernames")
                        .and_then(|v| v.as_array())
                        .map(|list| {
                            list.iter()
                                .filter_map(|u| u.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default();
                    if !usernames.is_empty() {
                        reactions.push(WireReaction { name, usernames });
                    }
                }
                Ok(WireReactions(reactions))
            }

            // `null` and any other shape: no reactions, and the value is consumed.
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(WireReactions::default())
            }

            fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<Self::Value, E> {
                Ok(WireReactions::default())
            }

            fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<Self::Value, E> {
                Ok(WireReactions::default())
            }

            fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<Self::Value, E> {
                Ok(WireReactions::default())
            }

            fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<Self::Value, E> {
                Ok(WireReactions::default())
            }

            fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<Self::Value, E> {
                Ok(WireReactions::default())
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
                Ok(WireReactions::default())
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

/// The answer of `emoji-custom.list`: `{"emojis": {"update": [..], "remove": [..]}}`. The whole list
/// is fetched, so `remove` is not declared. Entries stay raw `Value`s so one odd entry (no name, a
/// number for a name) is skipped by the mapping instead of failing the list.
#[derive(Deserialize)]
pub struct CustomEmojiResp {
    #[serde(default)]
    pub emojis: WireEmojis,
}

#[derive(Deserialize, Default)]
pub struct WireEmojis {
    #[serde(default)]
    pub update: Vec<serde_json::Value>,
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

/// The part of a subscription document of a `subscriptions-changed` stream event that the live
/// unread state needs. Everything else (names, settings) is not declared, so serde skips it.
#[derive(Deserialize)]
pub struct WireSubDoc {
    pub rid: String,
    /// Absent from updates that do not touch the unread state (measured on 8.9.1 and 9.0.0: a
    /// message is followed by a second update that carries neither key).
    pub alert: Option<bool>,
    pub unread: Option<u32>,
    #[serde(rename = "userMentions")]
    pub user_mentions: Option<u32>,
    #[serde(rename = "groupMentions")]
    pub group_mentions: Option<u32>,
    /// The thread unread sets (see `WireSub`). `None` when the update lacks the field.
    pub tunread: Option<Vec<String>>,
    #[serde(rename = "tunreadUser")]
    pub tunread_user: Option<Vec<String>>,
    #[serde(rename = "tunreadGroup")]
    pub tunread_group: Option<Vec<String>>,
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

    fn reactions_of(extra: &str) -> Vec<(String, Vec<String>)> {
        let json = format!(
            r#"{{"_id":"m","msg":"x","ts":"2026-11-16T10:00:00.000Z","u":{{"username":"a"}}{extra}}}"#
        );
        let msg: WireMsg = serde_json::from_str(&json).unwrap();
        msg.reactions
            .0
            .into_iter()
            .map(|r| (r.name, r.usernames))
            .collect()
    }

    #[test]
    fn reactions_keep_server_order_not_alphabetical() {
        let got = reactions_of(
            r#","reactions":{":z:":{"usernames":["b"]},":a:":{"usernames":["a","c"]},":m:":{"usernames":["d"]}}"#,
        );
        let names: Vec<&str> = got.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, [":z:", ":a:", ":m:"]);
        assert_eq!(got[1].1, ["a", "c"]);
    }

    #[test]
    fn reactions_keep_server_order_through_a_json_value() {
        // The stream decodes a whole frame into a `Value` before the typed parse.
        let value: serde_json::Value = serde_json::from_str(
            r#"{"_id":"m","msg":"x","ts":"2026-11-16T10:00:00.000Z","u":{"username":"a"},
                "reactions":{":z:":{"usernames":["b"]},":a:":{"usernames":["c"]}}}"#,
        )
        .unwrap();
        let msg: WireMsg = serde_json::from_value(value).unwrap();
        let names: Vec<&str> = msg.reactions.0.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, [":z:", ":a:"]);
    }

    #[test]
    fn reactions_absent_null_empty_or_not_an_object_are_none() {
        assert!(reactions_of("").is_empty());
        assert!(reactions_of(r#","reactions":null"#).is_empty());
        assert!(reactions_of(r#","reactions":{}"#).is_empty());
        assert!(reactions_of(r#","reactions":"nope""#).is_empty());
        assert!(reactions_of(r#","reactions":7"#).is_empty());
        assert!(reactions_of(r#","reactions":true"#).is_empty());
        assert!(reactions_of(r#","reactions":[1,{"a":2}]"#).is_empty());
    }

    #[test]
    fn reactions_skip_bad_entries_and_keep_the_good_ones() {
        let got = reactions_of(
            r#","reactions":{":a:":"text",":b:":{"usernames":[]},":c:":{},":d:":{"usernames":"x"},":e:":{"usernames":["u",5,null]},":f:":{"usernames":["v"]}}"#,
        );
        assert_eq!(
            got,
            [
                (":e:".to_string(), vec!["u".to_string()]),
                (":f:".to_string(), vec!["v".to_string()])
            ]
        );
    }

    #[test]
    fn reactions_from_the_captured_server_fixture() {
        let resp: HistoryResp =
            serde_json::from_str(include_str!("fixtures/docker/history_reactions.json")).unwrap();
        let reacted = resp
            .messages
            .iter()
            .find(|m| !m.reactions.0.is_empty())
            .expect("the capture has a message with reactions");
        let names: Vec<&str> = reacted
            .reactions
            .0
            .iter()
            .map(|r| r.name.as_str())
            .collect();
        // The order the capture script reacted in, which is not alphabetical.
        assert_eq!(
            names,
            [":+1:", ":tada:", ":thumbsup_tone2:", ":rtpartyparrot:"]
        );
        assert_eq!(reacted.reactions.0[0].usernames, ["dev", "alice", "bob"]);
        // The sibling message whose only reaction was taken back has none.
        assert!(resp.messages.iter().any(|m| m.reactions.0.is_empty()));
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
