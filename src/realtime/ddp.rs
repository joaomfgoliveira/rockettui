//! The DDP frames the client uses, and nothing else (a minimal client, no generic DDP crate).
//!
//! Parsing is tolerant: an unknown `msg` becomes `ServerFrame::Other`, and unknown fields are
//! ignored. A frame that is not valid JSON or has no `msg` is a `FrameError`, a fixed class with
//! no payload. Frames can hold message text and the login frame holds the token, so none of the
//! types here derives `Debug` over its content: the output names the variant only.

use crate::config::connection::Secret;
use reqwest::Url;
use serde_json::{Value, json};

/// A frame that could not be understood. Carries no text from the frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameError;

/// One frame from the server. Hand-written `Debug`: `Changed` holds message content.
#[derive(Clone, PartialEq)]
pub enum ServerFrame {
    /// The server accepted `connect`.
    Connected,
    /// The server refused the protocol version.
    Failed,
    /// A heartbeat; the optional id is echoed in the `pong`.
    Ping(Option<String>),
    Pong,
    /// The answer to a `method` call. `ok` is false when it carries an `error`.
    Result {
        id: String,
        ok: bool,
    },
    /// Subscriptions whose initial data is complete.
    Ready(Vec<String>),
    /// A subscription the server refused or ended.
    Nosub(String),
    /// A stream event: `collection` is the stream name, `event` its `eventName`, `args` the
    /// payload.
    Changed {
        collection: String,
        event: String,
        args: Vec<Value>,
    },
    /// A frame this client does not use (`added`, `updated`, `server_id`, ...).
    Other,
}

impl std::fmt::Debug for ServerFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServerFrame::Connected => write!(f, "Connected"),
            ServerFrame::Failed => write!(f, "Failed"),
            ServerFrame::Ping(_) => write!(f, "Ping"),
            ServerFrame::Pong => write!(f, "Pong"),
            ServerFrame::Result { ok, .. } => write!(f, "Result(ok={ok})"),
            ServerFrame::Ready(subs) => write!(f, "Ready(<{} subs>)", subs.len()),
            ServerFrame::Nosub(_) => write!(f, "Nosub"),
            ServerFrame::Changed { args, .. } => write!(f, "Changed(<{} args>)", args.len()),
            ServerFrame::Other => write!(f, "Other"),
        }
    }
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

