# Phase 4: Send Messages - Pattern Map

**Mapped:** 2026-11-17
**Files analyzed:** 22 new/modified (incl. tests, harness)
**Analogs found:** 22 / 22 (one new module, `app/send.rs`, has only role-match analogs)

All analog paths below are git-tracked (`git ls-files src scripts dev` confirmed). Decisions D-15/D-16/D-17 override D-03/D-04/D-07: endpoint is `chat.sendMessage` with `{"message":{"_id","rid","msg"}}`; a Failed/Unconfirmed message does NOT block later queued messages (one in flight per room, queue continues past failures); own username comes from one `GET /api/v1/me` per connection, kept in `App`, fallback `LOCAL_AUTHOR`.

Note for the planner: RESEARCH Pattern 3 sketches head-of-line blocking. D-16 reverses that. `pump` must pick the first `Queued` message and must NOT stop at a `Failed`/`Unconfirmed` one; it still returns early if any message in the room is `Sending`.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match |
|---|---|---|---|---|
| `src/app/send.rs` (NEW) | reducer module (state machine) | event-driven (effect outbox) | `src/app/history.rs` | role-match (same shape: pure fns + `app.effects.push`) |
| `src/app/mod.rs` | model | state | itself (`App` fields, `open_room`) | exact |
| `src/app/update.rs` | reducer | event-driven | itself (`submit_input`, `HistoryLoaded` arms) | exact |
| `src/app/messages.rs` | store helper | CRUD (in-memory) | itself (`follow_newest`, `view`, `row_starts`) | exact |
| `src/app/history.rs` | reducer module | merge | itself (id-dedupe in `on_loaded`) | exact |
| `src/app/command.rs` | command | request-response | itself (`cmd_reconnect`) | exact |
| `src/action.rs` | enum + redacting Debug | event | itself (`HistoryLoaded`/`HistoryFailed`) | exact |
| `src/effect.rs` | effect runner | request-response | itself (`LoadHistory` arm) | exact |
| `src/api/mod.rs` | HTTP client | request-response | itself (`get_json`, `load_history`) | exact |
| `src/api/wire.rs` | wire types | transform | itself (`HistoryResp`, `WireMsg`) | exact |
| `src/api/map.rs` | mapper | transform | itself (`history`, `kind_and_body`) | exact |
| `src/data/mod.rs` | model | state | itself (`MessageKind`, redacting `Debug`) | exact |
| `src/input/actions.rs` | vocabulary | config | itself (exhaustive `name_of`) | exact |
| `src/input/keymap.rs` | config (default rows) | config | itself (`default_bindings`) | exact |
| `src/config/settings.rs` | config | validation | itself (`message_layout` via `one_of`) | exact |
| `src/config/theme.rs` | config | validation | itself (`Markers`, `ELEMENT_PATHS`, `messages_notice`) | exact |
| `src/ui/wrap.rs` | view | transform | itself (`render_rows`, `time_column`) | exact |
| `src/ui/messages.rs`, `src/ui/input.rs` | view | render | existing snapshot tests in `src/ui/messages.rs` | exact |
| `src/lib.rs` | run loop | event-driven | itself (`Effect::LoadRooms` push, `app.zone`) | exact |
| `src/redaction.rs` | test | sweep | itself (`sentinel_sweep`, `ALL_ERRORS`) | exact |
| `scripts/smoke_pty.py` | harness | request-response | itself (`start_mock`, `scenario_docker_read`, `docker_env`) | exact |
| `dev/` probe script (NEW, Wave 0) | harness | request-response | `scripts/smoke_pty.py::docker_env` + `dev/seed.py` | role-match |

## Pattern Assignments

### `src/app/send.rs` (NEW; reducer module, event-driven)

**Analog:** `src/app/history.rs` (pure functions, `app.effects.push`, status error on failure).

**Module header + imports** (history.rs lines 1-21):
```rust
//! Per-room history state ...
//! The functions here are pure: they read and change `App` and push `Effect`s onto `app.effects`,
//! and never do IO. ...
use crate::api::ApiError;
use crate::app::messages::{self, MsgView};
use crate::app::{App, StatusMsg};
use crate::data::{Message, RoomId};
use crate::effect::Effect;
```

