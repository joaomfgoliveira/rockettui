//! The realtime connection: a minimal DDP client over WebSocket (`realtime-ddp`).
//!
//! The task in here owns the socket, the clock and the backoff, and talks to the reducer only
//! through `Action`s on the one channel. It logs nothing: no frame, token or message text is
//! written anywhere.
//!
//! One connection ("session"): open the socket, DDP `connect`, `login {resume: <PAT>}`, subscribe
//! to the message stream and the user's subscription changes, send `Action::RtConnected`, then
//! read frames until the socket closes, errors or stays silent. `run` repeats sessions with
//! exponential backoff, and stops for good when the server rejects the login.

pub mod ddp;
pub mod events;

use crate::action::Action;
use crate::config::connection::{Connection, Secret};
use ddp::{LOGIN_ID, LoginFrame, ServerFrame};
use futures::{SinkExt, StreamExt};
use reqwest::Url;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::Message as WsMessage;

/// What the task needs to connect. `Debug` is safe: `Secret` redacts and the URL has no
/// credentials (a URL with a user name or password is rejected when the config loads).
#[derive(Clone, Debug)]
pub struct RtConfig {
    url: Url,
    user_id: Secret,
    token: Secret,
}

impl RtConfig {
    /// The realtime config of a connection; `None` when its URL cannot become a WebSocket URL.
    pub fn from_connection(conn: &Connection) -> Option<RtConfig> {
        Some(RtConfig {
            url: ddp::websocket_url(&conn.server_url)?,
            user_id: conn.user_id.clone(),
            token: conn.token.clone(),
        })
    }
}

/// The timings of the task. `Timing::default()` is what the app uses; tests shorten them.
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    /// A connection with no frame for this long is treated as dropped. The server pings about
    /// every 30 s, so 75 s is 2.5 intervals.
    pub silence: Duration,
    /// Limit for connecting, DDP connect, login and subscribing together.
    pub handshake: Duration,
    /// First wait after a drop; each failure doubles it.
    pub backoff_base: Duration,
    pub backoff_max: Duration,
    /// A connection that stayed live this long resets the backoff.
    pub stable_after: Duration,
    /// Randomise each wait (half to full); tests switch it off for exact sequences.
    pub jitter: bool,
}

impl Default for Timing {
    fn default() -> Timing {
        Timing {
            silence: Duration::from_secs(75),
            handshake: Duration::from_secs(15),
            backoff_base: Duration::from_secs(1),
            backoff_max: Duration::from_secs(30),
            stable_after: Duration::from_secs(60),
            jitter: true,
        }
    }
}

/// The wait before retry number `attempt` (0 is the first): `base * 2^attempt` up to `max`, then
/// scaled by `unit` in `0.5..=1.0` when jitter is on (`unit` is 1.0 otherwise).
pub fn backoff_delay(attempt: u32, base: Duration, max: Duration, unit: f64) -> Duration {
    let doubled = base.saturating_mul(2u32.saturating_pow(attempt.min(20)));
    doubled.min(max).mul_f64(unit.clamp(0.0, 1.0))
}

/// A uniform value in `0.5..=1.0` from the process's `RandomState` keys (no extra dependency).
fn jitter_unit() -> f64 {
    use std::hash::BuildHasher;
    let bits = std::hash::RandomState::new().hash_one(2u8) >> 11;
    0.5 + 0.5 * (bits as f64 / (1u64 << 53) as f64)
}

/// A request to send one more `sub` frame on the live connection: the seam for streams a later
/// change needs (for example a per-room stream). The connection forgets it when it drops, so
/// the owner sends it again on every `Action::RtConnected`.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtraSub {
    pub name: String,
    pub params: Vec<serde_json::Value>,
}

/// The ids of the rooms whose delete notifications the app wants: every joined room that can show
/// messages (an encrypted room never loads history, so it has nothing to delete from). Derived
/// from `App.rooms`, so there is no Action or Effect for it.
pub fn wanted_rooms(rooms: &[crate::data::Room]) -> BTreeSet<String> {
    rooms
        .iter()
        .filter(|room| !room.encrypted)
        .map(|room| room.id.0.clone())
        .collect()
}