/// Parse one text frame.
pub fn parse(text: &str) -> Result<ServerFrame, FrameError> {
    let value: Value = serde_json::from_str(text).map_err(|_| FrameError)?;
    let msg = value.get("msg").and_then(Value::as_str).ok_or(FrameError)?;
    Ok(match msg {
        "connected" => ServerFrame::Connected,
        "failed" => ServerFrame::Failed,
        "ping" => ServerFrame::Ping(string_field(&value, "id")),
        "pong" => ServerFrame::Pong,
        "result" => ServerFrame::Result {
            id: string_field(&value, "id").unwrap_or_default(),
            ok: value.get("error").is_none() && value.get("result").is_some(),
        },
        "ready" => ServerFrame::Ready(
            value
                .get("subs")
                .and_then(Value::as_array)
                .map(|subs| {
                    subs.iter()
                        .filter_map(|s| s.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
        ),
        "nosub" => ServerFrame::Nosub(string_field(&value, "id").unwrap_or_default()),
        "changed" => {
            let fields = value.get("fields");
            ServerFrame::Changed {
                collection: string_field(&value, "collection").unwrap_or_default(),
                event: fields
                    .and_then(|f| string_field(f, "eventName"))
                    .unwrap_or_default(),
                args: fields
                    .and_then(|f| f.get("args"))
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            }
        }
        _ => ServerFrame::Other,
    })
}

/// The `connect` frame that opens a DDP session.
pub fn connect_frame() -> String {
    json!({"msg": "connect", "version": "1", "support": ["1", "pre2", "pre1"]}).to_string()
}

/// The answer to a server `ping`, echoing its id when it had one.
pub fn pong_frame(id: Option<&str>) -> String {
    match id {
        Some(id) => json!({"msg": "pong", "id": id}).to_string(),
        None => json!({"msg": "pong"}).to_string(),
    }
}

/// A `sub` frame for stream `name` with `params`.
pub fn sub_frame(id: &str, name: &str, params: &[Value]) -> String {
    json!({"msg": "sub", "id": id, "name": name, "params": params}).to_string()
}

/// The id of the login `method` call, so the answer can be told from other results.
pub const LOGIN_ID: &str = "login";

/// The login frame: `method login [{resume: <token>}]`. This is the one place the token is put
/// into a frame. `Debug` prints no text, so the token cannot reach a log through it.
pub struct LoginFrame(String);

impl LoginFrame {
    pub fn new(token: &Secret) -> LoginFrame {
        LoginFrame(
            json!({
                "msg": "method",
                "method": "login",
                "params": [{"resume": token.expose()}],
                "id": LOGIN_ID,
            })
            .to_string(),
        )
    }

    /// The frame text, to hand to the socket and nowhere else.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for LoginFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LoginFrame(<redacted>)")
    }
}

/// The WebSocket URL of a server: `http` becomes `ws`, `https` becomes `wss`, and `websocket` is
/// appended to the path, so a server installed under a sub-path keeps it.
pub fn websocket_url(server: &Url) -> Option<Url> {
    let mut url = server.clone();
    let scheme = if server.scheme() == "https" {
        "wss"
    } else {
        "ws"
    };
    url.set_scheme(scheme).ok()?;
    url.path_segments_mut()
        .ok()?
        .pop_if_empty()
        .push("websocket");
    Some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_frames_the_probe_saw() {
        assert_eq!(
            parse(r#"{"msg":"connected","session":"s"}"#),
            Ok(ServerFrame::Connected)
        );
        assert_eq!(parse(r#"{"msg":"ping"}"#), Ok(ServerFrame::Ping(None)));
        assert_eq!(
            parse(r#"{"msg":"ping","id":"7"}"#),
            Ok(ServerFrame::Ping(Some("7".to_string())))
        );
        assert_eq!(parse(r#"{"msg":"pong"}"#), Ok(ServerFrame::Pong));
        assert_eq!(
            parse(
                r#"{"msg":"result","id":"login","result":{"id":"u1","token":"t","tokenExpires":{"$date":1}}}"#
            ),
            Ok(ServerFrame::Result {
                id: "login".to_string(),
                ok: true
            })
        );
        assert_eq!(
            parse(
                r#"{"msg":"result","id":"login","error":{"error":403,"reason":"User not found"}}"#
            ),
            Ok(ServerFrame::Result {
                id: "login".to_string(),
                ok: false
            })
        );
        assert_eq!(
            parse(r#"{"msg":"ready","subs":["a","b"]}"#),
            Ok(ServerFrame::Ready(vec!["a".to_string(), "b".to_string()]))
        );
        assert_eq!(
            parse(r#"{"msg":"nosub","id":"a"}"#),
            Ok(ServerFrame::Nosub("a".to_string()))
        );
        let changed = parse(
            r#"{"msg":"changed","collection":"stream-room-messages","id":"id","fields":{"eventName":"__my_messages__","args":[{"_id":"m1"}]}}"#,
        );
        assert_eq!(
            changed,
            Ok(ServerFrame::Changed {
                collection: "stream-room-messages".to_string(),
                event: "__my_messages__".to_string(),
                args: vec![json!({"_id": "m1"})],
            })
        );
    }

    #[test]
    fn unknown_frames_are_skipped_and_garbage_is_a_fixed_error() {
        assert_eq!(
            parse(r#"{"msg":"added","collection":"x"}"#),
            Ok(ServerFrame::Other)
        );
        assert_eq!(
            parse(r#"{"msg":"result","id":"1","extra":true}"#).map(|_| ()),
            Ok(())
        );
        assert_eq!(parse("not json"), Err(FrameError));
        assert_eq!(parse(r#"{"no":"msg"}"#), Err(FrameError));
        assert_eq!(parse(r#"["msg"]"#), Err(FrameError));
    }

    #[test]
    fn debug_never_prints_frame_content() {
        let frame = parse(
            r#"{"msg":"changed","collection":"c","fields":{"eventName":"e","args":[{"msg":"SENTINEL-BODY-7f3a"}]}}"#,
        )
        .unwrap();
        assert_eq!(format!("{frame:?}"), "Changed(<1 args>)");
    }

    #[test]
    fn login_frame_carries_the_token_but_debug_does_not() {
        let token = Secret::new("SENTINEL-TOKEN-9c41".to_string());
        let frame = LoginFrame::new(&token);
        let value: Value = serde_json::from_str(frame.as_str()).unwrap();
        assert_eq!(value["method"], "login");
        assert_eq!(value["params"][0]["resume"], "SENTINEL-TOKEN-9c41");
        assert!(!format!("{frame:?}").contains("SENTINEL-TOKEN"));
    }

    #[test]
    fn websocket_urls() {
        let url = |s: &str| websocket_url(&Url::parse(s).unwrap()).unwrap().to_string();
        assert_eq!(
            url("http://127.0.0.1:3100"),
            "ws://127.0.0.1:3100/websocket"
        );
        assert_eq!(
            url("https://chat.example.com"),
            "wss://chat.example.com/websocket"
        );
        assert_eq!(
            url("https://chat.example.com/"),
            "wss://chat.example.com/websocket"
        );
        assert_eq!(
            url("https://example.com/chat/"),
            "wss://example.com/chat/websocket"
        );
        assert_eq!(
            url("https://example.com/chat"),
            "wss://example.com/chat/websocket"
        );
    }

    #[test]
    fn client_frames() {
        let v: Value = serde_json::from_str(&sub_frame(
            "s1",
            "stream-room-messages",
            &[json!("__my_messages__"), json!(false)],
        ))
        .unwrap();
        assert_eq!(v["msg"], "sub");
        assert_eq!(v["params"][1], false);
        assert_eq!(
            serde_json::from_str::<Value>(&pong_frame(Some("7"))).unwrap()["id"],
            "7"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&pong_frame(None)).unwrap(),
            json!({"msg":"pong"})
        );
        assert_eq!(
            serde_json::from_str::<Value>(&connect_frame()).unwrap()["msg"],
            "connect"
        );
    }
}
