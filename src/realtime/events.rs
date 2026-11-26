//! Stream events as `Action`s. Pure: a decoded `changed` frame in, at most one `Action` out.
//!
//! Message events go through `api::map::message`, the one per-message mapper, so sanitizing and
//! the encrypted-message rule are the same as for history. A payload that does not decode gives
//! `None` and is counted by the caller; nothing from it is logged.

use crate::action::Action;
use crate::api::map;
use crate::api::wire::{WireMsg, WireSubDoc};
use crate::data::{RoomId, ThreadUnreadUpdate, UnreadUpdate};
use crate::realtime::ddp::ServerFrame;
use serde_json::Value;
use std::collections::HashSet;

/// The stream that carries new messages of every joined room (`__my_messages__`).
pub const MESSAGES_STREAM: &str = "stream-room-messages";
pub const MESSAGES_EVENT: &str = "__my_messages__";
/// The per-user notification stream; its `<uid>/subscriptions-changed` event carries unread.
pub const USER_STREAM: &str = "stream-notify-user";
const SUBSCRIPTIONS_SUFFIX: &str = "/subscriptions-changed";
/// The per-room notification stream; its `<rid>/deleteMessage` event carries only `{_id}`.
pub const ROOM_STREAM: &str = "stream-notify-room";
pub const DELETE_SUFFIX: &str = "/deleteMessage";

/// What a subscription document says about the unread state. Keys the document lacks stay `None`;
/// `None` overall means the update is about something else (the last-seen time, for example) and
/// must not change anything. The one place the rule lives.
fn set_of(list: &Option<Vec<String>>) -> Option<HashSet<String>> {
    list.as_ref().map(|ids| ids.iter().cloned().collect())
}

fn unread_of(doc: &WireSubDoc) -> Option<UnreadUpdate> {
    let update = UnreadUpdate {
        alert: doc.alert,
        unread: doc.unread,
        user_mentions: doc.user_mentions,
        group_mentions: doc.group_mentions,
        threads: ThreadUnreadUpdate {
            unread: set_of(&doc.tunread),
            user: set_of(&doc.tunread_user),
            group: set_of(&doc.tunread_group),
        },
    };
    (!update.is_empty()).then_some(update)
}