**Effect-push pattern** (history.rs 57-65):
```rust
let effect = Effect::LoadHistory { room: room.id.clone(), kind: room.kind, private: room.private,
    count: app.settings.history_size, before: None };
app.history.insert(id.clone(), HistoryState::Loading);
app.effects.push(effect);
```
Copy for `pump`: mark the chosen message `Sending`, then push `Effect::SendMessage`. Results are addressed by `(room, id)`, never `current_room`.

**Failure/status pattern** (history.rs 230-239):
```rust
app.status_msg = Some(StatusMsg::Error(error.to_string()));
```
For send, do NOT use `error.to_string()` (RateLimited/Server text says ":reconnect", conflicts with D-08). Use a separate fixed `send_error_text(error, unconfirmed)`; never the server error string.

**Submit seam to move out of update.rs** (update.rs 200-224), the current local append being replaced:
```rust
fn submit_input(app: &mut App) {
    if app.mode != Mode::Insert || app.input.text().trim().is_empty() { return; }
    let Some(room) = app.current_room.clone() else {
        app.status_msg = Some(StatusMsg::Error("No room open: select a room with Enter".into()));
        return;
    };
    let body = app.input.take();
    app.local_seq += 1;
    let message = Message { id: format!("local-{}", app.local_seq), room_id: room.clone(),
        author: LOCAL_AUTHOR.into(), ts: app.clock_secs, body, kind: MessageKind::Normal };
    app.messages.entry(room.clone()).or_default().push(message);
    messages::follow_newest(app, &room);
}
```
New `send::submit` keeps the guard order and the "No room open" text verbatim, adds the encrypted-room check (`app.rooms.iter().find(|r| r.id == room).is_some_and(|r| r.encrypted)` -> `StatusMsg::Error("encrypted, not supported")`, draft kept, same as the no-room branch which does not call `take()`), id = `format!("{}{}", app.id_prefix, app.local_seq)`, author = `app.own_username.clone().unwrap_or(LOCAL_AUTHOR.into())`, `send: SendState::Queued`, then follow policy, then `pump`.

**`pump` shape (D-16 version)**:
```rust
pub fn pump(app: &mut App, room: &RoomId) {
    let Some(msgs) = app.messages.get_mut(room) else { return };
    if msgs.iter().any(|m| m.send == SendState::Sending) { return; }          // one in flight per room
    let Some(next) = msgs.iter_mut().find(|m| m.send == SendState::Queued) else { return }; // skips Failed/Unconfirmed
    next.send = SendState::Sending;
    app.effects.push(Effect::SendMessage { room: room.clone(), id: next.id.clone(),
        text: SendText::new(next.body.clone()), reconcile: /* from reconcile_ids */ false });
}
```
Call `pump` after submit, `MessageSent`, `SendFailed`, `retry`, `discard`.

**Test pattern** (history.rs `first_open_issues_one_load`, tests mod from line 242): build `app_80x24()`, drive via `update(&mut app, Action::...)`, `assert_eq!(app.effects, [Effect::SendMessage{..}])` (needs `SendText: PartialEq`), `app.effects.clear()` between steps. Use `crate::test_util::{app_80x24, press, render_to_string}`.

---

### `src/data/mod.rs` (model, state)

**Analog:** itself. Add `SendState` beside `MessageKind` (lines 49-56) and a `send` field on `Message` (58-69).

`MessageKind` pattern to copy (lines 49-56):
```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MessageKind { #[default] Normal, System }
```
`SendState { #[default] Sent, Queued, Sending, Failed, Unconfirmed }` has no text so derived `Debug` is safe.

**Redacting Debug** (lines 71-80): add `.field("send", &self.send)` inside the hand-written `debug_struct`; keep `id`, `room_id`, `body_len` only.

