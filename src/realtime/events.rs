//! Stream events as `Action`s. Pure: a decoded `changed` frame in, at most one `Action` out.
//!
//! Message events go through `api::map::message`, the one per-message mapper, so sanitizing and
//! the encrypted-message rule are the same as for history. A payload that does not decode gives
//! `None` and is counted by the caller; nothing from it is logged.

use crate::action::Action;
use crate::api::map;
use crate::api::wire::{WireMsg, WireSubDoc};
use crate::data::RoomId;
use crate::realtime::ddp::ServerFrame;
use serde_json::Value;

/// The stream that carries new messages of every joined room (`__my_messages__`).
pub const MESSAGES_STREAM: &str = "stream-room-messages";
pub const MESSAGES_EVENT: &str = "__my_messages__";
/// The per-user notification stream; its `<uid>/subscriptions-changed` event carries unread.
pub const USER_STREAM: &str = "stream-notify-user";
const SUBSCRIPTIONS_SUFFIX: &str = "/subscriptions-changed";

/// Whether a subscription document means "unread": the server's `alert` flag or a positive unread
/// count. `None` when the document carries neither key: such an update is about something else
/// (the last-seen time, for example) and must not change the flag. The one place the rule lives.
fn unread_of(doc: &WireSubDoc) -> Option<bool> {
    if doc.alert.is_none() && doc.unread.is_none() {
        return None;
    }
    Some(doc.alert.unwrap_or(false) || doc.unread.unwrap_or(0) > 0)
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
                unread: unread_of(&doc)?,
                room: RoomId(doc.rid),
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
            changed("stream-notify-room", "r/deleteMessage", r#"[{"_id":"m1"}]"#),
            ServerFrame::Pong,
        ] {
            assert!(action_for(frame).is_none());
        }
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
                unread: true
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
                unread: false
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
}
