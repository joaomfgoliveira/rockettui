# Phase 3: Connect & Read - Pattern Map

**Mapped:** 2026-11-16
**Files analyzed:** 22 new/modified
**Analogs found:** 19 / 22 (all analogs are git-tracked source under `src/` and `scripts/`)

Crate is small (about 11k lines). All excerpts below were read this session.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match |
|---|---|---|---|---|
| `src/api/mod.rs` (Client, ApiError, load_rooms, load_history) | service | request-response | none in repo; RESEARCH "Client builder" + `src/event.rs` task style | no analog |
| `src/api/wire.rs` | model | transform | `src/data/mod.rs` (types) + RESEARCH wire types | partial |
| `src/api/map.rs` (pure wire to domain) | utility | transform | `src/data/fake.rs` (builds `Room`/`Folder`/`Message`) | role-match |
| `src/effect.rs` (Effect enum + spawn) | service | event-driven | `src/event.rs::spawn_input_task` (task sends Actions on channel) | role-match |
| `src/config/connection.rs` (Secret, ServerUrl, Connection) | model/config | transform | `src/config/settings.rs` (Reader-based validation) + `src/data/mod.rs` (redacting Debug) | role-match |
| `src/config/mod.rs` (Sources, load_from_sources, load) | config | transform | itself | exact (modify) |
| `src/config/paths.rs` (env + file mode) | utility | file-I/O | itself `read_source`/`read_sources` | exact (modify) |
| `src/config/settings.rs` (KNOWN_KEYS add) | config | transform | itself | exact (modify) |
| `src/action.rs` (new variants + Debug arms) | model | event-driven | itself (`RoomsLoaded`, `MessagesLoaded` arms) | exact (modify) |
| `src/input/actions.rs` (None arms) | utility | transform | itself lines 120-132 | exact (modify) |
| `src/app/mod.rs` (effects, conn, perm_warning, history fields) | store | event-driven | itself (`App` struct) | exact (modify) |
| `src/app/history.rs` | store | CRUD | `src/app/messages.rs` (per-room `MsgView`, `row_starts`) | role-match |
| `src/app/update.rs` (Loaded/Failed arms, Select trigger) | controller (reducer) | event-driven | itself lines 42-52 | exact (modify) |
| `src/app/command.rs` (`reconnect`/`retry` row) | controller | request-response | itself `COMMANDS` + `cmd_quit` | exact (modify) |
| `src/data/mod.rs` (`MessageKind`) | model | transform | itself | exact (modify) |
| `src/data/fake.rs` (test-only) | utility | transform | itself | exact (modify) |
| `src/ui/messages.rs` (placeholders, system dim) | component | transform | itself lines 13-35 | exact (modify) |
| `src/ui/status.rs` (conn state, persistent warning) | component | transform | itself `render_status` | exact (modify) |
| `src/lib.rs` (`run`: client, effect drain, keep `tx`) | provider | event-driven | itself | exact (modify) |
| `src/main.rs` (stderr perm warning after restore) | config | request-response | itself (config error eprint) | exact (modify) |
| `scripts/smoke_pty.py` (mock server) | test | request-response | itself | exact (modify) |
| `dev/compose.yml`, `dev/seed.py`, `dev/README.md`, `.gitignore` | config | batch | `scripts/smoke_pty.py` (stdlib Python style) | partial |

## Pattern Assignments

### `src/effect.rs` (service, event-driven)

**Analog:** `src/event.rs` (28 lines; a spawned task that sends `Action`s on the one channel, ending with `Action::Quit` when its stream ends) and `src/lib.rs::run` loop.

Run-loop structure to extend (`src/lib.rs` lines 72-80 and 93-110). Today `tx` is dropped at line 80; keep one clone alive for REST tasks:
```rust
let (tx, mut rx) = tokio::sync::mpsc::channel::<Action>(CHANNEL_CAPACITY);
for action in data::fake::seed() { ... }   // REMOVE (D-03: no fake fallback)
event::spawn_input_task(tx.clone());
drop(tx);                                   // REMOVE; keep tx, drain app.effects after the update batch
...
for action in std::iter::once(Action::Clock(unix_now())).chain(batch.drain(..)) {
    saw_key |= matches!(action, Action::Key(_) | Action::KeyTimeout);
    update(&mut app, action);
    ...
}
// NEW: for effect in app.effects.drain(..) { effect::spawn(client.clone(), tx.clone(), effect); }
terminal.draw(|f| ui::view(f, &app))?;
```
Because `tx` stays alive, `rx.recv()` returns `None` only never; rely on the input task's `Action::Quit` (src/event.rs). Initial load: push `Effect::LoadRooms` and set `app.conn = Connecting` before the first draw (D-05), replacing the "Drain the seed before the first draw" block (lib.rs 86-90).

