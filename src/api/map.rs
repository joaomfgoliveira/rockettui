//! Pure mapping from the wire types to the domain types of `data`. No IO, no clock.
//!
//! `subscriptions.get` says which rooms the user joined and how they show; `rooms.get` adds what
//! the subscription lacks (`encrypted`, `lm`, `teamMain`). The two are joined on the room id.

use crate::api::wire::{WireMsg, WirePrefs, WireRoom, WireSub};
use crate::api::{HistoryPage, SearchHits};
use crate::data::{Folder, FolderId, Message, MessageKind, Room, RoomId, RoomKind, SendState};
use std::collections::{HashMap, HashSet};

/// The kind of a joined room, `None` for kinds the sidebar does not show (livechat and anything
/// unknown). Direct messages first, then discussions, teams, channels and private groups.
fn kind_of(sub: &WireSub, room: &WireRoom) -> Option<RoomKind> {
    if sub.t == "d" {
        return Some(RoomKind::DirectMessage);
    }
    if sub.prid.is_some() || room.prid.is_some() {
        return Some(RoomKind::Discussion);
    }
    if room.team_main == Some(true) {
        return Some(RoomKind::Team);
    }
    match sub.t.as_str() {
        "c" => Some(RoomKind::Channel),
        "p" => Some(RoomKind::PrivateGroup),
        _ => None,
    }
}

/// The display name: a direct message uses the other user's username (`name`, never the full
/// name); everything else prefers `fname`. Falls back to the room id so a row is never blank.
fn display_name(sub: &WireSub, kind: RoomKind) -> String {
    let (first, second) = if kind == RoomKind::DirectMessage {
        (&sub.name, &sub.fname)
    } else {
        (&sub.fname, &sub.name)
    };
    first
        .as_deref()
        .filter(|s| !s.is_empty())
        .or(second.as_deref().filter(|s| !s.is_empty()))
        .unwrap_or(&sub.rid)
        .to_string()
}

/// True when a subscription is listed in the sidebar at all: joined and shown (`open`), not an
/// invitation or ban (`status`), not livechat.
fn is_listed(sub: &WireSub) -> bool {
    sub.open && sub.status.is_none() && sub.t != "l"
}

/// The joined, open rooms, newest activity first, then name, then room id so ties are stable
/// across reloads. A subscription without a `rooms.get` record is skipped: its encrypted flag is
/// unknown, so it must not be listed (it could later request history).
pub fn rooms(subs: &[WireSub], records: &[WireRoom]) -> Vec<Room> {
    let by_id: HashMap<&str, &WireRoom> = records.iter().map(|r| (r.id.as_str(), r)).collect();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut out: Vec<Room> = Vec::new();
    for sub in subs.iter().filter(|s| is_listed(s)) {
        let Some(record) = by_id.get(sub.rid.as_str()) else {
            continue;
        };
        let Some(kind) = kind_of(sub, record) else {
            continue;
        };
        if !seen.insert(sub.rid.as_str()) {
            continue;
        }
        let last_activity = record
            .lm
            .as_ref()
            .and_then(|d| d.secs())
            .or_else(|| sub.ts.as_ref().and_then(|d| d.secs()))
            .unwrap_or(0);
        out.push(Room {
            id: RoomId(sub.rid.clone()),
            name: display_name(sub, kind),
            kind,
            encrypted: record.encrypted.unwrap_or(false),
            private: sub.t == "p",
            unread: sub.unread > 0,
            last_activity,
        });
    }
    out.sort_by(|a, b| {
        b.last_activity
            .cmp(&a.last_activity)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.id.cmp(&b.id))
    });
    out
}

