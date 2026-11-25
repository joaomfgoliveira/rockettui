//! Tests of the realtime task against an in-process fake WebSocket server on loopback. Frame
//! contents here are made-up sentinels; nothing is printed.

use super::*;
use crate::data::RoomId;
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;

const TOKEN: &str = "SENTINEL-TOKEN-9c41";
const BODY: &str = "SENTINEL-BODY-7f3a";

type Server = WebSocketStream<TcpStream>;

async fn listener() -> (TcpListener, RtConfig) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let config = RtConfig {
        url: Url::parse(&format!("ws://127.0.0.1:{port}/websocket")).expect("url"),
        user_id: Secret::new("u1".to_string()),
        token: Secret::new(TOKEN.to_string()),
    };
    (listener, config)
}

async fn accept(listener: &TcpListener) -> Server {
    let (stream, _) = listener.accept().await.expect("accept");
    tokio_tungstenite::accept_async(stream)
        .await
        .expect("upgrade")
}

async fn read_json(ws: &mut Server) -> Value {
    loop {
        match ws.next().await.expect("client frame").expect("ok") {
            WsMessage::Text(text) => return serde_json::from_str(text.as_str()).expect("json"),
            _ => continue,
        }
    }
}

async fn send_json(ws: &mut Server, value: Value) {
    ws.send(WsMessage::text(value.to_string()))
        .await
        .expect("send");
}

/// Play the server side of the handshake. Returns the login frame and the two `sub` frames, or
/// `None` after refusing the login.
async fn handshake(ws: &mut Server, accept_login: bool) -> Option<(Value, Vec<Value>)> {
    send_json(ws, json!({"server_id": "0"})).await;
    assert_eq!(read_json(ws).await["msg"], "connect");
    send_json(ws, json!({"msg": "connected", "session": "s"})).await;
    let login = read_json(ws).await;
    assert_eq!(login["method"], "login");
    if !accept_login {
        send_json(
            ws,
            json!({"msg": "result", "id": login["id"], "error": {"error": 403, "reason": "no"}}),
        )
        .await;
        return None;
    }
    send_json(
        ws,
        json!({"msg": "result", "id": login["id"], "result": {"id": "u1", "token": "t"}}),
    )
    .await;
    let subs = vec![read_json(ws).await, read_json(ws).await];
    for sub in &subs {
        send_json(ws, json!({"msg": "ready", "subs": [sub["id"]]})).await;
    }
    Some((login, subs))
}

fn message_event(id: &str, text: &str) -> Value {
    json!({
        "msg": "changed", "collection": "stream-room-messages", "id": "x",
        "fields": {"eventName": "__my_messages__", "args": [{
            "_id": id, "rid": "r1", "msg": text, "ts": {"$date": 1_794_823_200_000i64},
            "u": {"_id": "u2", "username": "alice"}
        }]}
    })
}

/// Real-time timings scaled down so the tests finish fast. A paused clock cannot be used here: the
/// fake server speaks over a real socket, and a paused runtime jumps its clock whenever it waits
/// for I/O, which fires the handshake timeout.
fn fast() -> Timing {
    Timing {
        silence: Duration::from_millis(300),
        handshake: Duration::from_secs(5),
        backoff_base: Duration::from_millis(100),
        backoff_max: Duration::from_millis(400),
        stable_after: Duration::from_secs(3600),
        jitter: false,
    }
}