The `spawn` function: `tokio::spawn(async move { let action = ...api call...; let _ = tx.send(action).await; })` mirroring event.rs's `let _ = tx.send(...).await;`.

---

### `src/api/mod.rs` (service, request-response)

**No in-repo analog** (no HTTP code exists). Use RESEARCH.md "Client builder", "Endpoint URL and query building", and Pattern 7 (`ApiError` enum, `Clone + PartialEq`, no payload text) verbatim. In-repo conventions to follow:
- Hand-written redacting `Debug` for anything carrying secrets, as `Message` in `src/data/mod.rs` lines 57-66:
```rust
impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Message")
            .field("id", &self.id)
            .field("room_id", &self.room_id)
            .field("body_len", &self.body.len())
            .finish()
    }
}
```
- Register module in `src/lib.rs` (`pub mod api;` alphabetically before `app`, lines 4-11).
- Never format a `reqwest::Error`; map to `ApiError` in one function.

---

### `src/api/map.rs` (utility, transform) and `src/api/wire.rs`

**Analog:** `src/data/fake.rs` constructs the exact domain types (`Room`, `Folder`, `Message`, `FolderId`, `RoomId`) the mapper must produce; `src/data/mod.rs` lines 8-55 are the targets:
```rust
pub struct Room { pub id: RoomId, pub name: String, pub kind: RoomKind,
                  pub encrypted: bool, pub unread: bool, pub last_activity: i64 }
pub struct Folder { pub id: FolderId, pub name: String, pub room_ids: Vec<RoomId> }
pub struct Message { pub id: String, pub room_id: RoomId, pub author: String, pub ts: i64, pub body: String }
```
Mapping rules: RESEARCH Pattern 3 table; wire structs: RESEARCH "Wire types". Tests: fixture JSON via `include_str!`; use the `SENTINEL-BODY-7f3a` redaction idiom from `src/data/mod.rs` lines 84-97. Message needs a new `kind: MessageKind` (Open Question 2); 10 literal sites must be updated (`app/update.rs`, `app/messages.rs`, `data/fake.rs`, `data/mod.rs`, `ui/messages.rs`, `ui/mod.rs`, `action.rs`, `ui/wrap.rs`); consider a `Message::normal(..)` helper first.

---

### `src/config/connection.rs` (config, transform)

**Analog A:** `src/config/settings.rs` + `src/config/diag.rs` `Reader` for key validation that never echoes values (`Reader::string`, diag.rs lines 154-166):
```rust
pub fn string(&mut self, value: &Spanned<DeValue<'_>>, key: &str) -> Option<String> {
    match value.get_ref() {
        DeValue::String(s) => Some(s.to_string()),
        other => { self.error(value.span().start, key,
            format!("expected a string, found {}", other.type_str())); None }
    }
}
```
Reuse for `server_url`, `user_id`, `token` (never put a value in a `Problem`). `settings::parse` pattern (settings.rs 104-170): `if let Some(value) = table.get("key") && let Some(x) = reader.<fn>(value, "key", ..) { settings.field = x; }`.

**Analog B:** redacting types: `Secret` with `Debug` printing `Secret(<redacted>)` and no `Display` (RESEARCH "Secret wrapper").

**Known-keys edit** (`src/config/settings.rs` lines ~102-109 and the comment at ~177):
```rust
const KNOWN_KEYS: &[&str] = &["keymap_preset","sidebar_width","history_size",
    "timestamp_format","message_layout","key_timeout_ms"];   // add "server_url","user_id","token"
// D-03: unknown keys ... `server_url` is one of them until Phase 3 (D-12).   // update comment
```
Rewrite test `server_url_is_unknown_key` in `src/config/mod.rs` (~line 228).

---

### `src/config/mod.rs` and `src/config/paths.rs` (config, file-I/O)