/// The sidebar folders (D-10, flagged assumption A6): a `Favorites` folder first, then the user's
/// custom categories in the order the server lists them.
///
/// `Favorites` holds the favorited rooms that have no category; it is shown only when it has a
/// room. Each custom category (`default` is false) holds the rooms whose subscription names it.
/// Assignment is exclusive per room, so no room is in two folders. A room is listed only if it is
/// in `rooms` (the mapped, non-skipped list), in that list's order, and a folder with no listed
/// rooms is omitted. Turning this into the literal D-10 reading ("categories, otherwise
/// Favorites") means dropping `Favorites` when any custom category has a room.
pub fn folders(subs: &[WireSub], rooms: &[Room], prefs: Option<&WirePrefs>) -> Vec<Folder> {
    let mut sub_of: HashMap<&str, &WireSub> = HashMap::new();
    for sub in subs {
        sub_of.entry(sub.rid.as_str()).or_insert(sub);
    }
    let has_category = |sub: &WireSub| sub.category.as_deref().is_some_and(|c| !c.is_empty());
    let room_ids = |wanted: &dyn Fn(&WireSub) -> bool| -> Vec<RoomId> {
        rooms
            .iter()
            .filter(|room| {
                sub_of
                    .get(room.id.0.as_str())
                    .is_some_and(|sub| wanted(sub))
            })
            .map(|room| room.id.clone())
            .collect()
    };

    let mut out = Vec::new();
    let favorites = room_ids(&|sub| sub.f && !has_category(sub));
    if !favorites.is_empty() {
        out.push(Folder {
            id: FolderId("favorites".to_string()),
            name: "Favorites".to_string(),
            room_ids: favorites,
        });
    }
    let categories = prefs.map_or(&[][..], |p| p.sidebar_categories.as_slice());
    for category in categories.iter().filter(|c| !c.default) {
        let members = room_ids(&|sub| sub.category.as_deref() == Some(category.id.as_str()));
        if members.is_empty() {
            continue;
        }
        out.push(Folder {
            id: FolderId(category.id.clone()),
            name: category.name.clone(),
            room_ids: members,
        });
    }
    out
}

/// The text shown for an end-to-end encrypted message found in a room's history (D-12, D-15).
/// The ciphertext is never copied into a body.
pub(crate) const ENCRYPTED_TEXT: &str = "[encrypted message]";

/// Kind and body of one wire message (D-15).
///
/// - `t` is `e2e`, or a `content` object is present without a system type: an end-to-end
///   leftover, `Normal` with the fixed text, `msg` and `content` ignored;
/// - any other `t` is a system message, drawn as a one-liner by type (`au` and `ru` append the
///   target user the server puts in `msg`);
/// - otherwise the text, followed by ` [attachment: <name>]` for the file, each listed file and
///   each attachment title, every name once.
fn kind_and_body(msg: &WireMsg) -> (MessageKind, String) {
    // `rm`: the server keeps a deleted message as this type when it shows deleted status. It is a
    // tombstone, not a system message; its text is already gone.
    if msg.t.as_deref() == Some("rm") {
        return (MessageKind::Deleted, String::new());
    }
    let system_type = msg.t.as_deref().filter(|t| *t != "e2e");
    if msg.t.as_deref() == Some("e2e") || (system_type.is_none() && msg.content.is_some()) {
        return (MessageKind::Normal, ENCRYPTED_TEXT.to_string());
    }
    if let Some(t) = system_type {
        let target = msg.msg.trim();
        let text = match t {
            "uj" => "* joined the room".to_string(),
            "ul" => "* left the room".to_string(),
            "au" => format!("* added {target}").trim_end().to_string(),
            "ru" => format!("* removed {target}").trim_end().to_string(),
            "r" => "* renamed the room".to_string(),
            "room_changed_topic" => "* changed the topic".to_string(),
            _ => "* system message".to_string(),
        };
        return (MessageKind::System, text);
    }
    let mut names: Vec<&str> = Vec::new();
    let candidates = msg
        .file
        .iter()
        .chain(&msg.files)
        .filter_map(|f| f.name.as_deref())
        .chain(msg.attachments.iter().filter_map(|a| a.title.as_deref()));
    for name in candidates {
        if !name.is_empty() && !names.contains(&name) {
            names.push(name);
        }
    }
    let mut body = msg.msg.clone();
    for name in names {
        if !body.is_empty() {
            body.push(' ');
        }
        body.push_str(&format!("[attachment: {name}]"));
    }
    (MessageKind::Normal, body)
}

/// One wire message as a domain `Message` of `room`: the author is `u.username` or `unknown`, `ts`
/// is whole seconds or 0 for an unparsable date, and the body follows `kind_and_body`. The one
/// per-message mapper: history uses it for every message and `chat.sendMessage` for its answer.
pub fn message(room: &RoomId, msg: WireMsg) -> Message {
    let (kind, body) = kind_and_body(&msg);
    let edited = msg.edited_at.is_some() && kind != MessageKind::Deleted;
    let plain_text = is_plain_text(&msg, kind);
    Message {
        id: msg.id,
        room_id: room.clone(),
        author: msg.u.username.unwrap_or_else(|| "unknown".to_string()),
        ts: msg.ts.secs().unwrap_or(0),
        body,
        kind,
        send: SendState::Sent,
        edited,
        plain_text,
    }
}