**Mechanical churn:** every `Message { .. kind }` literal needs `send: SendState::Sent` (api/map.rs 217-224, update.rs 214-221, redaction.rs 94-103, data/fake.rs, data/mod.rs tests 99-106, action.rs tests ~197, ui/*, history tests). Do this first, compiler-guided.

---

### `src/app/mod.rs` (model, state)

**Analog:** itself.

Fields to add to `App` (struct lines 67-110, constructor 123-150), following `local_seq: u64` / `history: HashMap<..>` style:
- `drafts: HashMap<RoomId, input_buf::InputBuffer>` (D-10; `InputBuffer` is Clone + Default + redacting Debug, input_buf.rs 13-37)
- `id_prefix: String` (default `"local-"` so tests stay deterministic; set by `run()`)
- `own_username: Option<String>` (D-17)
- optionally `reconcile_ids: HashSet<String>`

**`open_room` to extend** (lines 207-211):
```rust
pub fn open_room(&mut self, id: RoomId) {
    self.current_room = Some(id);
    self.focus = Focus::Messages;
}
```
New body: if same room as current, leave draft alone; else stash `std::mem::take(&mut self.input)` under the old id (skip when empty), load `self.drafts.remove(&id).unwrap_or_default()`, then `let (w, h) = self.input_viewport(); self.input.settle(w, h);`. Single caller: `sidebar.rs` line 262 (`app.open_room(id.clone());` then `history::on_open`).

**`row_ctx`** (lines 172-185): raise `time_col` to the widest non-`Sent` marker so Compact bodies stay aligned (the only place a `RowCtx` is built).

**`LOCAL_AUTHOR`** (line 64) doc comment says "until Phase 4"; update, keep const as the fallback.

---

### `src/app/update.rs` (reducer)

**Analog:** itself, the `HistoryLoaded`/`HistoryFailed` arms (lines 65-73) and `SubmitInput` (line 163):
```rust
Action::HistoryLoaded { room, messages, oldest_ts, older } => history::on_loaded(app, room, messages, oldest_ts, older),
Action::HistoryFailed { room, older, error } => { history::on_failed(app, room, older, error); }
...
Action::SubmitInput => submit_input(app),
```
New arms delegate to `send::`: `SubmitInput => send::submit(app)`, `MessageSent{room,id,message} => send::on_sent(..)`, `SendFailed{room,id,error} => send::on_failed(..)`, `RetrySend => send::retry(app)`, `DiscardSend => send::discard(app)`, plus a `MeLoaded(String)` arm setting `app.own_username` (and retro-fixing `LOCAL_AUTHOR` pending rows' author). Delete `submit_input`; rewrite the test `submit_appends_local_message`.

Counted repeats (lines 29-41) re-dispatch non-motion actions n times: retry/discard must re-read the cursor message each call and be silent no-ops on non-failed messages. `Action::Key` clears `status_msg` first (line 20): the failed marker, not the status line, carries state.

---

### `src/app/messages.rs` (store helper, in-memory CRUD)

**Analog:** itself.

`follow_newest` (lines 154-158):
```rust
pub fn follow_newest(app: &mut App, room: &RoomId) { app.msg_views.remove(room); }
```
Add `push_unique` / `replace_by_id` (D-13: ignore when id present and `send == Sent`; replace when present is pending) and `is_at_bottom(app, room)` computed BEFORE the append using `row_starts` (28-40) and `viewport` (21-24):
```rust
let starts = row_starts(app, room);
let total = *starts.last().unwrap();
let h = usize::from(viewport(app).1);
// None entry in app.msg_views == default newest view
match app.msg_views.get(room) { None => true, Some(v) => v.top_row >= total.saturating_sub(h) }
```
`view` (68-85) shows the default-view rule (`None => (count - 1, total.saturating_sub(viewport_h))`). `MsgView.cursor` is a vector index: `discard` must decrement a stored cursor greater than the removed index (mirror the anchor logic in `history.rs::on_older_page`, lines 183-225, and its cursor tests).

---

### `src/app/history.rs` (merge)

**Analog:** itself. First-page merge already dedupes (lines 152-156 `.filter(|m| !have.contains(&m.id))`); leave it. Pending messages held in the room vector survive the merge (`merged.append(held)`), which is the behavior the send path relies on.

---

### `src/app/command.rs` (command)

**Analog:** itself, `cmd_reconnect` (lines 31-36):
```rust
app.conn = ConnState::Connecting;
app.effects.push(Effect::LoadRooms);
history::retry_failed(app);
```
D-17: `:reconnect` also re-runs the `/me` load. Prefer either adding `Effect::LoadMe` push here and at connect time in `lib.rs`, or folding `me` into `Client::load_rooms` best-effort (like `users.getPreferences`, api/mod.rs 251-263). D-08: `:reconnect` must NOT touch pending sends.

---

### `src/action.rs` (event enum, redacting Debug)

**Analog:** itself. Doc header (lines 1-8) and the `HistoryLoaded`/`HistoryFailed` variants (64-77) plus their Debug arms (158-171):
```rust
HistoryFailed { room: RoomId, older: bool, error: crate::api::ApiError },
...
Action::HistoryFailed { room, older, error } => write!(f, "HistoryFailed({room:?}, older={older}, {error:?})"),
Action::HistoryLoaded { room, messages, older, .. } => write!(f, "HistoryLoaded({:?}, <{} messages>, older={older})", room, messages.len()),
```
New variants: `MessageSent { room, id, message: Message }` (print room + id only), `SendFailed { room, id, error: ApiError, unconfirmed: bool }` (room, id, error kind), `RetrySend`, `DiscardSend` (unit, bindable), `MeLoaded(String)` (print `MeLoaded(..)`). The Debug match is exhaustive on purpose: every variant must choose its printing. Extend the `debug_redacts` test (line 185+) with `Message` bodies.

---

### `src/input/actions.rs` (vocabulary)

**Analog:** itself. Three lockstep edits: `ACTION_NAMES` (11-43), `action_from_name` (46-81), `name_of` (85-134, exhaustive, no wildcard):
```rust
"submit_input",           // ACTION_NAMES
"submit_input" => Action::SubmitInput,   // action_from_name
Action::SubmitInput => Some("submit_input"),   // name_of
```
Add `retry_send`, `discard_send` to all three; event-only actions (`MessageSent`, `SendFailed`, `MeLoaded`) go in the final `=> None` or-pattern list (lines 119-132). `names_round_trip` test (143-150) enforces consistency; `internal_actions_unnamed` (153-171) can gain the new event actions.

---

### `src/input/keymap.rs` (default rows)

**Analog:** itself `default_bindings` (lines 78-121):
```rust
(Mode::Normal, "za", Action::ToggleFold),
(Mode::Normal, "gg", Action::GotoTop),
```
Add `(Mode::Normal, "r", Action::RetrySend)` and `(Mode::Normal, "dd", Action::DiscardSend)`. `d` is unbound alone, so `dd` is a pure prefix like `gg`/`za`. No Normal `r`/`d`/`x` exists today. Keep `keymap_g_is_prefix`-style tests green and check `config/keymap.rs` clash/lockout tests and the `vim-extras` row-count test. D-09: newline stays `<A-CR>`/`<C-j>` (lines 105-106); documentation only.

---

### `src/config/settings.rs` (config validation)

**Analog:** itself, `message_layout` via `Reader::one_of` (lines 163-174):
```rust
if let Some(value) = table.get("message_layout")
    && let Some(layout) = reader.one_of(value, "message_layout",
        &[("compact", MessageLayout::Compact), ("two_line", MessageLayout::TwoLine)])
{ settings.message_layout = layout; }
```
Add `follow_on_send` ("always" default | "if_at_bottom"): enum `FollowOnSend`, `Settings` field + `Default` (89-100), entry in `KNOWN_KEYS` (102-113) or it is reported unknown, and a test copying `message_layout_values` (192-212: valid values, bad value yields one Error with file/line/key, wrong type yields "expected a string").

---

### `src/config/theme.rs` (config validation, vocabulary)

**Analog:** itself.
- Markers: add `sending`, `failed`, `unconfirmed` to `MARKER_KEYS` (71-82), `Markers` struct (96-107), `Default` (109-125, plain single-column glyphs e.g. `"…"`, `"!"`, `"?"`), `slot_mut` (140-155; non-encrypted slots use `MAX_MARKER_WIDTH = 4`). Parse path already rejects control/bidi (see `parse_markers`, ~line 455 `warn_unknown(table, MARKER_KEYS, "markers.")`).
- Styles: add `messages.pending` and `messages.failed` to `ELEMENT_PATHS` (21-44), `Theme` fields (near `messages_notice`, 187), `Default` (222: `messages_notice: plain.add_modifier(Modifier::DIM)`; pending copies this, failed copies the `status_error` look), and `style_slot_mut` (235-256: `("messages", "notice") => Some(&mut self.messages_notice)`).
- Tests: extend the complete-`[markers]` TOML test (~line 1016) and the element-path count tests (~753-799, 994).

---

### `src/ui/wrap.rs` (view, transform)

**Analog:** itself, `render_rows` (lines 122-194). System-message styling shows the exact pattern for state-dependent style:
```rust
let system = msg.kind == MessageKind::System;
let body_style = if system { ctx.theme.messages_notice } else { ctx.theme.messages_body };
let name = if system { ctx.theme.messages_notice } else if ctx.theme.messages_author.fg.is_some() {..} else {..};
```
Add: pending (`Queued|Sending`) -> `messages_pending` for author and body; `Failed|Unconfirmed` -> `messages_failed`. Marker placement (Compact): replaces the time cell for non-`Sent` messages, padded to `time_col` (line 149-150: `format!("{time}{}", " ".repeat(time_col - time.width()))`); TwoLine: after the author in the header (line 182-186). Marker text goes through `Markers` (already validated). Keep everything inside `render_rows` so the row model (`message_rows`) and renderer cannot disagree (module invariant, line 1-2). Echoed body text still goes through `sanitize_line`.

Tests/snapshots: copy `src/ui/messages.rs` insta tests (`message_kinds_52x20` snapshot) for new `send_states_*` snapshots; review with `cargo insta review`.

---

### `src/api/mod.rs` (HTTP client, request-response)

**Analog:** itself, `get_json` (216-243) and `load_history` (269-283).

Refactor `get_json` into `read_json(request: reqwest::RequestBuilder, client_error: ApiError)` containing the status check, `content_length` cap, 8 MiB chunk loop and `serde_json::from_slice`; `get_json` becomes a thin wrapper:
```rust
let mut response = request.send().await.map_err(|e| send_error(&e))?;
if let Some(error) = status_error(response.status()) { return Err(error); }
if response.content_length().is_some_and(|n| n > MAX_BODY_BYTES as u64) { return Err(ApiError::TooLarge); }
let mut body: Vec<u8> = Vec::new();
while let Some(chunk) = response.chunk().await.map_err(|e| send_error(&e))? {
    if body.len() + chunk.len() > MAX_BODY_BYTES { return Err(ApiError::TooLarge); }
    body.extend_from_slice(&chunk);
}
serde_json::from_slice(&body).map_err(|_| ApiError::BadResponse)
```
Update the "Only GET is ever sent in Phase 3" doc comment (line 214-215); redirects stay disabled (line 193), important for POST. New methods:
- `send_message(&self, room, id, text) -> Result<Message, ApiError>`: `self.http.post(self.endpoint("chat.sendMessage")).json(&SendBody{message: SendMsg{id, rid, msg}})`, send-path 4xx (other than 401/403/404/429) maps to new `ApiError::Rejected`.
- `get_message(&self, id)`: `GET chat.getMessage` with `query(&[("msgId", id)])` via `get_json`.
- `me(&self)`: `GET me` returning username only (best effort, like prefs at line 259 `prefs.ok()`).

`ApiError` (31-46, `Copy`, no payload) + `Display` (48-88, one fixed line each): add `Rejected` and `ConnectionLost`; map only `e.is_connect()` to `Unreachable` in `send_error` (106-119, currently the final `else` maps everything to `Unreachable`). Keep `status_error` (91-104) unchanged for GET (test `code(400) == BadResponse`). Add an `is_unconfirmed(ApiError) -> bool` classifier (Timeout, Server(_), ConnectionLost, BadResponse/TooLarge on a 2xx).

Tests: copy the wiremock style at api/mod.rs tests (287+): `connection()`, `client_for`, `Client::with_timeouts` for the delayed-response timeout case, `wiremock::matchers::{header, method, path, body_partial_json}`; assert the POST body is exactly `{"message":{"_id","rid","msg"}}` and auth headers present. Update the `ALL_ERRORS` array there and in `redaction.rs` (currently `[ApiError; 11]`).

---

### `src/api/wire.rs` / `src/api/map.rs` (transform)

**Analog:** wire.rs header (lines 1-9): Deserialize only, never Debug. Add:
```rust
#[derive(Deserialize)]
pub struct SendResp { pub message: WireMsg }
```
and a `MeResp { username }` (lenient, `#[serde(default)]`). Request bodies are `#[derive(serde::Serialize)]` structs with `#[serde(rename = "_id")]`, never hand-built JSON. `ApiDate` (11-19) already handles ISO and EJSON `ts`.

map.rs: extract the per-message construction from `history` (lines 214-225) into `pub fn message(room: &RoomId, msg: WireMsg) -> Message`, reused by `history` and `send_message`:
```rust
let (kind, body) = kind_and_body(&msg);
Message { id: msg.id, room_id: room.clone(),
    author: msg.u.username.unwrap_or_else(|| "unknown".to_string()),
    ts: msg.ts.secs().unwrap_or(0), body, kind, send: SendState::Sent }
```
`history` still needs `millis` for sorting, so compute `millis` before moving `msg` into `message`.

---

### `src/effect.rs` (effect runner)

**Analog:** itself. `Effect` (lines 10-24, currently `#[derive(Clone, Debug, PartialEq)]`), `run` incl. fail-closed `Err(client)` branch (28-41), per-effect arms (42-71), `spawn` (76-84), wiremock tests (86-177).

Add `Effect::SendMessage { room: RoomId, id: String, text: SendText, reconcile: bool }` and `Effect::LoadMe` (if used). `SendText` is a newtype with hand-written redacting `Debug` (`SendText(<N bytes>)`) and derived `Clone, PartialEq`, so the derived `Debug` on `Effect` stays safe.

Fail-closed arm (mirror lines 32-39):
```rust
Effect::SendMessage { room, id, .. } => Action::SendFailed { room, id, error, unconfirmed: false },
```
Run arm for send: if `reconcile`, `client.get_message(&id)`; Ok -> `vec![Action::MessageSent{..}]` and no POST; otherwise `client.send_message(..)`; Ok -> `MessageSent`, Err -> `SendFailed` with `unconfirmed = is_unconfirmed(error)`. On a 400 from a reconciling send, re-run `get_message` once (RESEARCH Pattern 4).

Tests: copy `load_history_actions` (122-163: mock 200 then mock 401, `assert_eq!(actions, [Action::...])`) and `load_history_without_client_fails_closed` (165-176); add `received_requests` assertion that a 200 `getMessage` produces no POST.

---

### `src/lib.rs` (run loop)

**Analog:** itself. Effects already drain generically (lines 110-112), so no change there. Add next to `app.zone = ...` (line 92) and `app.effects.push(Effect::LoadRooms)` (94):
```rust
app.id_prefix = session_id_prefix();   // new fn beside unix_now(); only place reading OS randomness
app.effects.push(Effect::LoadMe);      // D-17, if a separate effect
```
`unix_now` (60-67) is the model for "the reducer never reads clock/randomness". `session_id_prefix` per RESEARCH uses `std::hash::RandomState::new().hash_one(..)` (no new dependency).

---

### `src/redaction.rs` (test sweep)

**Analog:** itself. Header (lines 1-8): "Every new type that holds a secret or message text must be added to `sentinel_sweep`." Reuse `BODY` sentinel (line 28), `message_with_body()` (94-103), `ALL_ERRORS` (105-117), `sweep_both!`/`no_sentinel` (77-92). Add: a `Message` in each `SendState`, `Effect::SendMessage` with `SendText::new(BODY)`, `Action::MessageSent`, `Action::SendFailed`, `ApiError::Rejected`/`ConnectionLost` in `ALL_ERRORS` (bump the array length), and a rendered-screen check that echoed text is sanitized.

---

### `scripts/smoke_pty.py` (harness)

**Analog:** itself.
- `start_mock` (232-289): the handler currently returns 405 for every non-GET (line 267-268: `if self.command != "GET": self.reply(405, {"success": False})`) and records `(method, path, query, authed)`. Extend it: serve `POST /api/v1/chat.sendMessage` (parse JSON body from `Content-Length`, record it, echo `{"message": {...,"ts": ISO}, "success": True}`), `GET /api/v1/chat.getMessage`, and `GET /api/v1/me`; keep the 401 branch for unauthenticated requests ahead of them. Update the docstring ("Only GET is served").
- `docker_env(project)` (828-848): reads `dev/state/<project>/.env.local` in-process, refuses non-loopback hosts, never prints values. Copy for `scenario_docker_send` and for the Wave 0 probe.
- `scenario_docker_read` (line 914) is the model for `docker_send`; `scenario_insert` (449) for the `send` scenario against the mock.
- Register in `SCENARIOS` (971-984): `"send": scenario_send`, `"docker_send": scenario_docker_send`.
- Existing `insert` scenario now exercises a failed/queued send through the mock; keep it passing.

---

### `dev/` probe script (NEW, Wave 0; harness)

**Analog:** `scripts/smoke_pty.py::docker_env` for credential reading, `dev/seed.py` for talking to the local Docker server and project/state layout (`dev/state/<project>/.env.local`). Per RESEARCH Validation Architecture, probes P1-P7 must read the state file itself, print only HTTP status, key names and shapes (never values, tokens or bodies), refuse non-loopback hosts, and clean up probe rooms. Run first, before reducer work depends on D-15/duplicate-`_id` behavior.

## Shared Patterns

### Redacting Debug for anything holding text
**Source:** `src/data/mod.rs` 71-80, `src/action.rs` 103-176, `src/api/mod.rs` 135-140 (`HistoryPage`)
```rust
impl fmt::Debug for HistoryPage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HistoryPage(<{} messages>)", self.messages.len())
    }
}
```
**Apply to:** `SendText`, new `Action` arms, any pending-state type, `Effect::SendMessage` (via `SendText`).

### Typed, payload-free errors; `reqwest::Error` never formatted
**Source:** `src/api/mod.rs` 31-46, 106-119. `send_error` is the single mapping point. **Apply to:** all send/getMessage/me paths. Never display the server `error` string.

### Effect outbox + one channel (reducer pure, IO in `effect::run`)
**Source:** `src/app/history.rs` 57-65 (push), `src/effect.rs` 28-84 (run/spawn), `src/lib.rs` 110-112 (drain). **Apply to:** `send::pump`, `LoadMe`.

### Status-bar errors
**Source:** `src/app/history.rs` 238, `src/app/update.rs` 207-209: `app.status_msg = Some(StatusMsg::Error(text))`. `StatusMsg` has no Debug. Status text passes through `sanitize_line` in `ui/status.rs`.

### User-facing vocabulary is validated and documented
**Source:** `src/input/actions.rs` (exhaustive `name_of`), `src/config/theme.rs` (`MARKER_KEYS`, `ELEMENT_PATHS`), `src/config/settings.rs` (`KNOWN_KEYS`, `one_of`). Add every new name to its const list, its parser, `Default`, and the round-trip test.

### Test helpers
**Source:** `src/test_util.rs` (`app_80x24`, `app_with_config`, `config_from_toml`, `press`, `render_to_string`); wiremock client builder in `src/effect.rs` 96-110 and `src/api/mod.rs` 307-329.

## No Analog Found

| File/Concern | Role | Data Flow | Reason |
|---|---|---|---|
| Per-room serialized send queue (`pump`) | reducer logic | event-driven | No existing in-flight-limited queue; closest is `history.rs` `loading_older` flag (one request per room), used as the style model only. Follow RESEARCH Pattern 3 with D-16 modification. |
| Client-side id generation (`session_id_prefix`) | utility | transform | No randomness in repo; use std `RandomState` per RESEARCH. |
| POST requests | HTTP | request-response | Phase 3 sends only GET; `read_json` refactor extends `get_json`. |
| Marker drawn inside message rows | view | render | No per-message marker exists yet (sidebar markers only, `config/theme.rs` `Markers` + `ui/sidebar.rs`). |

## Metadata

**Analog search scope:** `src/` (app, api, config, input, ui, data, effect, action, lib, redaction), `scripts/smoke_pty.py`, `dev/`
**Files scanned:** about 20 read in full or by range; `git ls-files` listing of src/scripts/dev
**Pattern extraction date:** 2026-11-17
