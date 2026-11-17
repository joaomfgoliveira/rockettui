//! Pure mapping from the wire types to the domain types of `data`. No IO, no clock.
//!
//! `subscriptions.get` says which rooms the user joined and how they show; `rooms.get` adds what
//! the subscription lacks (`encrypted`, `lm`, `teamMain`). The two are joined on the room id.

use crate::api::wire::{WireRoom, WireSub};
use crate::data::{Room, RoomId, RoomKind};
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::wire::{RoomsResp, SubscriptionsResp};

    const SUBS: &str = include_str!("fixtures/subscriptions.json");
    const ROOMS: &str = include_str!("fixtures/rooms.json");

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
}