/// True when the displayed body of `msg` is exactly the text the server stores (the
/// `Message.plain_text` property): an ordinary message with no file, no `files` entry and no
/// attachment, and not an end-to-end leftover. A system message, a tombstone, an encrypted message
/// and any message that gets ` [attachment: ..]` text appended are not plain.
fn is_plain_text(msg: &WireMsg, kind: MessageKind) -> bool {
    kind == MessageKind::Normal
        && msg.t.is_none()
        && msg.content.is_none()
        && msg.file.is_none()
        && msg.files.is_empty()
        && msg.attachments.is_empty()
}

/// One page of history (the server sends it newest first) as oldest-first messages.
///
/// The order is the server timestamp in milliseconds, then the id, so two messages in the same
/// second keep their order even though `Message.ts` holds whole seconds. A message with an
/// unparsable timestamp gets `ts` 0 and sorts first; it is never dropped. `oldest_ts` is the raw
/// server timestamp string of the message with the smallest milliseconds, kept verbatim for the
/// `latest` query of the next page (Pitfall 7); it is `None` for an empty page.
pub fn history(room: &RoomId, msgs: Vec<WireMsg>) -> HistoryPage {
    let mut keyed: Vec<(i64, Message)> = Vec::with_capacity(msgs.len());
    let mut oldest: Option<(i64, String)> = None;
    for msg in msgs {
        let millis = msg.ts.millis();
        if let (Some(ms), Some(raw)) = (millis, msg.ts.raw_iso())
            && oldest.as_ref().is_none_or(|(best, _)| ms < *best)
        {
            oldest = Some((ms, raw));
        }
        keyed.push((millis.unwrap_or(0), message(room, msg)));
    }
    keyed.sort_by(|(a_ms, a), (b_ms, b)| a_ms.cmp(b_ms).then_with(|| a.id.cmp(&b.id)));
    HistoryPage {
        messages: keyed.into_iter().map(|(_, m)| m).collect(),
        oldest_ts: oldest.map(|(_, raw)| raw),
    }
}