/// Owns the running task. Dropping the handle stops the task and closes its socket, so
/// `:reconnect` replaces the handle and quitting drops it.
pub struct RtHandle {
    task: JoinHandle<()>,
    extra: Sender<ExtraSub>,
    rooms: watch::Sender<BTreeSet<String>>,
}

impl RtHandle {
    /// Publish the wanted room set to the task, only when it differs from the last one published
    /// (`true` then). The task subscribes to rooms that were added and unsubscribes from the ones
    /// that were removed, and after every reconnect it subscribes to the whole current set.
    pub fn set_rooms(&self, wanted: BTreeSet<String>) -> bool {
        self.rooms.send_if_modified(|current| {
            if *current == wanted {
                return false;
            }
            *current = wanted;
            true
        })
    }

    /// Ask the live connection to subscribe to one more stream. `false` when the task is gone.
    pub async fn subscribe(&self, sub: ExtraSub) -> bool {
        self.extra.send(sub).await.is_ok()
    }
}

impl Drop for RtHandle {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Decides, once per turn of the run loop, whether the realtime task must start or restart. It
/// is the only owner of the `RtHandle`: starting a new task drops the old handle first (which
/// aborts the old task and closes its socket), so at most one connection is ever live, and
/// dropping the supervisor at quit stops the task.
pub struct Supervisor {
    config: Option<RtConfig>,
    tx: Sender<Action>,
    timing: Timing,
    handle: Option<RtHandle>,
    started: bool,
}

impl Supervisor {
    /// `config` is `None` when the app has no connection (nothing to start).
    pub fn new(config: Option<RtConfig>, tx: Sender<Action>, timing: Timing) -> Supervisor {
        Supervisor {
            config,
            tx,
            timing,
            handle: None,
            started: false,
        }
    }

    /// Start the task once the rooms have loaded, and restart it when `:reconnect` asked
    /// (`App.restart_realtime`, cleared here).
    pub fn sync(&mut self, app: &mut crate::app::App) {
        let restart = std::mem::take(&mut app.restart_realtime);
        let first = !self.started && app.conn == crate::app::ConnState::Ready;
        if !(restart || first) {
            return;
        }
        self.started = true;
        // Drop the old task before the new one exists: never two sockets at once.
        self.handle = None;
        self.handle = self
            .config
            .clone()
            .map(|config| spawn(config, self.tx.clone(), self.timing));
    }

    /// Tell the running task which rooms' deletes to follow; a no-op when the set did not change.
    /// Called after every reducer batch, so the set follows `App.rooms` without an Action.
    pub fn sync_rooms(&mut self, app: &crate::app::App) {
        if let Some(handle) = &self.handle {
            handle.set_rooms(wanted_rooms(&app.rooms));
        }
    }