**Analog:** themselves. Sources (mod.rs 37-44) currently `#[derive(Clone, Debug, Default)]` and holds raw config text; replace `Debug` derive with a hand-written one printing byte counts (Pitfall 3):
```rust
#[derive(Clone, Debug, Default)]
pub struct Sources { pub config: Option<String>, pub keymap: Option<String>,
                     pub theme: Option<String>, pub read_errors: Vec<Problem> }
```
Add env values (token as `Secret`) and `config_mode: Option<u32>`. Keep the "IO shell reads, pure core takes data" split: `load()` (mod.rs ~92-100) reads `std::env::var_os`, and passes in, exactly as `config_dir(xdg, home)` takes env as args (paths.rs 12-22, "edition 2024 makes `set_var` unsafe"). Mode read goes in `read_sources` (paths.rs 74-85) next to `read_source`'s `fs::metadata` (lines 42-49); use `#[cfg(unix)] std::os::unix::fs::PermissionsExt`, `mode & 0o077 != 0`. Keep `load_from_sources` tolerant (existing ~170 tests); add the "required" problems only on the `load()` path. `read_error(file, msg)` helper (paths.rs 24-32) builds `Problem`s for the error screen.

---

### `src/action.rs` and `src/input/actions.rs` (model, event-driven)

**Analog:** `RoomsLoaded`/`MessagesLoaded` handling. New variants (suggested `HistoryLoaded`, `HistoryFailed(RoomId, ApiError)`, `LoadFailed(ApiError)`) need:
1. Enum variant (action.rs lines 59-61 sit beside the `*Loaded` family).
2. Exhaustive `Debug` arm printing counts only (action.rs 129-141):
```rust
Action::MessagesLoaded(room_id, messages) => {
    write!(f, "MessagesLoaded({:?}, <{} messages>)", room_id, messages.len())
}
```
3. `=> None` arm in `src/input/actions.rs` lines 120-132 (the `| Action::MessagesLoaded(..) | Action::KeyTimeout | Action::Counted(..) => None` list).
4. Redaction test beside `debug_redacts` (action.rs ~146-186).
`Action` derives `Clone, PartialEq`, so `ApiError` must too.

---

### `src/app/update.rs` (reducer)

**Analog:** itself, lines 42-52:
```rust
Action::RoomsLoaded(rooms) => { app.rooms = rooms; sidebar::on_rows_changed(app); }
Action::FoldersLoaded(folders) => { app.folders = folders; sidebar::on_rows_changed(app); }
Action::MessagesLoaded(room_id, messages) => { app.messages.insert(room_id, messages); }
```
Add arms for new Actions; on `RoomsLoaded` also set `app.conn = ConnState::Ready`. Select trigger: Select is in the motion list (update.rs ~67) routed per pane; extend the Select-on-room call site (not `App::open_room`, app/mod.rs 181-184 `self.current_room = Some(id); self.focus = Focus::Messages;`) to insert `HistoryState::Loading` and push `Effect::LoadHistory` for non-encrypted rooms without an entry. Note `Action::Key` begins with `app.status_msg = None;` (update.rs ~19), so connection and warnings need their own `App` fields (Pattern 6). The reducer never reads clocks/IO.

---

### `src/app/history.rs` (store, CRUD)

**Analog:** `src/app/messages.rs` (per-room `MsgView { cursor, top_row }`, `row_starts(app, room)`, `viewport(app)`), lines 1-40 above. Follow its doc-comment style and "heights from `ui::wrap::message_rows`" rule. Prepend-with-cursor-kept: after prepending `n`, `cursor += n` and recompute `top_row` from new `row_starts` (Pattern 4). State enum `Loading | Loaded{oldest_ts, exhausted, loading_older} | Failed` per RESEARCH. Tests use `crate::test_util::{app_80x24, press}` like command.rs tests.

---

### `src/app/command.rs` (controller, request-response)

**Analog:** itself. Add a row to the table (lines 10-13) and a function like `cmd_quit` (16-19):
```rust
pub const COMMANDS: &[CommandDef] = &[CommandDef { names: &["q", "quit"], run: cmd_quit }];
pub fn cmd_quit(app: &mut App, _args: &[&str]) -> Result<(), String> { app.should_quit = true; Ok(()) }
```
New: `CommandDef { names: &["reconnect","retry"], run: cmd_reconnect }` setting `app.conn = Connecting`, `app.effects.push(Effect::LoadRooms)`, clearing `Failed` history entries. Tests copy command.rs 45-58 (`press(&mut app, ":reconnect<CR>")`, assert on `app.effects`).

---

### `src/ui/messages.rs` (component)