/// A page of `chat.search` results mapped one by one, keeping the server's order (newest first):
/// unlike history, the list is not re-sorted. Each result keeps the raw server timestamp string
/// beside it (verbatim, never recomputed), the `latest` value of its context request.
pub fn search_page(room: &RoomId, msgs: Vec<WireMsg>) -> SearchHits {
    let raw_ts = msgs.iter().map(|m| m.ts.raw_iso()).collect();
    SearchHits {
        messages: msgs.into_iter().map(|msg| message(room, msg)).collect(),
        raw_ts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::wire::{HistoryResp, PreferencesResp, RoomsResp, SubscriptionsResp};

    const SUBS: &str = include_str!("fixtures/subscriptions.json");
    const ROOMS: &str = include_str!("fixtures/rooms.json");
    const PREFS: &str = include_str!("fixtures/preferences.json");
    const KINDS: &str = include_str!("fixtures/history_kinds.json");

    fn fixture_subs() -> Vec<WireSub> {
        serde_json::from_str::<SubscriptionsResp>(SUBS)
            .expect("subscriptions fixture")
            .update
    }

    fn fixture_rooms() -> Vec<WireRoom> {
        serde_json::from_str::<RoomsResp>(ROOMS)
            .expect("rooms fixture")
            .update
    }

    fn sub(json: &str) -> WireSub {
        serde_json::from_str(json).expect("sub json")
    }

    fn record(json: &str) -> WireRoom {
        serde_json::from_str(json).expect("room json")
    }

    #[test]
    fn rooms_from_fixtures() {
        let mapped = rooms(&fixture_subs(), &fixture_rooms());
        let summary: Vec<(&str, RoomKind, bool, bool)> = mapped
            .iter()
            .map(|r| (r.name.as_str(), r.kind, r.unread, r.encrypted))
            .collect();
        assert_eq!(
            summary,
            [
                ("general", RoomKind::Channel, true, false),
                ("dev-team", RoomKind::PrivateGroup, true, false),
                ("alice", RoomKind::DirectMessage, true, false),
                ("random", RoomKind::Channel, false, false),
                ("platform", RoomKind::Team, false, false),
                ("Design review", RoomKind::Discussion, false, false),
                ("secrets", RoomKind::PrivateGroup, false, true),
            ]
        );
        let ids: Vec<&str> = mapped.iter().map(|r| r.id.0.as_str()).collect();
        for skipped in ["r-dm-carol", "r-livechat", "r-invited", "r-orphan"] {
            assert!(!ids.contains(&skipped), "{skipped} must be skipped");
        }
        assert!(
            mapped
                .windows(2)
                .all(|w| w[0].last_activity >= w[1].last_activity)
        );
        assert_eq!(mapped[0].last_activity, 1_794_823_200);
    }

    #[test]
    fn rooms_edges() {
        assert!(rooms(&[], &[]).is_empty());

        // Same name and same lm, different ids: two rows, ordered by id.
        let subs = [
            sub(r#"{"rid":"r-b","t":"c","name":"twin"}"#),
            sub(r#"{"rid":"r-a","t":"c","name":"twin"}"#),
        ];
        let records = [
            record(r#"{"_id":"r-b","lm":"2026-11-16T10:00:00.000Z"}"#),
            record(r#"{"_id":"r-a","lm":"2026-11-16T10:00:00.000Z"}"#),
        ];
        let mapped = rooms(&subs, &records);
        let ids: Vec<&str> = mapped.iter().map(|r| r.id.0.as_str()).collect();
        assert_eq!(ids, ["r-a", "r-b"]);

        // No lm and no usable subscription ts: activity 0.
        let subs = [
            sub(r#"{"rid":"r-1","t":"c","name":"one"}"#),
            sub(r#"{"rid":"r-2","t":"c","name":"two","ts":"garbage"}"#),
        ];
        let records = [
            record(r#"{"_id":"r-1"}"#),
            record(r#"{"_id":"r-2","lm":"also garbage"}"#),
        ];
        let mapped = rooms(&subs, &records);
        assert_eq!(mapped.len(), 2);
        assert!(mapped.iter().all(|r| r.last_activity == 0));

        // One subscription: one row. The subscription ts is the fallback for a missing lm.
        let subs = [sub(
            r#"{"rid":"r-1","t":"p","name":"solo","ts":"2026-11-01T00:00:00.000Z"}"#,
        )];
        let records = [record(r#"{"_id":"r-1"}"#)];
        let mapped = rooms(&subs, &records);
        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].last_activity, 1_793_491_200);
    }

    #[test]
    fn same_name_rooms_stay_separate() {
        let subs = [
            sub(r#"{"rid":"r-1","t":"c","name":"same","fname":"same"}"#),
            sub(r#"{"rid":"r-2","t":"p","name":"same","fname":"same"}"#),
        ];
        let records = [record(r#"{"_id":"r-1"}"#), record(r#"{"_id":"r-2"}"#)];
        assert_eq!(rooms(&subs, &records).len(), 2);
    }

    #[test]
    fn last_message_text_is_ignored() {
        let records = fixture_rooms();
        assert!(ROOMS.contains("SENTINEL-LASTMSG"));
        let mapped = rooms(&fixture_subs(), &records);
        assert!(
            !mapped
                .iter()
                .any(|r| format!("{r:?}").contains("SENTINEL-LASTMSG"))
        );
    }

    fn fixture_prefs() -> WirePrefs {
        serde_json::from_str::<PreferencesResp>(PREFS)
            .expect("preferences fixture")
            .preferences
    }

    fn folder(id: &str, name: &str, rooms: &[&str]) -> Folder {
        Folder {
            id: FolderId(id.to_string()),
            name: name.to_string(),
            room_ids: rooms.iter().map(|r| RoomId((*r).to_string())).collect(),
        }
    }

    #[test]
    fn folders_union() {
        let subs = fixture_subs();
        let mapped = rooms(&subs, &fixture_rooms());
        let prefs = fixture_prefs();
        let made = folders(&subs, &mapped, Some(&prefs));
        assert_eq!(
            made,
            [
                folder("favorites", "Favorites", &["r-random"]),
                folder("cat-work", "Work", &["r-devteam", "r-platform"]),
            ]
        );
        // Each room is listed in at most one folder.
        let mut seen = HashSet::new();
        for room_id in made.iter().flat_map(|f| &f.room_ids) {
            assert!(seen.insert(room_id), "{room_id:?} listed twice");
        }
    }

    #[test]
    fn folders_no_categories() {
        let subs = fixture_subs();
        let mapped = rooms(&subs, &fixture_rooms());
        assert_eq!(
            folders(&subs, &mapped, None),
            [folder("favorites", "Favorites", &["r-random"])]
        );

        // No favorite and no category: no folder at all.
        let plain = [sub(r#"{"rid":"r-1","t":"c","name":"one"}"#)];
        let records = [record(r#"{"_id":"r-1"}"#)];
        let mapped = rooms(&plain, &records);
        assert!(folders(&plain, &mapped, None).is_empty());

        // A favorite whose subscription was skipped (open = false) is not listed.
        let hidden = [
            sub(r#"{"rid":"r-1","t":"c","name":"one","f":true,"open":false}"#),
            sub(r#"{"rid":"r-2","t":"c","name":"two"}"#),
        ];
        let records = [record(r#"{"_id":"r-1"}"#), record(r#"{"_id":"r-2"}"#)];
        let mapped = rooms(&hidden, &records);
        assert!(folders(&hidden, &mapped, None).is_empty());
    }

    const HISTORY: &str = include_str!("fixtures/history.json");

    fn history_msgs() -> Vec<WireMsg> {
        serde_json::from_str::<crate::api::wire::HistoryResp>(HISTORY)
            .expect("history fixture")
            .messages
    }

    #[test]
    fn history_oldest_first() {
        let room = RoomId("r-devteam".to_string());
        let page = history(&room, history_msgs());
        let ids: Vec<&str> = page.messages.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["m1", "m2", "m3", "m4"]);
        // Two messages in one second keep their order although ts is whole seconds.
        assert_eq!(page.messages[1].ts, 1_794_823_200);
        assert_eq!(page.messages[2].ts, 1_794_823_200);
        assert_eq!(page.messages[0].author, "alice");
        assert_eq!(page.messages[0].room_id, room);
        assert_eq!(page.messages[3].body, "fourth and newest message");
        assert_eq!(page.oldest_ts.as_deref(), Some("2026-11-16T09:59:00.000Z"));
    }

    #[test]
    fn history_date_forms_and_edges() {
        let room = RoomId("r".to_string());
        let wire = |json: &str| -> Vec<WireMsg> { serde_json::from_str(json).expect("msgs") };

        // An EJSON ts parses to seconds, and its raw form is converted to ISO.
        let ejson =
            wire(r#"[{"_id":"e","msg":"x","ts":{"$date":1794823200123},"u":{"username":"a"}}]"#);
        let page = history(&room, ejson);
        assert_eq!(page.messages[0].ts, 1_794_823_200);
        assert_eq!(page.oldest_ts.as_deref(), Some("2026-11-16T10:00:00.123Z"));

        // An unparsable ts gets 0, sorts first and is still returned.
        let bad = wire(
            r#"[{"_id":"b","msg":"x","ts":"garbage","u":{}},
                {"_id":"g","msg":"y","ts":"2026-11-16T10:00:00.000Z","u":{"username":"a"}}]"#,
        );
        let page = history(&room, bad);
        assert_eq!(page.messages.len(), 2);
        assert_eq!(page.messages[0].id, "b");
        assert_eq!(page.messages[0].ts, 0);
        assert_eq!(page.messages[0].author, "unknown");
        assert_eq!(page.oldest_ts.as_deref(), Some("2026-11-16T10:00:00.000Z"));

        // An empty page has no oldest timestamp.
        let page = history(&room, Vec::new());
        assert!(page.messages.is_empty());
        assert_eq!(page.oldest_ts, None);

        // One message: it is both the newest and the oldest.
        let one =
            wire(r#"[{"_id":"o","msg":"x","ts":"2026-11-16T10:00:00.000Z","u":{"username":"a"}}]"#);
        let page = history(&room, one);
        assert_eq!(page.messages.len(), 1);
        assert_eq!(page.oldest_ts.as_deref(), Some("2026-11-16T10:00:00.000Z"));
    }

    #[test]
    fn rooms_private_flag() {
        let mapped = rooms(&fixture_subs(), &fixture_rooms());
        let private: Vec<(&str, bool)> = mapped
            .iter()
            .map(|r| (r.name.as_str(), r.private))
            .collect();
        assert!(private.contains(&("dev-team", true)));
        assert!(private.contains(&("secrets", true)));
        assert!(private.contains(&("general", false)));
        assert!(private.contains(&("alice", false)));
    }

    fn mapped(json: &str) -> Vec<Message> {
        let resp: HistoryResp = serde_json::from_str(json).expect("history json");
        history(&RoomId("r-general".to_string()), resp.messages).messages
    }

    fn one(json: &str) -> Message {
        let mut all = mapped(&format!("{{\"messages\":[{json}]}}"));
        assert_eq!(all.len(), 1);
        all.remove(0)
    }

    #[test]
    fn history_kinds() {
        let all = mapped(KINDS);
        let get = |id: &str| all.iter().find(|m| m.id == id).expect(id);
        let shape = |id: &str| (get(id).kind, get(id).author.clone(), get(id).body.clone());
        let sys =
            |author: &str, body: &str| (MessageKind::System, author.to_string(), body.to_string());
        let normal =
            |author: &str, body: &str| (MessageKind::Normal, author.to_string(), body.to_string());
        assert_eq!(shape("k-joined"), sys("carol", "* joined the room"));
        assert_eq!(shape("k-added"), sys("alice", "* added bob"));
        assert_eq!(shape("k-topic"), sys("alice", "* changed the topic"));
        assert_eq!(shape("k-unknown"), sys("alice", "* system message"));
        assert_eq!(
            shape("k-attach"),
            normal("bob", "see attached [attachment: report.pdf]")
        );
        assert_eq!(
            shape("k-files"),
            normal("alice", "[attachment: a.png] [attachment: b.txt]")
        );
        assert_eq!(shape("k-e2e"), normal("bob", "[encrypted message]"));
        for m in &all {
            assert!(
                !m.body.contains("U0VOVElORUwtQ0lQSEVS"),
                "ciphertext reached a body"
            );
        }
    }

    #[test]
    fn system_texts() {
        let sys = |t: &str, msg: &str| {
            let m = one(&format!(
                "{{\"_id\":\"s\",\"t\":\"{t}\",\"msg\":\"{msg}\",\"ts\":\"2026-11-16T10:00:00.000Z\",\"u\":{{\"username\":\"dave\"}}}}"
            ));
            assert_eq!(m.kind, MessageKind::System, "{t}");
            assert_eq!(m.author, "dave");
            m.body
        };
        assert_eq!(sys("ul", "dave"), "* left the room");
        assert_eq!(sys("ru", "erin"), "* removed erin");
        assert_eq!(sys("r", "new-name"), "* renamed the room");
        assert_eq!(sys("au", "erin"), "* added erin");
        assert_eq!(sys("room_changed_topic", "x"), "* changed the topic");
        assert_eq!(sys("something-new", "x"), "* system message");
    }

    #[test]
    fn content_object_is_encrypted_even_without_type() {
        let m = one(
            "{\"_id\":\"c\",\"msg\":\"U0VOVElORUwtQ0lQSEVS\",\"content\":{\"ciphertext\":\"x\"},\"ts\":\"2026-11-16T10:00:00.000Z\",\"u\":{\"username\":\"bob\"}}",
        );
        assert_eq!(m.kind, MessageKind::Normal);
        assert_eq!(m.body, "[encrypted message]");
    }

    #[test]
    fn plain_text_property() {
        let wire = |extra: &str| {
            one(&format!(
                "{{\"_id\":\"p\",\"msg\":\"hello\"{extra},\"ts\":\"2026-11-16T10:00:00.000Z\",\"u\":{{\"username\":\"bob\"}}}}"
            ))
        };
        assert!(wire("").plain_text, "a plain message");
        assert!(wire(",\"editedAt\":\"2026-11-16T10:05:00.000Z\"").plain_text);
        assert!(!wire(",\"file\":{\"name\":\"a.png\"}").plain_text, "a file");
        assert!(
            !wire(",\"files\":[{\"name\":\"a.png\"}]").plain_text,
            "a files entry"
        );
        assert!(
            !wire(",\"attachments\":[{\"title\":\"t.pdf\"}]").plain_text,
            "an attachment title"
        );
        assert!(
            !wire(",\"attachments\":[{}]").plain_text,
            "an attachment without a title"
        );
        assert!(
            !wire(",\"content\":{\"ciphertext\":\"x\"}").plain_text,
            "an encrypted message"
        );
        assert!(!wire(",\"t\":\"e2e\"").plain_text, "an e2e type");
        assert!(!wire(",\"t\":\"uj\"").plain_text, "a system message");
        assert!(!wire(",\"t\":\"rm\"").plain_text, "a tombstone");
    }

    #[test]
    fn attachment_alone_has_no_leading_space() {
        let m = one(
            "{\"_id\":\"a\",\"msg\":\"\",\"attachments\":[{\"title\":\"t.pdf\"},{}],\"ts\":\"2026-11-16T10:00:00.000Z\",\"u\":{\"username\":\"bob\"}}",
        );
        assert_eq!(m.body, "[attachment: t.pdf]");
    }

    // Captured from the real local Rocket.Chat 8.9.1 by `dev/seed.py capture` (plan 03-02). These
    // pin the mapping to real server output; the captured files are never edited to pass (T-03-31).
    const DOCKER_SUBS: &str = include_str!("fixtures/docker/subscriptions.json");
    const DOCKER_ROOMS: &str = include_str!("fixtures/docker/rooms.json");
    const DOCKER_PREFS: &str = include_str!("fixtures/docker/preferences.json");
    const DOCKER_RT_PUBLIC: &str = include_str!("fixtures/docker/history_rt_public.json");
    const DOCKER_DM_ALICE: &str = include_str!("fixtures/docker/history_dm_alice.json");

    fn docker_rooms() -> Vec<Room> {
        let subs = serde_json::from_str::<SubscriptionsResp>(DOCKER_SUBS)
            .expect("docker subscriptions")
            .update;
        let records = serde_json::from_str::<RoomsResp>(DOCKER_ROOMS)
            .expect("docker rooms")
            .update;
        rooms(&subs, &records)
    }

    fn docker_room<'a>(mapped: &'a [Room], name: &str) -> &'a Room {
        mapped
            .iter()
            .find(|r| r.name == name)
            .unwrap_or_else(|| panic!("room {name} is not listed"))
    }

    fn docker_history(json: &str) -> Vec<WireMsg> {
        serde_json::from_str::<crate::api::wire::HistoryResp>(json)
            .expect("docker history")
            .messages
    }

    #[test]
    fn docker_fixtures_rooms() {
        let mapped = docker_rooms();
        let kind_of_name = |name: &str| docker_room(&mapped, name).kind;
        assert_eq!(kind_of_name("rt-public"), RoomKind::Channel);
        assert_eq!(kind_of_name("general"), RoomKind::Channel);
        assert_eq!(kind_of_name("rt-private"), RoomKind::PrivateGroup);
        assert!(docker_room(&mapped, "rt-private").private);
        // A9: a direct message is named by the other user's username, not the full name `Alice`.
        assert_eq!(kind_of_name("alice"), RoomKind::DirectMessage);
        assert!(mapped.iter().all(|r| r.name != "Alice"));
        assert_eq!(kind_of_name("secrets"), RoomKind::PrivateGroup);
        assert!(docker_room(&mapped, "secrets").encrypted);
        // The discussion subscription's `name` is the room id; the title comes from `fname`.
        assert_eq!(kind_of_name("rt-discussion"), RoomKind::Discussion);
        assert_eq!(kind_of_name("rt-team"), RoomKind::Team);
        // The closed bob DM (open = false) is not listed.
        assert!(mapped.iter().all(|r| r.name != "bob" && r.name != "Bob"));
        assert_eq!(mapped.len(), 7);
        // Only secrets is encrypted.
        let encrypted: Vec<&str> = mapped
            .iter()
            .filter(|r| r.encrypted)
            .map(|r| r.name.as_str())
            .collect();
        assert_eq!(encrypted, ["secrets"]);
        // A2: the real ISO dates parse, including for `general`, which has no `lm`.
        assert!(mapped.iter().all(|r| r.last_activity > 0));
    }

    #[test]
    fn docker_fixtures_folders() {
        let subs = serde_json::from_str::<SubscriptionsResp>(DOCKER_SUBS)
            .expect("docker subscriptions")
            .update;
        let prefs = serde_json::from_str::<PreferencesResp>(DOCKER_PREFS)
            .expect("docker preferences")
            .preferences;
        let mapped = docker_rooms();
        let made = folders(&subs, &mapped, Some(&prefs));
        let favorites = made
            .iter()
            .find(|f| f.name == "Favorites")
            .expect("a Favorites folder");
        assert_eq!(made[0].name, "Favorites", "Favorites comes first");
        assert_eq!(
            favorites.room_ids,
            [docker_room(&mapped, "rt-private").id.clone()]
        );
        // The Work folder exists when the captured preferences hold that category.
        if prefs
            .sidebar_categories
            .iter()
            .any(|c| c.name == "Work" && !c.default)
        {
            let work = made.iter().find(|f| f.name == "Work").expect("Work folder");
            assert_eq!(work.room_ids, [docker_room(&mapped, "rt-team").id.clone()]);
        }
        // Assignment is exclusive: no room sits in two folders.
        let mut all: Vec<&RoomId> = made.iter().flat_map(|f| f.room_ids.iter()).collect();
        let before = all.len();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), before);
    }

    #[test]
    fn docker_fixtures_history() {
        let room = RoomId("r".to_string());
        let public = history(&room, docker_history(DOCKER_RT_PUBLIC));
        assert_eq!(public.messages.len(), 50);
        assert!(public.messages.iter().all(|m| m.ts > 0));
        assert!(public.messages.windows(2).all(|w| w[0].ts <= w[1].ts));
        // oldest_ts is the raw server string of the oldest message, parseable as a real date.
        let oldest = public.oldest_ts.expect("oldest_ts");
        let date: crate::api::wire::ApiDate =
            serde_json::from_str(&format!("\"{oldest}\"")).expect("date json");
        assert!(date.millis().is_some_and(|ms| ms > 0));
        assert!(
            public
                .messages
                .iter()
                .all(|m| m.author == "dev" || m.author == "alice")
        );
        // Real attachments carry the file title, listed once.
        assert!(
            public
                .messages
                .iter()
                .any(|m| m.body.contains("[attachment: report.pdf]")),
            "the report.pdf attachment is mapped"
        );
        // Every message the server marks with a type `t` is a System line, and the newest page
        // holds at least one (discussion-created); a `uj` join message maps the same way.
        let typed: Vec<String> = docker_history(DOCKER_RT_PUBLIC)
            .into_iter()
            .chain(docker_history(DOCKER_DM_ALICE))
            .filter(|m| m.t.is_some())
            .map(|m| m.id)
            .collect();
        assert!(
            !typed.is_empty(),
            "the captured pages hold a system message"
        );
        let alice = history(&room, docker_history(DOCKER_DM_ALICE));
        assert!(alice.messages.iter().all(|m| m.ts > 0));
        assert!(
            alice
                .messages
                .iter()
                .all(|m| m.author == "dev" || m.author == "alice")
        );
        assert!(alice.oldest_ts.is_some());
        for id in typed {
            let mapped = public
                .messages
                .iter()
                .chain(&alice.messages)
                .find(|m| m.id == id)
                .expect("typed message mapped");
            assert_eq!(mapped.kind, MessageKind::System, "{id}");
            assert!(mapped.body.starts_with("* "), "{id}");
        }
    }

    #[test]
    fn message_single() {
        // A sendMessage-shaped message maps through `message` to the fields history would produce.
        let json = r#"{"_id":"rt-test-1","rid":"r-general","msg":"hello","ts":"2026-11-17T10:00:00.123Z","u":{"_id":"u1","username":"smoke","name":"Smoke"},"md":[],"urls":[]}"#;
        let wire = || serde_json::from_str::<WireMsg>(json).expect("wire message");
        let room = RoomId("r-general".to_string());
        let single = message(&room, wire());
        assert_eq!(single.id, "rt-test-1");
        assert_eq!(single.room_id, room);
        assert_eq!(single.author, "smoke");
        assert_eq!(single.ts, 1_794_909_600);
        assert_eq!(single.body, "hello");
        assert_eq!(single.kind, MessageKind::Normal);
        assert_eq!(single.send, SendState::Sent);
        let page = history(&room, vec![wire()]);
        assert_eq!(page.messages, [single]);
    }

    #[test]
    fn edited_flag_follows_edited_at() {
        let room = RoomId("r-general".to_string());
        let base = |extra: &str| -> Message {
            let json = format!(
                r#"{{"_id":"m1","rid":"r-general","msg":"hello","ts":"2026-11-17T10:00:00.123Z","u":{{"username":"alice"}}{extra}}}"#
            );
            message(&room, serde_json::from_str(&json).expect("wire message"))
        };
        assert!(!base("").edited);
        let edited = base(
            r#","editedAt":"2026-11-17T10:05:00.000Z","editedBy":{"_id":"u2","username":"bob"}"#,
        );
        assert!(edited.edited);
        // The EJSON date form (live stream) counts too.
        assert!(base(r#","editedAt":{"$date":1794909900000}"#).edited);
        assert_eq!(edited.body, "hello");
    }

    #[test]
    fn rm_message_is_a_tombstone_not_a_system_message() {
        let json = r#"{"_id":"m1","rid":"r","msg":"","t":"rm","ts":"2026-11-17T10:00:00.000Z","editedAt":"2026-11-17T10:05:00.000Z","u":{"username":"joliveira"}}"#;
        let m = message(
            &RoomId("r".to_string()),
            serde_json::from_str(json).unwrap(),
        );
        assert_eq!(m.kind, MessageKind::Deleted);
        assert_eq!(m.body, "");
        assert!(!m.edited);
    }
}