    /// Whether a task is running (for tests).
    #[cfg(test)]
    fn is_running(&self) -> bool {
        self.handle.as_ref().is_some_and(|h| !h.task.is_finished())
    }
}

/// Start the realtime task. Events go to `tx` as `Action`s.
pub fn spawn(config: RtConfig, tx: Sender<Action>, timing: Timing) -> RtHandle {
    let (extra, extra_rx) = mpsc::channel(8);
    let (rooms, rooms_rx) = watch::channel(BTreeSet::new());
    let task = tokio::spawn(run(config, tx, extra_rx, rooms_rx, timing));
    RtHandle { task, extra, rooms }
}

/// How one session ended.
enum End {
    /// The socket closed, errored, went silent or never finished its handshake. `live_for` is how
    /// long the connection was live (`None` when it never got there).
    Dropped { live_for: Option<Duration> },
    /// The server rejected the login.
    Rejected,
    /// The reducer's channel is closed: the app is gone.
    Closed,
}

async fn run(
    config: RtConfig,
    tx: Sender<Action>,
    mut extra: Receiver<ExtraSub>,
    rooms: watch::Receiver<BTreeSet<String>>,
    timing: Timing,
) {
    let mut attempt: u32 = 0;
    loop {
        match session(&config, &tx, &mut extra, &rooms, &timing).await {
            End::Closed => return,
            End::Rejected => {
                // Retrying a rejected token would only hammer the server: wait for `:reconnect`.
                let _ = tx.send(Action::RtRejected).await;
                return;
            }
            End::Dropped { live_for } => {
                if live_for.is_some_and(|d| d >= timing.stable_after) {
                    attempt = 0;
                }
            }
        }
        if tx.send(Action::RtDisconnected).await.is_err() {
            return;
        }
        let unit = if timing.jitter { jitter_unit() } else { 1.0 };
        tokio::time::sleep(backoff_delay(
            attempt,
            timing.backoff_base,
            timing.backoff_max,
            unit,
        ))
        .await;
        attempt = attempt.saturating_add(1);
    }
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// A socket that reads DDP frames and answers heartbeats, whichever step is waiting. Once `live`,
/// it also sends the extra `sub` frames requested through `RtHandle::subscribe`; both happen
/// inside `next`, so a heartbeat is never lost to a cancelled read.
struct Conn<'a> {
    ws: Socket,
    silence: Duration,
    extra: &'a mut Receiver<ExtraSub>,
    /// The rooms whose delete notifications are wanted, and the subscription id held for each room
    /// this connection has subscribed to. A new connection starts with none held.
    rooms: watch::Receiver<BTreeSet<String>>,
    room_subs: BTreeMap<String, String>,
    live: bool,
    sub_seq: u32,
    /// Frames that could not be parsed and were skipped. Only a count: nothing of a frame is kept.
    skipped: u32,
}

impl Conn<'_> {
    async fn send(&mut self, text: &str) -> Result<(), ()> {
        self.ws.send(WsMessage::text(text)).await.map_err(|_| ())
    }

    /// Bring the per-room delete subscriptions in line with the wanted set: a `sub` for each room
    /// not held yet, an `unsub` for each held room no longer wanted.
    async fn sync_rooms(&mut self) -> Result<(), ()> {
        let wanted = self.rooms.borrow_and_update().clone();
        let removed: Vec<String> = self
            .room_subs
            .keys()
            .filter(|rid| !wanted.contains(*rid))
            .cloned()
            .collect();
        for rid in removed {
            if let Some(id) = self.room_subs.remove(&rid) {
                self.send(&ddp::unsub_frame(&id)).await?;
            }
        }
        for rid in wanted {
            if self.room_subs.contains_key(&rid) {
                continue;
            }
            self.sub_seq += 1;
            let id = format!("sub-del-{}", self.sub_seq);
            let frame = ddp::sub_frame(
                &id,
                events::ROOM_STREAM,
                &[
                    json!(format!("{rid}{}", events::DELETE_SUFFIX)),
                    json!(false),
                ],
            );
            self.send(&frame).await?;
            self.room_subs.insert(rid, id);
        }
        Ok(())
    }

    /// The next frame this client acts on. A DDP `ping` is answered here and never returned.
    /// `Err` when the socket closed or failed, or nothing arrived within `silence`.
    async fn next(&mut self) -> Result<ServerFrame, ()> {
        loop {
            let wanted = async {
                if self.live {
                    self.extra.recv().await
                } else {
                    std::future::pending().await
                }
            };
            let rooms_changed = async {
                if self.live {
                    self.rooms.changed().await
                } else {
                    std::future::pending().await
                }
            };
            let item = tokio::select! {
                item = tokio::time::timeout(self.silence, self.ws.next()) => {
                    item.map_err(|_| ())?
                }
                Ok(()) = rooms_changed => {
                    self.sync_rooms().await?;
                    continue;
                }
                Some(sub) = wanted => {
                    self.sub_seq += 1;
                    let frame = ddp::sub_frame(
                        &format!("sub-extra-{}", self.sub_seq),
                        &sub.name,
                        &sub.params,
                    );
                    self.send(&frame).await?;
                    continue;
                }
            };
            let text = match item {
                Some(Ok(WsMessage::Text(text))) => text,
                // WebSocket-level ping/pong are answered by the library; binary is not used.
                Some(Ok(WsMessage::Close(_))) | Some(Err(_)) | None => return Err(()),
                Some(Ok(_)) => continue,
            };
            match ddp::parse(text.as_str()) {
                Ok(ServerFrame::Ping(id)) => self.send(&ddp::pong_frame(id.as_deref())).await?,
                Ok(frame) => return Ok(frame),
                Err(_) => self.skipped = self.skipped.saturating_add(1),
            }
        }
    }