/// The Action a `changed` frame stands for, or `None` for a frame that is not an event this client
/// uses or whose payload does not decode.
pub fn action_for(frame: ServerFrame) -> Option<Action> {
    let ServerFrame::Changed {
        collection,
        event,
        args,
    } = frame
    else {
        return None;
    };
    match collection.as_str() {
        MESSAGES_STREAM if event == MESSAGES_EVENT => {
            let wire: WireMsg = serde_json::from_value(args.into_iter().next()?).ok()?;
            let room = RoomId(wire.rid.clone().filter(|rid| !rid.is_empty())?);
            Some(Action::RtMessage(map::message(&room, wire)))
        }
        USER_STREAM if event.ends_with(SUBSCRIPTIONS_SUFFIX) => {
            let mut args = args.into_iter();
            let kind = args.next()?;
            // A removed subscription is the user leaving a room; the room list handles that.
            if !matches!(kind.as_str(), Some("updated" | "inserted" | "changed")) {
                return None;
            }
            let doc: WireSubDoc = serde_json::from_value::<Value>(args.next()?)
                .ok()
                .and_then(|v| serde_json::from_value(v).ok())?;
            Some(Action::RtSubscription {
                update: unread_of(&doc)?,
                room: RoomId(doc.rid),
            })
        }
        ROOM_STREAM => {
            let rid = event
                .strip_suffix(DELETE_SUFFIX)
                .filter(|r| !r.is_empty())?;
            let doc = args.into_iter().next()?;
            let id = doc.get("_id")?.as_str().filter(|id| !id.is_empty())?;
            Some(Action::RtDeleted {
                room: RoomId(rid.to_string()),
                id: id.to_string(),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{MessageKind, SendState};
    use crate::realtime::ddp::parse;

    fn changed(collection: &str, event: &str, args: &str) -> ServerFrame {
        parse(&format!(
            r#"{{"msg":"changed","collection":"{collection}","id":"id","fields":{{"eventName":"{event}","args":{args}}}}}"#
        ))
        .expect("frame")
    }

    #[test]
    fn message_event_becomes_rt_message() {
        let frame = changed(
            MESSAGES_STREAM,
            MESSAGES_EVENT,
            r#"[{"_id":"m1","rid":"r1","msg":"hi","ts":{"$date":1794823200123},"u":{"_id":"u1","username":"alice"},"_updatedAt":{"$date":1}}]"#,
        );
        match action_for(frame) {
            Some(Action::RtMessage(m)) => {
                assert_eq!(m.id, "m1");
                assert_eq!(m.room_id, RoomId("r1".to_string()));
                assert_eq!(m.author, "alice");
                assert_eq!(m.body, "hi");
                assert_eq!(m.ts, 1_794_823_200);
                assert_eq!(m.kind, MessageKind::Normal);
                assert_eq!(m.send, SendState::Sent);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn encrypted_message_keeps_the_placeholder_rule() {
        let frame = changed(
            MESSAGES_STREAM,
            MESSAGES_EVENT,
            r#"[{"_id":"m2","rid":"r1","msg":"","ts":{"$date":1},"u":{"username":"bob"},"t":"e2e","content":{"algorithm":"x","ciphertext":"SECRET"}}]"#,
        );
        match action_for(frame) {
            Some(Action::RtMessage(m)) => assert!(!m.body.contains("SECRET")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn unusable_payloads_give_none() {
        // No room id, wrong event name, not an object, unrelated stream.
        for frame in [
            changed(
                MESSAGES_STREAM,
                MESSAGES_EVENT,
                r#"[{"_id":"m1","ts":{"$date":1},"u":{}}]"#,
            ),
            changed(
                MESSAGES_STREAM,
                "other",
                r#"[{"_id":"m1","rid":"r","ts":{"$date":1},"u":{}}]"#,
            ),
            changed(MESSAGES_STREAM, MESSAGES_EVENT, r#"["text"]"#),
            changed(MESSAGES_STREAM, MESSAGES_EVENT, "[]"),
            // A delete without an id, with an empty id or room, or an unrelated room event.
            changed("stream-notify-room", "r/deleteMessage", "[{}]"),
            changed("stream-notify-room", "r/deleteMessage", r#"[{"_id":""}]"#),
            changed("stream-notify-room", "r/deleteMessage", r#"["m1"]"#),
            changed("stream-notify-room", "r/deleteMessage", "[]"),
            changed("stream-notify-room", "/deleteMessage", r#"[{"_id":"m1"}]"#),
            changed("stream-notify-room", "r/typing", r#"["alice",true]"#),
            ServerFrame::Pong,
        ] {
            assert!(action_for(frame).is_none());
        }
    }

    #[test]
    fn delete_event_becomes_rt_deleted() {
        let frame = changed(ROOM_STREAM, "r1/deleteMessage", r#"[{"_id":"m1"}]"#);
        assert_eq!(
            action_for(frame),
            Some(Action::RtDeleted {
                room: RoomId("r1".to_string()),
                id: "m1".to_string()
            })
        );
    }

    #[test]
    fn subscription_events() {
        let set = changed(
            USER_STREAM,
            "u1/subscriptions-changed",
            r#"["updated",{"rid":"r1","alert":true,"unread":2,"name":"x"}]"#,
        );
        assert_eq!(
            action_for(set),
            Some(Action::RtSubscription {
                room: RoomId("r1".to_string()),
                update: UnreadUpdate {
                    alert: Some(true),
                    unread: Some(2),
                    ..UnreadUpdate::default()
                }
            })
        );
        let cleared = changed(
            USER_STREAM,
            "u1/subscriptions-changed",
            r#"["updated",{"rid":"r1","alert":false,"unread":0}]"#,
        );
        assert_eq!(
            action_for(cleared),
            Some(Action::RtSubscription {
                room: RoomId("r1".to_string()),
                update: UnreadUpdate {
                    alert: Some(false),
                    unread: Some(0),
                    ..UnreadUpdate::default()
                }
            })
        );
        // An update with neither key (measured on the servers) leaves the flag alone.
        let other_update = changed(
            USER_STREAM,
            "u1/subscriptions-changed",
            r#"["updated",{"rid":"r1","ls":{"$date":1}}]"#,
        );
        assert_eq!(action_for(other_update), None);
        let removed = changed(
            USER_STREAM,
            "u1/subscriptions-changed",
            r#"["removed",{"rid":"r1"}]"#,
        );
        assert_eq!(action_for(removed), None);
        let other = changed(
            USER_STREAM,
            "u1/rooms-changed",
            r#"["updated",{"rid":"r1"}]"#,
        );
        assert_eq!(action_for(other), None);
    }

    fn ids(list: &[&str]) -> Option<HashSet<String>> {
        Some(list.iter().map(|s| s.to_string()).collect())
    }

    fn thread_update(doc: &str) -> Option<UnreadUpdate> {
        let frame = changed(
            USER_STREAM,
            "u1/subscriptions-changed",
            &format!(r#"["updated",{doc}]"#),
        );
        match action_for(frame) {
            Some(Action::RtSubscription { update, .. }) => Some(update),
            _ => None,
        }
    }

    #[test]
    fn thread_fields_full_partial_and_empty() {
        // A full update carries all three sets.
        let full = thread_update(
            r#"{"rid":"r1","tunread":["a","b"],"tunreadUser":["b"],"tunreadGroup":["a"]}"#,
        )
        .expect("full");
        assert_eq!(full.threads.unread, ids(&["a", "b"]));
        assert_eq!(full.threads.user, ids(&["b"]));
        assert_eq!(full.threads.group, ids(&["a"]));
        // A thread-only update is about unread: it must not be dropped as "something else".
        assert_eq!(full.alert, None);
        // A partial update keeps the fields it lacks as None.
        let partial = thread_update(r#"{"rid":"r1","tunread":["a"]}"#).expect("partial");
        assert_eq!(partial.threads.unread, ids(&["a"]));
        assert_eq!(partial.threads.user, None);
        assert_eq!(partial.threads.group, None);
        // An empty list is a value: it replaces the held set.
        let empty = thread_update(r#"{"rid":"r1","tunread":[],"tunreadUser":[]}"#).expect("empty");
        assert_eq!(empty.threads.unread, ids(&[]));
        assert_eq!(empty.threads.user, ids(&[]));
        assert_eq!(empty.threads.group, None);
        // None of the thread fields and none of the unread keys: nothing to apply.
        assert!(thread_update(r#"{"rid":"r1","ls":{"$date":1}}"#).is_none());
        // Thread fields next to the room keys travel together.
        let both =
            thread_update(r#"{"rid":"r1","unread":2,"alert":true,"tunread":["a"]}"#).expect("both");
        assert_eq!((both.unread, both.alert), (Some(2), Some(true)));
        assert_eq!(both.threads.unread, ids(&["a"]));
    }

    #[test]
    fn thread_fields_from_captured_updates() {
        for (json, label) in [
            (
                include_str!("../api/fixtures/docker/thread_unread_update.json"),
                "8.9.1",
            ),
            (
                include_str!("../api/fixtures/docker/thread_unread_update_9.json"),
                "9.0.0",
            ),
        ] {
            let doc: Value = serde_json::from_str(json).expect("fixture");
            let updates = doc["updates"].as_array().expect("updates");
            assert!(!updates.is_empty(), "{label}");
            for entry in updates {
                let text = serde_json::to_string(&serde_json::json!([
                    entry["action"],
                    entry["subscription"]
                ]))
                .expect("args");
                let update = thread_update_raw(&text).expect("decoded");
                assert!(update.threads.unread.is_some(), "{label}: tunread carried");
            }
        }
    }

    fn thread_update_raw(args: &str) -> Option<UnreadUpdate> {
        match action_for(changed(USER_STREAM, "u1/subscriptions-changed", args)) {
            Some(Action::RtSubscription { update, .. }) => Some(update),
            _ => None,
        }
    }
}