**Analog:** itself lines 13-35. Placeholders reuse `app.theme.messages_notice`:
```rust
let notice = app.theme.messages_notice;
let Some(room) = app.current_room.as_ref() else {
    frame.render_widget(Paragraph::new(Line::styled("No room open", notice)), inner); return; };
let msgs = app.messages.get(room).map_or(&[][..], Vec::as_slice);
if msgs.is_empty() {
    frame.render_widget(Paragraph::new(Line::styled("No messages yet", notice)), inner); return; }
```
Insert before the empty check: encrypted ("encrypted, not supported"), `Loading`, `Failed` ("Could not load history"). All text through `sanitize_line` (`ui/sanitize.rs`). Snapshot tests via insta in `src/ui/snapshots/`. System messages: `Message.kind == System` renders with notice style (also touches `ui/wrap.rs::render_rows`).

---

### `src/ui/status.rs` (component)

**Analog:** itself `render_status` (lines 11-36). Add spans in the same shape before `status_msg`:
```rust
spans.push(Span::styled(format!(" {}", sanitize_line(text)), app.theme.status_error));
```
for `ConnState::Connecting` (status_info), `Failed(err)` (status_error), and `perm_warning` (status_error, or new `status.warning`; if added update the `ELEMENT_PATHS` 22-count test in `config/theme.rs`). Tests: copy `themed_error_style` (status.rs ~75-100) using `TestBackend::new(40, 1)`.

---

### `src/main.rs` (config, request-response)

**Analog:** itself lines 10-17: error path does `tui::restore(); eprint!("{report}")`. For D-04, after `rockettui::tui::restore()` on the success path, `eprintln!` the permission warning (name file and mode only).

---

### `scripts/smoke_pty.py` and `dev/*`

**Analog:** `scripts/smoke_pty.py` (stdlib-only Python run with `python3 -I`). `wait_ready` currently relies on seeded rooms; add a stdlib `http.server` mock serving fixtures on an ephemeral port, passed through `ROCKETTUI_SERVER_URL/USER_ID/TOKEN`. `dev/seed.py` copies the same stdlib-only style. Compose skeleton and seed steps: RESEARCH "Docker Rocket.Chat fixture".

## Shared Patterns

### Redaction (D-07, AUTH-03)
**Source:** `src/data/mod.rs` 57-66 (`Message` Debug) and 84-97 (sentinel test); `src/action.rs` 87-91 and 146-186.
**Apply to:** `Secret`, `Connection`, `Sources`, `Config`, `ApiError`, every new `Action` variant, wire structs. Hand-written `Debug`, exhaustive match, counts not text; sentinel test per type.

### Pure core / IO shell
**Source:** `src/config/paths.rs` (only fs module; env passed in as args, line 12-22) and `src/lib.rs` (`unix_now()` is the only wall-clock read; reducer gets `Action::Clock`).
**Apply to:** config resolution (env + mode as data), the effect outbox (reducer pushes `Effect`s, `run()` spawns).

### Errors surfaced to user
**Source:** config problems via `Problem { file, pos, key, message, severity }` rendered by `ui/error_screen.rs`; runtime errors via `StatusMsg::Error(String)` in `app/command.rs` lines 30-36 and `status.error` theme style. Messages are fixed strings, never echoing values.

### Terminal-safe text
**Source:** `src/ui/sanitize.rs` `sanitize_line`, used in `ui/status.rs` and `ui/messages.rs`. Apply to room names, DM names, message bodies, system lines, placeholders, API error text.

### Tests
**Source:** `crate::test_util::{app_80x24, app_with_config, press, render_to_string, temp_config_dir, config_from_theme}`; insta snapshots in `src/ui/snapshots/`; `cargo test --lib`.

## No Analog Found

| File | Role | Data Flow | Reason |
|---|---|---|---|
| `src/api/mod.rs` | service | request-response | No HTTP or serde code exists; use RESEARCH Client builder, URL building, `ApiError` |
| `src/api/wire.rs` | model | transform | No serde-derive Deserialize types; use RESEARCH wire types |
| `dev/compose.yml` | config | batch | No Docker assets in repo; use RESEARCH compose skeleton (human-verify checkpoint) |

## Metadata

**Analog search scope:** `src/**`, `scripts/`
**Files scanned:** about 45 tracked files
**Pattern extraction date:** 2026-11-16
**Tracked-source check:** all named analogs are in `git ls-files` output; none are gitignored mirrors.