    /// Frames until one satisfies `want`; anything else is ignored.
    async fn wait_for<T>(
        &mut self,
        mut want: impl FnMut(&ServerFrame) -> Option<T>,
    ) -> Result<T, ()> {
        loop {
            let frame = self.next().await?;
            if let Some(found) = want(&frame) {
                return Ok(found);
            }
        }
    }
}

/// Connect, log in and subscribe. `Ok(Err(()))` is a rejected login; `Err(())` a failed handshake.
async fn handshake<'a>(
    config: &RtConfig,
    ws: Socket,
    silence: Duration,
    extra: &'a mut Receiver<ExtraSub>,
    rooms: watch::Receiver<BTreeSet<String>>,
) -> Result<Result<Conn<'a>, ()>, ()> {
    let mut conn = Conn {
        ws,
        silence,
        extra,
        rooms,
        room_subs: BTreeMap::new(),
        live: false,
        sub_seq: 0,
        skipped: 0,
    };
    conn.send(&ddp::connect_frame()).await?;
    conn.wait_for(|f| match f {
        ServerFrame::Connected => Some(Ok(())),
        ServerFrame::Failed => Some(Err(())),
        _ => None,
    })
    .await??;
    conn.send(LoginFrame::new(&config.token).as_str()).await?;
    let accepted = conn
        .wait_for(|f| match f {
            ServerFrame::Result { id, ok } if id == LOGIN_ID => Some(*ok),
            _ => None,
        })
        .await?;
    if !accepted {
        return Ok(Err(()));
    }
    let subs = [
        ddp::sub_frame(
            "sub-messages",
            events::MESSAGES_STREAM,
            &[json!(events::MESSAGES_EVENT), json!(false)],
        ),
        ddp::sub_frame(
            "sub-subscriptions",
            events::USER_STREAM,
            &[
                json!(format!("{}/subscriptions-changed", config.user_id.expose())),
                json!(false),
            ],
        ),
    ];
    for frame in &subs {
        conn.send(frame).await?;
    }
    // The delete notifications of every wanted room, after every connect.
    conn.sync_rooms().await?;
    Ok(Ok(conn))
}

async fn session(
    config: &RtConfig,
    tx: &Sender<Action>,
    extra: &mut Receiver<ExtraSub>,
    rooms: &watch::Receiver<BTreeSet<String>>,
    timing: &Timing,
) -> End {
    let established = tokio::time::timeout(timing.handshake, async {
        let (ws, _) = tokio_tungstenite::connect_async(config.url.as_str())
            .await
            .map_err(|_| ())?;
        handshake(config, ws, timing.silence, &mut *extra, rooms.clone()).await
    })
    .await;
    let mut conn = match established {
        Ok(Ok(Ok(conn))) => conn,
        Ok(Ok(Err(()))) => return End::Rejected,
        Ok(Err(())) | Err(_) => return End::Dropped { live_for: None },
    };
    if tx.send(Action::RtConnected).await.is_err() {
        return End::Closed;
    }
    conn.live = true;
    let live_since = Instant::now();
    loop {
        let Ok(frame) = conn.next().await else {
            return End::Dropped {
                live_for: Some(live_since.elapsed()),
            };
        };
        if let Some(action) = events::action_for(frame)
            && tx.send(action).await.is_err()
        {
            return End::Closed;
        }
    }
}

#[cfg(test)]
mod tests;