#[tokio::test]
async fn message_frame_yields_one_rt_message_and_garbage_is_skipped() {
    let (listener, config) = listener().await;
    let (tx, mut rx) = mpsc::channel(16);
    let _handle = spawn(config, tx, fast());
    let mut ws = accept(&listener).await;
    let (login, subs) = handshake(&mut ws, true).await.expect("accepted");
    assert_eq!(login["params"][0]["resume"], TOKEN);
    assert_eq!(subs[0]["name"], "stream-room-messages");
    assert_eq!(subs[0]["params"], json!(["__my_messages__", false]));
    assert_eq!(subs[1]["name"], "stream-notify-user");
    assert_eq!(
        subs[1]["params"],
        json!(["u1/subscriptions-changed", false])
    );

    assert_eq!(rx.recv().await, Some(Action::RtConnected));
    ws.send(WsMessage::text(format!("not json {BODY}")))
        .await
        .unwrap();
    send_json(&mut ws, message_event("m1", BODY)).await;
    match rx.recv().await {
        Some(Action::RtMessage(m)) => {
            assert_eq!(m.id, "m1");
            assert_eq!(m.room_id, RoomId("r1".to_string()));
            assert_eq!(m.body, BODY);
        }
        other => panic!("{other:?}"),
    }
    // Exactly one message came out of the two frames, and nothing printed the content.
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn extra_sub_goes_out_through_the_seam() {
    let (listener, config) = listener().await;
    let (tx, mut rx) = mpsc::channel(16);
    let handle = spawn(config, tx, fast());
    let mut ws = accept(&listener).await;
    handshake(&mut ws, true).await.expect("accepted");
    assert_eq!(rx.recv().await, Some(Action::RtConnected));
    let sent = handle
        .subscribe(ExtraSub {
            name: "stream-notify-room".to_string(),
            params: vec![json!("r1/deleteMessage"), json!(false)],
        })
        .await;
    assert!(sent);
    let frame = read_json(&mut ws).await;
    assert_eq!(frame["msg"], "sub");
    assert_eq!(frame["name"], "stream-notify-room");
    assert_eq!(frame["params"], json!(["r1/deleteMessage", false]));
}

#[tokio::test]
async fn ping_is_answered_at_once() {
    let (listener, config) = listener().await;
    let (tx, mut rx) = mpsc::channel(16);
    let _handle = spawn(config, tx, fast());
    let mut ws = accept(&listener).await;
    handshake(&mut ws, true).await.expect("accepted");
    assert_eq!(rx.recv().await, Some(Action::RtConnected));
    send_json(&mut ws, json!({"msg": "ping", "id": "p7"})).await;
    let pong = read_json(&mut ws).await;
    assert_eq!(pong, json!({"msg": "pong", "id": "p7"}));
    send_json(&mut ws, json!({"msg": "ping"})).await;
    assert_eq!(read_json(&mut ws).await, json!({"msg": "pong"}));
    // A ping is not an event for the reducer.
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn silence_is_detected() {
    let (listener, config) = listener().await;
    let (tx, mut rx) = mpsc::channel(16);
    let _handle = spawn(config, tx, fast());
    let mut ws = accept(&listener).await;
    handshake(&mut ws, true).await.expect("accepted");
    assert_eq!(rx.recv().await, Some(Action::RtConnected));
    let before = Instant::now();
    // The server says nothing: the connection is dropped once the silence limit passes.
    assert_eq!(rx.recv().await, Some(Action::RtDisconnected));
    assert!(before.elapsed() >= Duration::from_millis(300));
    assert!(before.elapsed() < Duration::from_secs(3));
}

#[tokio::test]
async fn close_is_detected() {
    let (listener, config) = listener().await;
    let (tx, mut rx) = mpsc::channel(16);
    let _handle = spawn(config, tx, fast());
    let mut ws = accept(&listener).await;
    handshake(&mut ws, true).await.expect("accepted");
    assert_eq!(rx.recv().await, Some(Action::RtConnected));
    ws.close(None).await.unwrap();
    assert_eq!(rx.recv().await, Some(Action::RtDisconnected));
    // And it reconnects.
    let mut again = accept(&listener).await;
    handshake(&mut again, true).await.expect("accepted");
    assert_eq!(rx.recv().await, Some(Action::RtConnected));
}

#[test]
fn backoff_doubles_up_to_the_ceiling() {
    let base = Duration::from_secs(1);
    let max = Duration::from_secs(30);
    let waits: Vec<u64> = (0..8)
        .map(|n| backoff_delay(n, base, max, 1.0).as_secs())
        .collect();
    assert_eq!(waits, [1, 2, 4, 8, 16, 30, 30, 30]);
    assert_eq!(backoff_delay(1000, base, max, 1.0), max);
    // Jitter only shortens, never past half.
    assert_eq!(backoff_delay(3, base, max, 0.5), Duration::from_secs(4));
    assert!(jitter_unit() >= 0.5 && jitter_unit() <= 1.0);
}

/// Connections that fail at once are retried after 1, 2, 4 x base (here 100, 200, 400 ms).
#[tokio::test]
async fn retries_follow_the_backoff_sequence() {
    let (listener, config) = listener().await;
    let (tx, mut rx) = mpsc::channel(64);
    let _handle = spawn(config, tx, fast());
    let mut starts = Vec::new();
    for _ in 0..5 {
        let (stream, _) = listener.accept().await.unwrap();
        starts.push(Instant::now());
        drop(stream);
        assert_eq!(rx.recv().await, Some(Action::RtDisconnected));
    }
    // Each gap in whole base units (100 ms), rounded down; scheduling only ever adds a little.
    let gaps: Vec<u128> = starts
        .windows(2)
        .map(|w| (w[1] - w[0]).as_millis() / 100)
        .collect();
    assert_eq!(
        gaps,
        [1, 2, 4, 4],
        "the fourth wait is held at the 400 ms ceiling"
    );
}

#[tokio::test]
async fn a_stable_connection_resets_the_backoff() {
    let (listener, config) = listener().await;
    let (tx, mut rx) = mpsc::channel(64);
    let timing = Timing {
        stable_after: Duration::from_millis(150),
        backoff_max: Duration::from_secs(10),
        ..fast()
    };
    let _handle = spawn(config, tx, timing);
    // Two quick failures raise the next wait to 400 ms...
    for _ in 0..2 {
        let (stream, _) = listener.accept().await.unwrap();
        drop(stream);
        assert_eq!(rx.recv().await, Some(Action::RtDisconnected));
    }
    // ...then a connection that stays up past `stable_after` and drops.
    let mut ws = accept(&listener).await;
    handshake(&mut ws, true).await.expect("accepted");
    assert_eq!(rx.recv().await, Some(Action::RtConnected));
    tokio::time::sleep(Duration::from_millis(250)).await;
    ws.close(None).await.unwrap();
    assert_eq!(rx.recv().await, Some(Action::RtDisconnected));
    let dropped_at = Instant::now();
    let (stream, _) = listener.accept().await.unwrap();
    let waited = dropped_at.elapsed();
    assert!(waited >= Duration::from_millis(100), "{waited:?}");
    assert!(
        waited < Duration::from_millis(350),
        "back to the base wait, not 400 ms: {waited:?}"
    );
    drop(stream);
}

#[tokio::test]
async fn a_rejected_login_stops_the_retries() {
    let (listener, config) = listener().await;
    let (tx, mut rx) = mpsc::channel(16);
    let _handle = spawn(config, tx, fast());
    let mut ws = accept(&listener).await;
    assert!(handshake(&mut ws, false).await.is_none());
    assert_eq!(rx.recv().await, Some(Action::RtRejected));
    drop(ws);
    // No second connection, however long we wait.
    let second = tokio::time::timeout(Duration::from_millis(800), listener.accept()).await;
    assert!(second.is_err(), "the task must not retry a rejected token");
}

#[tokio::test]
async fn dropping_the_handle_stops_the_task_and_its_socket() {
    let (listener, config) = listener().await;
    let (tx, mut rx) = mpsc::channel(16);
    let handle = spawn(config, tx, fast());
    let mut ws = accept(&listener).await;
    handshake(&mut ws, true).await.expect("accepted");
    assert_eq!(rx.recv().await, Some(Action::RtConnected));
    drop(handle);
    // The socket closes and nothing reconnects.
    while let Some(Ok(frame)) = ws.next().await {
        if frame.is_close() {
            break;
        }
    }
    let again = tokio::time::timeout(Duration::from_millis(800), listener.accept()).await;
    assert!(again.is_err());
    assert!(rx.recv().await.is_none(), "the task's sender is gone");
}

#[tokio::test]
async fn reconnect_restarts_the_task_and_leaves_one_connection() {
    use crate::app::{App, ConnState};
    let (listener, config) = listener().await;
    let (tx, mut rx) = mpsc::channel(16);
    let timing = Timing {
        silence: Duration::from_secs(10),
        ..fast()
    };
    let mut supervisor = Supervisor::new(Some(config), tx, timing);
    let mut app = App::new(ratatui::layout::Size::new(80, 24));

    // Nothing starts before the rooms load.
    app.conn = ConnState::Connecting;
    supervisor.sync(&mut app);
    assert!(!supervisor.is_running());

    app.conn = ConnState::Ready;
    supervisor.sync(&mut app);
    let mut first = accept(&listener).await;
    handshake(&mut first, true).await.expect("accepted");
    assert_eq!(rx.recv().await, Some(Action::RtConnected));
    // A further turn with nothing asked changes nothing.
    supervisor.sync(&mut app);
    assert!(
        tokio::time::timeout(Duration::from_millis(300), listener.accept())
            .await
            .is_err()
    );

    // `:reconnect`: the old socket is closed and exactly one new one appears.
    app.restart_realtime = true;
    supervisor.sync(&mut app);
    assert!(!app.restart_realtime);
    let mut second = accept(&listener).await;
    handshake(&mut second, true).await.expect("accepted");
    assert_eq!(rx.recv().await, Some(Action::RtConnected));
    while let Some(Ok(frame)) = first.next().await {
        if frame.is_close() {
            break;
        }
    }
    assert!(supervisor.is_running());
    assert!(
        tokio::time::timeout(Duration::from_millis(500), listener.accept())
            .await
            .is_err(),
        "no third connection"
    );
    // Only the new connection delivers.
    send_json(&mut second, message_event("m1", BODY)).await;
    assert!(matches!(rx.recv().await, Some(Action::RtMessage(_))));
    drop(supervisor);
    assert!(rx.recv().await.is_none(), "quitting stops the task");
}

// ---- per-room delete subscriptions

fn set(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

/// Read the next `n` client frames after the handshake and keep the ones that are per-room
/// `sub` or `unsub` frames, as `(msg, id, params)`.
async fn read_room_frames(ws: &mut Server, n: usize) -> Vec<(String, String, Value)> {
    let mut out = Vec::new();
    for _ in 0..n {
        let frame = read_json(ws).await;
        out.push((
            frame["msg"].as_str().unwrap_or_default().to_string(),
            frame["id"].as_str().unwrap_or_default().to_string(),
            frame["params"].clone(),
        ));
    }
    out
}

fn delete_params(rid: &str) -> Value {
    json!([format!("{rid}/deleteMessage"), false])
}

#[test]
fn wanted_rooms_are_the_joined_rooms_that_can_show_messages() {
    use crate::data::{Room, RoomKind};
    let room = |id: &str, encrypted: bool| Room {
        id: RoomId(id.to_string()),
        name: id.to_string(),
        kind: RoomKind::Channel,
        encrypted,
        private: false,
        unread: false,
        last_seen: None,
        last_activity: 0,
    };
    let rooms = [room("b", false), room("secret", true), room("a", false)];
    assert_eq!(wanted_rooms(&rooms), set(&["a", "b"]));
    assert!(wanted_rooms(&[]).is_empty());
}

#[tokio::test]
async fn the_wanted_set_is_published_only_when_it_changes() {
    let (_listener, config) = listener().await;
    let (tx, _rx) = mpsc::channel(16);
    let handle = spawn(config, tx, fast());
    assert!(!handle.set_rooms(set(&[])), "the empty set is the start");
    assert!(handle.set_rooms(set(&["a", "b"])));
    assert!(!handle.set_rooms(set(&["b", "a"])), "the same set again");
    assert!(handle.set_rooms(set(&["a"])));
    assert!(handle.set_rooms(set(&[])));
}

#[tokio::test]
async fn every_wanted_room_is_subscribed_after_login() {
    let (listener, config) = listener().await;
    let (tx, mut rx) = mpsc::channel(16);
    let handle = spawn(config, tx, fast());
    assert!(handle.set_rooms(set(&["r2", "r1"])));
    let mut ws = accept(&listener).await;
    let (_, subs) = handshake(&mut ws, true).await.expect("accepted");
    assert_eq!(subs.len(), 2, "the two fixed streams come first");
    let frames = read_room_frames(&mut ws, 2).await;
    let params: Vec<&Value> = frames.iter().map(|(_, _, p)| p).collect();
    assert_eq!(params, [&delete_params("r1"), &delete_params("r2")]);
    for (msg, id, _) in &frames {
        assert_eq!(msg, "sub");
        assert!(!id.is_empty());
    }
    assert_ne!(frames[0].1, frames[1].1, "each subscription has its own id");
    for frame in [&subs[0], &subs[1]] {
        assert_ne!(frames[0].1, frame["id"].as_str().unwrap());
    }
    assert_eq!(rx.recv().await, Some(Action::RtConnected));
}

#[tokio::test]
async fn a_room_added_later_is_subscribed_and_a_removed_one_unsubscribed() {
    let (listener, config) = listener().await;
    let (tx, mut rx) = mpsc::channel(16);
    let handle = spawn(config, tx, fast());
    assert!(handle.set_rooms(set(&["r1", "r2"])));
    let mut ws = accept(&listener).await;
    handshake(&mut ws, true).await.expect("accepted");
    let initial = read_room_frames(&mut ws, 2).await;
    assert_eq!(rx.recv().await, Some(Action::RtConnected));

    // A room joined while connected.
    assert!(handle.set_rooms(set(&["r1", "r2", "r3"])));
    let added = read_room_frames(&mut ws, 1).await;
    assert_eq!(added[0].0, "sub");
    assert_eq!(added[0].2, delete_params("r3"));

    // A room left while connected: the unsub names the subscription started for it.
    assert!(handle.set_rooms(set(&["r1", "r3"])));
    let removed = read_room_frames(&mut ws, 1).await;
    assert_eq!(removed[0].0, "unsub");
    assert_eq!(removed[0].1, initial[1].1, "r2 was the second subscription");

    // An unchanged set sends nothing: the next frame is the answer to a ping.
    assert!(!handle.set_rooms(set(&["r3", "r1"])));
    send_json(&mut ws, json!({"msg": "ping", "id": "p1"})).await;
    assert_eq!(read_json(&mut ws).await, json!({"msg": "pong", "id": "p1"}));
}

#[tokio::test]
async fn every_room_is_subscribed_again_after_a_reconnect() {
    let (listener, config) = listener().await;
    let (tx, mut rx) = mpsc::channel(16);
    let handle = spawn(config, tx, fast());
    assert!(handle.set_rooms(set(&["r1", "r2"])));
    let mut first = accept(&listener).await;
    handshake(&mut first, true).await.expect("accepted");
    read_room_frames(&mut first, 2).await;
    assert_eq!(rx.recv().await, Some(Action::RtConnected));
    // The set changes, then the connection drops.
    assert!(handle.set_rooms(set(&["r2", "r3"])));
    read_room_frames(&mut first, 2).await;
    first.close(None).await.unwrap();
    assert_eq!(rx.recv().await, Some(Action::RtDisconnected));

    let mut second = accept(&listener).await;
    handshake(&mut second, true).await.expect("accepted");
    let frames = read_room_frames(&mut second, 2).await;
    let params: Vec<&Value> = frames.iter().map(|(_, _, p)| p).collect();
    assert_eq!(params, [&delete_params("r2"), &delete_params("r3")]);
    assert!(frames.iter().all(|(msg, _, _)| msg == "sub"));
    assert_eq!(rx.recv().await, Some(Action::RtConnected));
}

#[tokio::test]
async fn a_delete_frame_reaches_the_reducer_as_rt_deleted() {
    let (listener, config) = listener().await;
    let (tx, mut rx) = mpsc::channel(16);
    let handle = spawn(config, tx, fast());
    assert!(handle.set_rooms(set(&["r1"])));
    let mut ws = accept(&listener).await;
    handshake(&mut ws, true).await.expect("accepted");
    read_room_frames(&mut ws, 1).await;
    assert_eq!(rx.recv().await, Some(Action::RtConnected));
    send_json(
        &mut ws,
        json!({
            "msg": "changed", "collection": "stream-notify-room", "id": "x",
            "fields": {"eventName": "r1/deleteMessage", "args": [{"_id": "m9"}]}
        }),
    )
    .await;
    assert_eq!(
        rx.recv().await,
        Some(Action::RtDeleted {
            room: RoomId("r1".to_string()),
            id: "m9".to_string()
        })
    );
}

#[tokio::test]
async fn the_supervisor_publishes_the_room_list_of_the_app() {
    use crate::app::{App, ConnState};
    use crate::data::{Room, RoomKind};
    let (listener, config) = listener().await;
    let (tx, mut rx) = mpsc::channel(16);
    let mut supervisor = Supervisor::new(Some(config), tx, fast());
    let mut app = App::new(ratatui::layout::Size::new(80, 24));
    let room = |id: &str, encrypted: bool| Room {
        id: RoomId(id.to_string()),
        name: id.to_string(),
        kind: RoomKind::Channel,
        encrypted,
        private: false,
        unread: false,
        last_seen: None,
        last_activity: 0,
    };
    app.rooms = vec![room("r1", false), room("secret", true)];
    // Nothing runs yet, so nothing is published and nothing panics.
    supervisor.sync_rooms(&app);
    app.conn = ConnState::Ready;
    supervisor.sync(&mut app);
    supervisor.sync_rooms(&app);
    let mut ws = accept(&listener).await;
    handshake(&mut ws, true).await.expect("accepted");
    let frames = read_room_frames(&mut ws, 1).await;
    assert_eq!(
        frames[0].2,
        delete_params("r1"),
        "the encrypted room is left out"
    );
    assert_eq!(rx.recv().await, Some(Action::RtConnected));
    app.rooms.push(room("r2", false));
    supervisor.sync_rooms(&app);
    let added = read_room_frames(&mut ws, 1).await;
    assert_eq!(added[0].2, delete_params("r2"));
}
