# Phase 3: Connect & Read - Research

**Researched:** 2026-11-16
**Domain:** Rocket.Chat REST client (reqwest + serde) inside an existing Elm/TEA ratatui app; PAT auth; secret handling; local Docker Rocket.Chat fixture
**Confidence:** HIGH for the server API and reqwest behavior (read from Rocket.Chat 9.0.0 source and exercised in a scratch crate); MEDIUM for the Docker seed recipe (assembled from source and docs, not executed end to end)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Credentials & server config (AUTH-01, AUTH-02)**
- **D-01:** User id and token come from env vars `ROCKETTUI_USER_ID` / `ROCKETTUI_TOKEN` or `user_id` / `token` keys in `config.toml`. **Env vars override config** when both exist.
- **D-02:** Server URL is `server_url` in `config.toml`, overridable by `ROCKETTUI_SERVER_URL`. Validated at load: must be an http/https URL; `http://` is allowed (local Docker). This fulfils Phase 2 D-12's deferral.
- **D-03:** Missing or incomplete URL/user id/token is handled by the **existing Phase 2 config error screen** (D-02 there): it names exactly what is missing and where to set it; any key quits non-zero. No fake-data fallback.
- **D-04:** AUTH-02 permission check is **warn-only**. It runs only when the token actually comes from the config file (not from env). A group/other-readable file yields a persistent status-bar warning plus a stderr line after exit; the app still starts.

**Connection & failure UX (AUTH-03)**
- **D-05:** The TUI starts immediately with an empty sidebar and a "Connecting…" status. REST tasks run in the background and deliver results through the existing Action channel (`RoomsLoaded` etc.). No blocking pre-TUI login.
- **D-06:** Runtime failures (401, unreachable, TLS, timeout) show a short actionable **status-bar error** using the Phase 2 `status.error` style; the app stays usable (`:q`). A **`:reconnect`** (alias `:retry`) command re-runs the load, added to the table-driven command dispatch. REST only; realtime reconnect stays MSG-04 in Phase 5.
- **D-07:** **No log file in Phase 3.** Credentials live in a secret wrapper with a redacting `Debug` (same approach as `Message` in Phase 1), a test asserts neither `Debug` nor `Display` of config/credentials/errors contains the token, and error messages are typed/generic with no message content.
- **D-08:** HTTP client is `reqwest` with rustls defaults and standard certificate verification. **No** `insecure_skip_verify` or `ca_cert` option now (private CA, if needed, is Phase 6).

**Room list mapping (ROOM-01..03)**
- **D-09:** Only the user's **joined rooms** appear (subscriptions), sorted by last activity (Phase 1 D-11). No directory browsing or joining. DMs are named by the other user's username. Subscriptions with `open: false` are skipped. Discussions and teams are included (Phase 1 D-12).
- **D-10:** The sidebar keeps its **Unread / Folders / Recent** layout. Rocket.Chat 8.9+ (Enterprise) has user-defined sidebar **Categories**; the researcher must find the API and minimum server version. If the category API is available, Folders = the user's categories. Otherwise (e.g. local Docker without a license) Folders = a single **Favorites** group (subscription `f` flag). Sidebar fold/unfold code is reused unchanged either way.
- **D-11:** `Room.unread` is set from subscription `unread > 0` as a boolean only. No counts or mentions. Opening a room does **not** mark it read on the server.
- **D-12:** Encrypted rooms (`encrypted: true`) are listed with the Phase 2 `[markers]` encrypted marker. Opening one shows an "encrypted, not supported" placeholder in the messages pane and **never requests history**.

**History loading & scroll (MSG-01)**
- **D-13:** History is fetched **on first open** of a room, `history_size` newest messages (config, default 50), and **cached per room**. Reopening does not refetch (realtime keeps it fresh in Phase 5). No startup prefetch of all rooms.
- **D-14:** Scrolling to the oldest loaded message (`gg` or cursor at the top) **loads the next older page** (`history_size` each), prepending while keeping the cursor on the same message. Includes handling "no more history".
- **D-15:** Rendering stays text-only. System messages show a dim one-liner (e.g. "* joined the room"); attachments show `[attachment: name]`. No markdown, reactions, edit indicators or threads (out of scope). All text goes through the existing sanitization (`ui/sanitize.rs`).
- **D-16:** While history loads, the messages pane shows a "Loading…" placeholder. On failure it shows "Could not load history" plus the status-bar error. Reopening the room or `:reconnect` retries. Empty rooms keep Phase 1's empty state.

### Claude's Discretion
- HTTP client module layout (`api/` per the planned structure), request/response types, timeouts, user-agent string (generic, no personal details).
- Exact REST endpoints (`subscriptions.get` / `rooms.get` / `channels|groups|im.history` or `chat.syncMessages`) — researcher to confirm against the Docker server version.
- Env var and error wording details, loading placeholder styling via new theme elements if needed (document any new `theme.toml` names; they are user-facing vocabulary per Phase 2).
- Docker Rocket.Chat setup/seed instructions for DLVR-01.

### Deferred Ideas (OUT OF SCOPE)
- Browsing/joining public rooms from the directory — new capability.
- Unread counts, mentions, mark-as-read on open — v2 (MSG-06).
- `ca_cert` / insecure TLS options — revisit in Phase 6 if the company server needs a private CA.
- File logging (tracing) — revisit with DDP in Phase 5 if needed.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| AUTH-01 | Authenticate with a PAT (user id + token) from env var or config file | `X-User-Id` / `X-Auth-Token` headers on every REST call; `Secret` newtype; env-over-config resolution in a pure core (Standard Stack, Pattern 2, Code Examples) |
| AUTH-02 | Warn if a config file holding credentials is not `chmod 600` | `std::os::unix::fs::PermissionsExt` mode check in the IO shell only; `Sources` carries the mode; persistent warning field (Pattern 6, Pitfall 9) |
| AUTH-03 | Token and message contents never written to logs | No log file exists; the leak surfaces are `Debug`/`Display`/error text/redirects. `set_sensitive(true)`, `redirect::Policy::none()`, `without_url()`, typed `ApiError` (Pitfalls 1-4, Security Domain) |
| ROOM-01 | See channels, private groups and DMs in the sidebar | `subscriptions.get` + `rooms.get` joined by `rid` = `_id`; kind mapping from `t`, `prid`, `teamMain` (Pattern 3) |
| ROOM-02 | Select a room to open it | Existing `App::open_room`; add an `Effect::LoadHistory` outbox entry on first open (Pattern 5) |
| ROOM-03 | Encrypted rooms show "encrypted, not supported" | `encrypted` is only on `rooms.get`, not on `subscriptions.get` (Pitfall 5); never request history |
| MSG-01 | View message history and scroll it | `channels|groups|im.history` with `roomId`, `count`, `latest`; newest-first; cursor keyed on exact `ts` (Pattern 4, Pitfalls 6-7) |
| DLVR-01 | Works against a local Docker Rocket.Chat | Compose + seed recipe, fixture capture, optional Python mock server for pty smoke (Docker section, Validation Architecture) |
</phase_requirements>

## Project Constraints (from CLAUDE.md)

Source: `/home/joliveira/personal-projects/rockettui/.claude/CLAUDE.md` (read this session). Treated with the authority of locked decisions.

- **Tech stack:** Rust, `ratatui` + `crossterm`, `tokio`, `reqwest` + `serde`, `serde` + `toml`. `tokio-tungstenite` is for Phase 5, not now.
- **Architecture:** Elm/TEA: one `App` state, one `Action` enum, one channel, one reducer. The reducer never does IO or reads clocks.
- **Security:** act only as a normal client with the user's own account; revocable PAT, never committed or logged; credentials from env var or `chmod 600` config file (warn otherwise); never log message contents.
- **Config:** XDG at `~/.config/rockettui/`, TOML; invalid entries yield clear errors, not crashes.
- **Policy:** confirm company policy before pointing at the company server (Phase 6; not this phase).
- **Workflow:** file changes go through a GSD command (planner/executor concern, not research).
- No project skills found (`.claude/skills/` etc. checked via CLAUDE.md "Project Skills: none").

## Summary

The server contract needed for Phase 3 is small and stable. Authenticate every request with `X-User-Id` and `X-Auth-Token` (PAT). Build the sidebar from **two** calls that must be joined: `GET /api/v1/subscriptions.get` (per-user: `open`, `f`, `unread`, `alert`, `category`, `t`, `rid`, `name`, `fname`, `prid`) and `GET /api/v1/rooms.get` (per-room: `encrypted`, `lm`, `teamMain`, `teamId`, `prid`). The split matters: the subscription projection does **not** include `encrypted`, `lm`, `teamMain` or `teamId`, so encrypted-room detection (ROOM-03) and Team classification depend on `rooms.get`. History is `GET /api/v1/{channels|groups|im}.history?roomId=…&count=…&latest=…`, returned **newest first**; page older by passing the exact `ts` of the oldest loaded message as `latest`. The server silently caps `count` at `API_Upper_Count_Limit` (default 100), so "no more history" must be detected by an empty page, never by `len < requested`.

Sidebar Categories exist since Rocket.Chat **8.9.0** and need the `experimental-enterprise-features` license to *assign* rooms. There is no dedicated read endpoint: the category list lives in the user preference `sidebarCategories` (`GET /api/v1/users.getPreferences`) and a room's membership is `subscription.category`. Reading is not license-gated in the endpoint source, so the app can always try it and fall back to Favorites (`f`) when the list has no custom entries. Because RC makes category and favorite assignment mutually exclusive per room, a union (Favorites folder first, then categories) is lossless; this slightly extends D-10 and is flagged for confirmation.

Two security findings go beyond the decisions already locked. (1) **reqwest 0.13.5 forwards `X-Auth-Token` and `X-User-Id` on cross-host redirects** (it only strips `Authorization`/`Cookie`-class headers); this was reproduced in a scratch crate. The client must use `redirect::Policy::none()`. (2) `reqwest::Error`'s `Display`/`Debug` embed the full request URL, so errors must be mapped to a typed, `Clone + PartialEq` `ApiError` at the `api/` boundary (which `Action` also needs). Mark the token `HeaderValue` with `set_sensitive(true)` so the client's own `Debug` prints `Sensitive` (verified).

**Primary recommendation:** Add an `api/` module (reqwest 0.13 + serde, rustls default, redirects off, typed `ApiError`), an `Effect` outbox on `App` drained by `run()` (keeps `update()`'s signature and the pure reducer), a std-only `Secret` newtype, and join `subscriptions.get` + `rooms.get` (+ best-effort `users.getPreferences`) in one background load that emits the existing `RoomsLoaded`/`FoldersLoaded` Actions. Slice the work tracer-first (config → client → rooms in sidebar → open room → history) as Phase 2 did.

## Architectural Responsibility Map

This is a single-process TUI client, so the "tiers" are the TEA layers plus the remote server.

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Credential/URL resolution (env over file, validation) | Config pure core (`config::load_from_sources`) | Config IO shell (`config::load`, `paths.rs`) | Pure core stays testable; env and file mode are read only in the shell and passed in as data |
| chmod 600 check (AUTH-02) | Config IO shell (reads `mode()`) | Reducer-visible `App` field (warning text) | Only the shell touches the filesystem (existing rule in `paths.rs`) |
| HTTP requests, headers, error mapping | `api/` module (reqwest) | — | Single choke point for the token, redirects, timeouts and redaction |
| Wire JSON to domain types (`Room`, `Message`, `Folder`) | `api/map.rs` (pure functions) | — | Pure, fixture-testable; keeps serde types out of `App` |
| Deciding when to fetch (first open, older page, reconnect) | Reducer (emits `Effect`s into `app.effects`) | Run loop (spawns tasks) | Reducer stays pure; the run loop is the only place that performs IO |
| Per-room history cache and paging state | `App` (new `history` map) | — | Single state; renderer and reducer read it |
| Placeholders (Loading, Could not load, encrypted) and system-message dimming | `ui/messages.rs` | `ui/sanitize.rs` | Existing renderer; all text sanitized |
| Persistent warning and connection status | `ui/status.rs` | `App` fields | Status bar already renders badge, room, pending, message |
| Rate limiting, auth, history retention, E2EE flag | Rocket.Chat server | — | Client only reacts: 401/403/429 mapping |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| reqwest | 0.13.5 (2026-09-08) | Async HTTP client | In the author's chosen stack; 0.13 makes rustls the default TLS (`default-tls = ["rustls"]`) so D-08 "rustls defaults" is just the default features plus `json` and `query` [VERIFIED: crates.io API; reqwest v0.13.5 Cargo.toml `[features]`; CHANGELOG v0.13.0 "`rustls` is now the default TLS backend"] |
| serde | 1.0.229 (`derive`) | Wire types | Already the project's deserialization standard (via toml); needed directly for derive [VERIFIED: crates.io API] |
| serde_json | 1.0.154 | Decode bodies, fixture tests | reqwest's `json` feature depends on it; also used in fixture tests [VERIFIED: crates.io API] |
| chrono | 0.4.45 (already a dependency) | Parse ISO-8601 `ts`/`lm` | `DateTime::parse_from_rfc3339` works with the project's `default-features = false, features = ["clock","std"]`; keeps milliseconds [VERIFIED: scratch-crate run: `1794823200 1794823200123`] |

Add to `Cargo.toml`:
```toml
reqwest = { version = "0.13", features = ["json", "query"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```
`json` and `query` are **not** default features in 0.13 (CHANGELOG: "`query` and `form` are now crate features, disabled by default"; `json = ["dep:serde", "dep:serde_json"]`). The `http2`, `charset` and `system-proxy` defaults stay (system-proxy honors `HTTPS_PROXY`, useful on a corporate network for Phase 6). A clean build of reqwest with these features compiled in about 45 s here, with no extra system packages (aws-lc-rs built without cmake) [VERIFIED: scratch build, Fedora, rustc 1.99.1]. reqwest 0.13.5 needs rustc 1.85; the project's `rust-version = "1.88"` is fine [VERIFIED: crates.io versions API `rust_version 1.85.0`].

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| wiremock | 0.6.5 (dev-dependency) | In-process mock HTTP server for `api/` tests | Verified to work with reqwest 0.13.5 and `#[tokio::test]` (macros feature is already enabled) [VERIFIED: scratch crate test passed] |
| insta | 1 (existing) | Snapshots for Loading / encrypted / failed placeholders and the warning bar | Existing pattern; snapshot files already live in `src/ui/snapshots/` |

`url` is not added: use `reqwest::Url` (re-export). Verified behaviors for the validator are in Code Examples.

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Hand-written `Secret` newtype (std only) | `secrecy` 0.10 (+ `zeroize`) | Both pass the legitimacy check. `secrecy` adds zeroize-on-drop, but the project already hand-writes redacting `Debug` (Phase 1 `Message`) and the token ends up in a reqwest `HeaderValue` anyway; a 15-line newtype avoids two crates. Pick `secrecy` only if the author wants zeroize for learning |
| wiremock | httpmock 0.9.0 / a hand-rolled `tokio::net::TcpListener` | httpmock is equivalent; wiremock verified here. A hand-rolled listener removes a dev-dependency but costs more code |
| per-type `*.history` | `rooms.history` (new cursor endpoint), `chat.syncMessages` | `rooms.history` exists only in 9.0.0 source (company version unknown, Phase 6). `chat.syncMessages` is incremental sync (`lastUpdate` required, returns `updated`/`deleted`/`cursor`); it fits Phase 5 gap-filling, not "latest N" |

## Package Legitimacy Audit

Ran `gsd-tools query package-legitimacy check --ecosystem crates …` this session.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| reqwest | crates.io | since 2016-10 | 20.9M/wk | github.com/seanmonstar/reqwest | OK | Approved (named in CLAUDE.md stack) |
| serde | crates.io | since 2014-12 | 33.9M/wk | github.com/serde-rs/serde | OK | Approved (named in CLAUDE.md stack) |
| serde_json | crates.io | since 2015-08 | 34.1M/wk | github.com/serde-rs/json | OK | Approved |
| wiremock | crates.io | since 2020-04 | 2.8M/wk | github.com/LukeMathWalker/wiremock-rs | OK | Approved (dev-dependency) |
| httpmock | crates.io | since 2019-09 | 544K/wk | github.com/httpmock/httpmock | OK | Not used (alternative only) |
| secrecy | crates.io | since 2018-10 | 4.6M/wk | github.com/iqlusioninc/crates | OK | Not used (alternative only) |
| zeroize | crates.io | since 2018-10 | 20.4M/wk | github.com/RustCrypto/utils | OK | Not used (alternative only) |
| url | crates.io | since 2014-11 | 20.7M/wk | github.com/servo/rust-url | OK | Not added directly (use `reqwest::Url`) |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

Crates.io has no `postinstall` concept; build scripts of transitive crates (aws-lc-sys) compiled normally here. Packages come from the author's CLAUDE.md stack (reqwest, serde) or are well-known; `wiremock` was discovered by training knowledge and then verified by the seam (OK), by crates.io metadata, and by an actual passing test, so it is not `[ASSUMED]`.

## Architecture Patterns

### System Architecture Diagram

```
                       startup (main.rs)
  env vars ─┐
  config.toml ──► config::load()  ── errors ──► error screen (Phase 2, exits 1)
  (+ file mode)     │  pure core: resolve env>file, validate URL, wrap token in Secret
                    ▼
               Config { settings, keymap, theme, connection }
                    │
                    ▼
   run(): build api::Client (headers sensitive, redirects off, timeouts)
          │             │ on build failure: ConnState::Failed, still start the TUI
          ▼             ▼
   App::with_config   effects.push(LoadRooms)  ──► "Connecting…" in status bar
          │
  ┌───────┴──────────── run loop (single select) ────────────────────────┐
  │ batch of Actions ─► update(&mut app, action) (pure; may push Effects) │
  │ draw(ui::view)                                                        │
  │ drain app.effects ─► tokio::spawn(effect::run(client.clone(), tx, e)) │
  └───────▲──────────────────────────────────────────────┬────────────────┘
          │ Action::RoomsLoaded / FoldersLoaded /        │
          │ HistoryLoaded / LoadFailed / HistoryFailed   ▼
          │                                    api::Client (reqwest)
   one mpsc channel (existing, CHANNEL_CAPACITY) ◄── join!(subscriptions.get,
          ▲                                           rooms.get, users.getPreferences)
   input task (keys, resize, paste)                   ─► api::map (pure) ─► Room/Folder
                                                      GET {channels|groups|im}.history
                                                      ─► api::map ─► Vec<Message> (oldest first)
                                                              │
                                                              ▼
                                                  Rocket.Chat server (REST)

  Reducer triggers (all pure, push Effect):
    Select on a room, not encrypted, no history entry  ─► Effect::LoadHistory{room, before: None}
    cursor reaches index 0 (gg / k) and has_more       ─► Effect::LoadHistory{room, before: Some(cursor)}
    :reconnect / :retry                                ─► Effect::LoadRooms (+ clears Failed history entries)
```

### Recommended Project Structure
```
src/
├── api/
│   ├── mod.rs        # Client (reqwest), ApiError, load_rooms(), load_history(); the only place the token is used
│   ├── wire.rs       # serde structs for subscriptions/rooms/history/preferences (lenient: Option + #[serde(default)])
│   ├── map.rs        # PURE wire -> domain mapping (kind, name, unread, folders, system text, attachment text)
│   └── fixtures/     # JSON captured from the Docker seed; include_str! in tests
├── effect.rs         # enum Effect { LoadRooms, LoadHistory{..} } + spawn/run (the only IO launcher)
├── config/
│   ├── connection.rs # Secret, ServerUrl, Credentials, Connection; resolve(env, file) -> Result/Problems
│   └── paths.rs      # + read mode of config.toml (cfg(unix)); Sources gains mode + env
├── app/
│   ├── history.rs    # per-room HistoryState, prepend-with-cursor-preserved, paging triggers
│   └── command.rs    # + ["reconnect","retry"] row
├── data/mod.rs       # Message gains a kind (Normal | System); fake.rs becomes test-only data
└── ui/               # messages.rs placeholders; status.rs persistent warning + connection state
dev/                  # compose.yml, seed.py, README.md  (Docker fixture for DLVR-01)
```

### Pattern 1: Effect outbox on `App` (keeps the reducer pure)
**What:** `update(&mut App, Action)` currently returns `()` and has many call sites (every test helper). Add `pub effects: Vec<Effect>` to `App`; the reducer pushes intents; `run()` drains them after each batch and spawns tasks that send Actions back on the same channel. Tests assert on `app.effects` with no HTTP.
**When to use:** every fetch trigger (first open, older page, `:reconnect`, initial load).
**Why not return effects from `update`:** it changes the signature of a function used in `test_util::press` and dozens of tests for no gain.
**Example:**
```rust
// Source: pattern derived from src/lib.rs run loop (read this session); not from external docs
pub enum Effect {
    LoadRooms,
    LoadHistory { room: RoomId, kind: RoomKind, before: Option<String> /* exact server ts */, count: u16 },
}
// run(): after the update() batch and before draw
for effect in app.effects.drain(..) {
    effect::spawn(client.clone(), tx.clone(), effect);
}
```
**Channel lifetime:** `run()` today ends with `drop(tx)` after cloning for the input task [VERIFIED: src/lib.rs:79-80 `event::spawn_input_task(tx.clone());` / `drop(tx);`], so `rx.recv()` returning `None` ends the loop. REST tasks need a live sender, so keep one `tx` in `run()`. Consequence: `recv()` no longer yields `None` when the input task ends; that is already covered because `spawn_input_task` sends `Action::Quit` when the event stream ends [VERIFIED: src/event.rs `let _ = tx.send(Action::Quit).await;`]. Replace the `None => break` arm with a defensive `None => break` that stays unreachable, do not rely on it.

### Pattern 2: Credentials in a pure core with env and file mode passed in as data
**What:** Extend `Sources` (pure input) with the three env values and the config file mode; `load()` (IO shell) reads `std::env::var` and `fs::metadata().permissions().mode()`. `load_from_sources` stays pure and returns `Problem`s the existing error screen already renders.
**Constraints found in the existing code:**
- `Sources` is `#[derive(Clone, Debug, Default)]` and holds the **raw text of config.toml** (`pub config: Option<String>`) [VERIFIED: src/config/mod.rs:38-39 `#[derive(Clone, Debug, Default)]` / `pub struct Sources`]. Once the file may contain `token = "…"`, any `{:?}` of `Sources` leaks it. Replace the derive with a hand-written `Debug` that prints lengths only, and hold the env token as `Secret` from the moment it is read.
- Absent config + no env currently yields `Ok(Config::default())` (CONF-02). D-03 makes URL/user id/token mandatory for the binary, but ~170 existing tests rely on `load_from_sources` succeeding with no connection. Keep `load_from_sources` tolerant (`connection: Option<Connection>`, `None` when nothing is set) and add the "missing" problems only on the path the binary uses (e.g. `load()` passes `require_connection: true`). A partial setting (URL set, token missing) is always an error.
- `settings.rs` registers known keys and currently treats `server_url` as an unknown key: `// D-03: unknown keys and tables only warn. `server_url` is one of them until Phase 3 (D-12).` [VERIFIED: src/config/settings.rs:177]. Add `server_url`, `user_id`, `token` to `KNOWN_KEYS` (settings.rs:102) and **rewrite the existing test `server_url_is_unknown_key`** [VERIFIED: src/config/mod.rs:228], which asserts the opposite.
- The existing reader never echoes values (`Reader::string` reports only the type; toml's `Display` that prints the source line is deliberately unused, `diag.rs` header comment). Reuse it unchanged for `user_id` and `token`; never put a value in a `Problem` message. For the URL error say only "must be an http:// or https:// URL"; a URL with userinfo can itself be a credential.
- An empty env var (`ROCKETTUI_TOKEN=`) should count as unset; a non-UTF-8 env value is a config error that does not print the value.

### Pattern 3: Join `subscriptions.get` and `rooms.get`, then map (pure)
**What:** Run both calls concurrently (`tokio::join!`) and join on `subscription.rid == room._id`. Fail the whole load if `rooms.get` fails (an encrypted room listed without its flag would later request history, violating D-12). A subscription with no matching room record is skipped (fail closed).
**Mapping rules** (inputs read from `packages/core-typings` at tag 9.0.0):

| Domain field | Source | Rule |
|---|---|---|
| `Room.id` | `rid` / `_id` | verbatim |
| `Room.kind` | `t`, `prid`, `teamMain` | `t=="d"` → `DirectMessage`; else `prid` present → `Discussion`; else `teamMain==true` (room) → `Team`; `t=="c"` → `Channel`; `t=="p"` → `PrivateGroup`; anything else (`"l"` livechat) → skip |
| `Room.name` | sub `name`/`fname` | DM: `name` (the other user's username, D-09); others: `fname` if present else `name` |
| `Room.encrypted` | room `encrypted` | `unwrap_or(false)` |
| `Room.unread` | sub `unread` | `unread > 0` only (D-11) |
| `Room.last_activity` | room `lm`, else sub `ts` | parse ISO to unix seconds |
| skip | sub `open == false` | D-09 |
| skip | sub `status` present (`INVITED`/`BANNED`) | not joined |

`RoomType` is `'c' | 'd' | 'p' | 'l'` [VERIFIED: core-typings RoomType.ts at 9.0.0: `export type RoomType = 'c' | 'd' | 'p' | 'l';`]. `Room`'s existing fields are `id, name, kind, encrypted, unread, last_activity` [VERIFIED: src/data/mod.rs:28-36].

### Pattern 4: History paging keyed on the exact server timestamp
**What:** The three history routes return `{ messages: [...], success: true }`, **newest first** (`sort: { ts: -1 }`). Reverse to oldest-first for `App.messages`. For the next older page send `latest=<ts of the oldest loaded message>` with `inclusive` omitted (false). `latest` is compared with millisecond precision; `Message.ts` is whole **seconds** (`pub ts: i64` "Unix seconds"), so recomputing `latest` from it would skip same-second messages that are older than the oldest loaded one. Keep the raw ISO string (or ms) of the oldest loaded message in a per-room paging record and send that. Dedupe on `_id` when prepending as a belt-and-braces measure.
**Per-room state** (new `App.history: HashMap<RoomId, HistoryState>`): `Loading`, `Loaded { oldest_ts: String, exhausted: bool, loading_older: bool }`, `Failed`. Encrypted rooms never get an entry and never reach `Effect::LoadHistory`.
**Prepend with cursor kept (D-14):** `MsgView { cursor, top_row }` is stored per room [VERIFIED: src/app/messages.rs `pub struct MsgView { pub cursor: usize, pub top_row: usize }`]. After prepending `n` messages: `cursor += n`, and recompute `top_row` so the cursor message stays on the same screen row (new `row_starts[cursor]` minus the old on-screen offset). `view()` re-clamps through `scroll_into_view`, but a stale stored `top_row` would still jump. A room with no stored view shows its newest message and needs no fix.
**Trigger:** after a Messages-pane motion, if `cursor == 0`, the room is `Loaded`, not `exhausted`, and not `loading_older` → `Effect::LoadHistory { before: Some(oldest_ts) }`. `exhausted` is set when a page comes back empty.
**Endpoint by kind:** `Channel`/`Discussion`/`Team` with `t=="c"` → `channels.history`; `t=="p"` → `groups.history`; `DirectMessage` → `im.history`. Keep `t` in the effect (or derive from kind plus a stored `t`), because Discussion and Team rooms are `c` or `p` underneath. Use `roomId`, never `roomName`.
**`history_size` and the server cap:** `getPaginationItems` clamps `count` to `API_Upper_Count_Limit` (default 100) silently [VERIFIED: server/api/lib/getPaginationItems.ts "if (count > hardUpperLimit) { count = hardUpperLimit; }"; settings `await this.add('API_Upper_Count_Limit', 100, …)`]. `HISTORY_SIZE_RANGE` is `(1, 1000)` [VERIFIED: src/config/settings.rs:14 `pub const HISTORY_SIZE_RANGE: (i64, i64) = (1, 1000);`]. Send `count = history_size`, accept a shorter page, and treat only an **empty** page as the end.

### Pattern 5: First-open and reopen logic in the reducer
`App::open_room` today is `self.current_room = Some(id); self.focus = Focus::Messages;` [VERIFIED: src/app/mod.rs:181-184]. Extend the call site (not the method's contract) so that on `Select` of a room: if `room.encrypted` → nothing (the renderer shows the placeholder); else if no `history` entry → insert `Loading` and push `Effect::LoadHistory`; else (cached) → nothing, even after a `Failed` entry is cleared by `:reconnect`; `Failed` entries retry on reopen (D-16).

### Pattern 6: Connection state and the persistent warning need their own fields
`Action::Key` begins with `app.status_msg = None;` [VERIFIED: src/app/update.rs:19], so any key press erases a transient status. D-04 says the permission warning is **persistent** and D-05/D-06 imply "Connecting…"/failure stay visible, so do not store them in `status_msg`. Add `App.conn: ConnState { Connecting, Ready, Failed(ApiError) }` and `App.perm_warning: Option<String>`, rendered by `render_status` (badge, room, warning, pending, message). `StatusMsg` keeps carrying `:reconnect` command errors. Styling: reuse `status.error` for failures (D-06). For the warning either reuse `status.error` or add one new theme element `status.warning` (default yellow); if added, document it as user-facing vocabulary and update the element-count test (`ELEMENT_PATHS` has 22 entries and a test asserts 22). Reuse `messages.notice` for Loading / Could not load / encrypted placeholders and for system-message lines so no further theme names are needed.

### Pattern 7: `ApiError` is a small typed enum, `Clone + PartialEq`, no payload text
`Action` derives `Clone, PartialEq` [VERIFIED: src/action.rs `#[derive(Clone, PartialEq)] pub enum Action`], and `reqwest::Error` is neither, and its `Display`/`Debug` print the URL. Map at the boundary:
`Unauthorized (401)`, `Forbidden (403)`, `RateLimited (429)`, `Redirected (3xx, redirects off)`, `NotFound`, `Server(u16)`, `Unreachable` (connect error, which includes TLS handshake failures), `Timeout`, `BadResponse` (decode failure), `Client` (builder failure). Each has a fixed user-facing string with the remedy ("Unauthorized: check user id and token", "Cannot reach server: check server_url", "Rate limited by server: wait a minute, then :reconnect"). TLS-vs-unreachable cannot be separated reliably from `reqwest::Error`, so one combined message is acceptable ("cannot connect (unreachable or TLS error)"). Map on the **HTTP status first**: Rocket.Chat 9.0.0 changed auth/permission failures to 401/403 and 401 bodies lost `status`/`message` [CITED: Rocket.Chat 9.0.0 release notes, #43138], so a body-text match would be brittle across versions.

### Anti-Patterns to Avoid
- **Reading `Debug`/`Display` of `reqwest::Error`, `Sources` or `Config`:** leaks the URL and possibly the token.
- **Putting the token in `App`:** it only needs to live in the `HeaderMap` inside `api::Client`.
- **Fetching in the reducer or from `ui`:** violates TEA; use the outbox.
- **Using `rooms.history` or `chat.syncMessages` for the initial page:** wrong shape/availability for the stated goal.
- **Seeding rooms from `fake::seed()` in `run()`:** D-03 forbids a fake-data fallback. Keep `data::fake` for tests (`App::with_fake_data`, `test_util::app_80x24`).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| HTTP, TLS, timeouts | A TCP/TLS client | `reqwest` 0.13 (rustls, platform verifier) | Certificate verification and proxy support; platform verifier reads the OS trust store, which also makes a private CA a Phase 6 OS-level fix |
| Header redaction | Custom header wrapper | `HeaderValue::set_sensitive(true)` | The client's and map's `Debug` print `Sensitive` instead of the value [VERIFIED: probe output `{"x-auth-token": Sensitive}` and `default_headers: {…"x-auth-token": Sensitive…}`] |
| URL parsing/joining/query encoding | String concatenation | `reqwest::Url` with `path_segments_mut().pop_if_empty().extend(..)` and `query_pairs_mut()` or `.query()` | Handles subpath deployments and encodes `roomId`/`latest` [VERIFIED: probe output below] |
| ISO-8601 parsing | Manual split/parse | `chrono::DateTime::parse_from_rfc3339` | Already a dependency |
| Test HTTP server | Hand-written socket server | `wiremock` | Verified with reqwest 0.13.5 |
| JSON decoding | Manual `serde_json::Value` walking | `#[derive(Deserialize)]` wire structs, all optional fields `Option`/`#[serde(default)]` | Server adds fields across versions; unknown fields are ignored by default |
| Terminal-safe text | New escaping | Existing `ui::sanitize` (`sanitize_line`) | Single choke point from Phase 1; room names, DM names and message text are all untrusted |
| File permission parsing | Shelling out to `stat` | `std::os::unix::fs::PermissionsExt::mode() & 0o077` | std only (`#[cfg(unix)]`) |

**Key insight:** the risk in this phase is not protocol complexity; it is silent data-shape drift (encrypted flag on the other endpoint, capped counts, second-granularity cursors) and secret leakage through helpers that look harmless (`Debug`, `Display`, redirects). Lean on library mechanisms that make leaks structurally impossible.

## Common Pitfalls

### Pitfall 1: Token forwarded on a cross-host redirect
**What goes wrong:** a redirecting proxy or a typo'd `http://` base sends `X-Auth-Token`/`X-User-Id` to another host.
**Why it happens:** reqwest's redirect code strips only `AUTHORIZATION`, `COOKIE`, `cookie2`, `PROXY_AUTHORIZATION`, `WWW_AUTHENTICATE` on cross-host hops [VERIFIED: reqwest-0.13.5 src/redirect.rs `remove_sensitive_headers`]. Reproduced: with the default policy the redirect target saw `("x-auth-token", "SECRET-T"), ("x-user-id", "u")`; with `Policy::none()` the client returned `307 Temporary Redirect` [VERIFIED: scratch crate run].
**How to avoid:** `Client::builder().redirect(reqwest::redirect::Policy::none())`. Map 3xx to `ApiError::Redirected` with the message "server redirected: check server_url (http vs https)". Add a wiremock test that a 307 to another origin is not followed and the second origin receives zero requests.
**Warning signs:** a plain `Client::new()` anywhere in `api/`.

### Pitfall 2: `reqwest::Error` text contains the request URL
**What goes wrong:** `format!("{e}")` prints `error sending request for url (http://127.0.0.1:1/api/info?count=5)` [VERIFIED: probe output]. With history calls that includes `roomId` and `latest`; with a userinfo URL it could include credentials.
**How to avoid:** never format a `reqwest::Error`; map it to `ApiError` in one function (`e.is_timeout()`, `e.is_connect()`, `e.is_decode()`, `e.status()`), and call `e.without_url()` if a source must be kept. A D-07 test feeds a sentinel token and a sentinel URL userinfo and asserts neither appears in `format!("{err:?}")`/`{err}` of every `ApiError`.

### Pitfall 3: `Sources` and `Config` `Debug` leak the file text
**What goes wrong:** `Sources` derives `Debug` and holds the raw `config.toml` string, now containing `token = "…"`.
**How to avoid:** hand-write `Debug` for `Sources` (print byte counts) and give `Connection` a derive-free `Debug` using `Secret`. Extend the existing sentinel-style tests (the repo already uses `SENTINEL-BODY-7f3a` patterns in `data/mod.rs` and `action.rs`) to `Sources`, `Config`, `Connection`, every new `Action` variant and every `ApiError`.

### Pitfall 4: `Action`'s hand-written `Debug` is an exhaustive match
Every new `Action` variant must add an arm in `src/action.rs` (a compile error otherwise) **and** a `None` arm in `src/input/actions.rs` where non-bindable event Actions are listed [VERIFIED: src/input/actions.rs:125-129 lists `Action::RoomsLoaded(_) | Action::FoldersLoaded(_) | Action::MessagesLoaded(..) | … => None`]. New variants: `HistoryLoaded(..)`, `HistoryFailed(RoomId, ApiError)`, `LoadFailed(ApiError)` (names are suggestions). The arms must print counts and error kinds, never message text.

### Pitfall 5: `encrypted` and `lm` are not in `subscriptions.get`
**What goes wrong:** building rooms from subscriptions alone never sets `encrypted`, so an encrypted room's history is requested (violating D-12) and last-activity sorting has no source.
**Why:** `subscriptionFields` has no `encrypted`, `lm`, `teamMain`, `teamId`; `roomFields` has `encrypted: 1`, `lm: 1`, `teamMain: 1`, `teamId: 1`, `fname: 1`, `prid: 1`, `lastMessage: 1` [VERIFIED: apps/meteor/lib/publishFields.ts at 9.0.0; `encrypted`, `lm`, `teamMain` also present at 6.5.0 and 7.0.0].
**How to avoid:** always call `rooms.get` too; fail the load if it fails.

### Pitfall 6: `count` is silently capped; `len < count` is not "end of history"
See Pattern 4. A user with `history_size = 500` against a default server gets 100 per request; a length heuristic would stop paging after the first page. Only an empty page ends history.

### Pitfall 7: Second-granularity cursor skips messages
See Pattern 4. Keep the raw server timestamp; test with two messages 300 ms apart in the same second.

### Pitfall 8: `rooms.get` carries `lastMessage` with the message text
The `rooms.get` response composes `lastMessage` (a full message object) into each room [VERIFIED: rooms.ts `composeRoomWithLastMessage`]. Do not declare it in the wire struct (serde ignores unknown fields), and never print wire structs. This also keeps message text out of the sidebar load path.

### Pitfall 9: Permission check semantics
Warn on `mode & 0o077 != 0` (group/other any bit), only when the token **comes from the file** (D-04), only for `config.toml`. Use `fs::metadata` (follows symlinks, consistent with `read_source`). Run inside `paths::read_sources`/`load()` (the only filesystem module) and pass the mode into the pure core as data. `#[cfg(unix)]`; on other targets skip. The warning text names the file and the mode (`0644`) but never contents. Also emit one stderr line **after** `tui::restore()` in `main.rs`, mirroring how config errors are printed.

### Pitfall 10: Server rate limiter (429) when opening many rooms
The REST limiter is on by default for all endpoints and answers **429** with `X-RateLimit-*` headers [CITED: developer.rocket.chat/apidocs "By default, rate limiting is enabled for all endpoints"; VERIFIED: server/api/ApiClass.ts `statusCode: 429` and `error-too-many-requests`]. The default numeric limit is configurable (docs give 10 calls per 60 000 ms only as an example; see A1). Per-room history fetch on first open can hit it when the user hops through many rooms. Map to `ApiError::RateLimited`, show it, and let reopen/`:reconnect` retry. No automatic retry loop (D-06). The local Docker fixture should disable the limiter (see Docker section), the company server cannot.

### Pitfall 11: Thread replies are hidden by default
`showThreadMessages` is passed as `showThreadMessages === 'true'`, so omitting it means thread replies are excluded [VERIFIED: channels.ts history handler `const { unreads, oldest, latest, showThreadMessages, inclusive, ...params } = this.queryParams;` … `showThreadMessages: showThreadMessages === 'true'`]. That matches "no threads" (D-15); do not pass the parameter, and note it so a verifier does not report "missing replies" as a bug.

### Pitfall 12: History can contain E2E messages in a non-encrypted room, and system messages
A room that had E2EE turned off still holds old messages with `t: "e2e"` and a `content` object instead of readable `msg` [VERIFIED: IMessage `t?: MessageTypesValues` includes `'e2e'`; `content?: EncryptedContent`]. Render those as `[encrypted message]`. System messages have `t` set (`uj`, `ul`, `au`, `ru`, `r`, `room_changed_topic`, …) [VERIFIED: IMessage MessageTypes list]; show `* <username> <verb>` for the few common ones and a generic dim `* system message` for the rest, always via `sanitize_line`.

### Pitfall 13: `users.getPreferences` can answer 400
The handler returns `API.v1.failure('ACCOUNTS_DEFAULT_USER_PREFERENCES_NOT_AVAILABLE')` when `user.settings` is missing [VERIFIED: users.ts `if (user?.settings) {…} return API.v1.failure(i18n.t('Accounts_Default_User_Preferences_not_available').toUpperCase());`]. Treat any failure of this third call as "no categories" (Favorites fallback), never as a load failure.

### Pitfall 14: Existing harness and tests assume seeded rooms
`scripts/smoke_pty.py`'s `wait_ready` "also proves that the seeded rooms were applied before the first draw" [VERIFIED: scripts/smoke_pty.py docstring]. Once `run()` stops seeding, every pty scenario needs a server to talk to. See Validation Architecture (stdlib Python mock server). Unit tests that use `App::with_fake_data` are unaffected as long as `fake` stays available under `cfg(test)`.

### Pitfall 15: A bare `host:3000` parses as a URL
`Url::parse("localhost:3000")` succeeds with scheme `localhost` and no host [VERIFIED: probe output `localhost:3000: ok scheme=localhost host=None`]. Check `scheme() ∈ {http, https}`, `host_str().is_some()`, reject `username()`/`password()` (userinfo), and decide about query/fragment (reject; `http://h/?a=1#f` parses fine). `ftp://x` also parses fine. `http://` fails with "empty host".

## Code Examples

### Client builder (verified in scratch crates)
```rust
// Source: scratch-crate probes run this session against reqwest 0.13.5
use reqwest::header::{HeaderMap, HeaderValue};
use std::time::Duration;

fn build_client(user_id: &str, token: &str) -> Result<reqwest::Client, ApiError> {
    let mut headers = HeaderMap::new();
    let mut uid = HeaderValue::from_str(user_id).map_err(|_| ApiError::Client)?;
    let mut tok = HeaderValue::from_str(token).map_err(|_| ApiError::Client)?;
    uid.set_sensitive(true);
    tok.set_sensitive(true);               // Debug prints `Sensitive`
    headers.insert("X-User-Id", uid);
    headers.insert("X-Auth-Token", tok);
    reqwest::Client::builder()
        .default_headers(headers)
        .redirect(reqwest::redirect::Policy::none()) // token is a custom header; reqwest would forward it
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .user_agent(concat!("rockettui/", env!("CARGO_PKG_VERSION"))) // generic, no personal details
        .build()
        .map_err(|_| ApiError::Client)
}
```
`HeaderValue::from_str` fails on control characters in the token; that surfaces as a `Client` error with a fixed message, not the value.

### Endpoint URL and query building (verified)
```rust
// Source: probe output: base "https://host.example/chat" -> https://host.example/chat/api/v1/rooms.get
fn endpoint(base: &reqwest::Url, segs: &[&str]) -> reqwest::Url {
    let mut u = base.clone();
    if let Ok(mut s) = u.path_segments_mut() { s.pop_if_empty().extend(segs); }
    u
}
// history: client.get(endpoint(&base, &["api","v1","channels.history"]))
//     .query(&[("roomId", id), ("count", &count.to_string())])   // + ("latest", oldest_ts) for older pages
```
Verified encodings: `roomId=a+b%26c%3Dd`, `latest=2026-11-16T10%3A00%3A00.123Z`.

### URL validation
```rust
// Source: probe results; rules from D-02
fn parse_server_url(text: &str) -> Result<reqwest::Url, &'static str> {
    let url = reqwest::Url::parse(text.trim()).map_err(|_| "must be an http:// or https:// URL")?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("must be an http:// or https:// URL");
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("must not contain a user name or password");
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("must not contain a query or fragment");
    }
    Ok(url)
}
```

### Secret wrapper (std only)
```rust
// Source: project pattern (redacting Debug on Message, src/data/mod.rs) applied to credentials
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);
impl Secret {
    pub fn new(value: String) -> Self { Secret(value) }
    pub fn expose(&self) -> &str { &self.0 }  // used only by api::build_client
}
impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str("Secret(<redacted>)") }
}
// Deliberately NO Display impl: `{}` of a Secret does not compile.
```

### Timestamp parsing that keeps milliseconds (verified)
```rust
// Source: scratch crate with the project's chrono feature set
fn parse_ts(s: &str) -> Option<(i64 /*secs*/, i64 /*ms*/)> {
    let d = chrono::DateTime::parse_from_rfc3339(s).ok()?;
    Some((d.timestamp(), d.timestamp_millis()))
}
// "2026-11-16T10:00:00.123Z" -> (1794823200, 1794823200123)
```
Make the wire date type lenient: `#[serde(untagged)] enum ApiDate { Iso(String), Ejson { #[serde(rename = "$date")] ms: i64 } }` (REST normally returns ISO strings; the EJSON form appears in DDP and some docs examples, see A2). A bad date on one message should map to `ts = 0`, not fail the whole page.

### Wire types (shape from Rocket.Chat 9.0.0 typings; all optional except ids)
```rust
// Source: packages/core-typings ISubscription.ts / IRoom.ts / IMessage.ts and publishFields.ts at tag 9.0.0
#[derive(Deserialize)] struct SubscriptionsResp { update: Vec<WireSub> }          // + `remove`, ignored
#[derive(Deserialize)] struct WireSub { rid: String, t: String, name: Option<String>, fname: Option<String>,
    #[serde(default)] open: bool, #[serde(default)] f: bool, #[serde(default)] unread: u32,
    prid: Option<String>, category: Option<String>, status: Option<String>, ts: Option<ApiDate> }
#[derive(Deserialize)] struct RoomsResp { update: Vec<WireRoom> }
#[derive(Deserialize)] struct WireRoom { #[serde(rename = "_id")] id: String, encrypted: Option<bool>,
    #[serde(rename = "teamMain")] team_main: Option<bool>, prid: Option<String>, lm: Option<ApiDate> } // no lastMessage
#[derive(Deserialize)] struct HistoryResp { messages: Vec<WireMsg> }
#[derive(Deserialize)] struct WireMsg { #[serde(rename = "_id")] id: String, #[serde(default)] msg: String,
    ts: ApiDate, u: WireUser, t: Option<String>, content: Option<serde_json::Value>,
    file: Option<WireFile>, #[serde(default)] files: Vec<WireFile>, #[serde(default)] attachments: Vec<WireAttach> }
// WireUser { username: Option<String> }, WireFile { name: Option<String> }, WireAttach { title: Option<String> }
// preferences: { "preferences": { "sidebarCategories": [ { "_id", "name", "default"? } ] } }
```
`subscriptions.get` answers `{ update: [...], remove: [], success: true }` (the array result is wrapped; shape unchanged from 6.5.0 through 9.0.0) [VERIFIED: server/api/v1/subscriptions.ts at 9.0.0 and 6.5.0/7.0.0/7.10.0/8.0.0 `update: result, remove: []`]. `rooms.get` answers `{ update: [...], remove: [...], success }`. History answers `{ messages: [...], success }` (plus `firstUnread`/`unreadNotLoaded` only when `unreads=true`).

### Folders from categories, else Favorites
```rust
// Source: docs/features/custom-sidebar-categories.md at 9.0.0 (assignment is exclusive per room)
// categories = prefs.sidebarCategories.filter(|c| !c.default)   (array order = render order)
// folders = [Favorites{ rooms with f && category.is_none() }]  if non-empty
//         ++ for each category: Folder{ id: category._id, name: category.name, room_ids: subs with category == _id }
// Without a license nobody can assign a category, so folders degrade to the single Favorites folder (D-10).
```

### Command row (existing table)
```rust
// Source: src/app/command.rs COMMANDS table (names: &["q", "quit"], run: cmd_quit)
CommandDef { names: &["reconnect", "retry"], run: cmd_reconnect }
// cmd_reconnect: app.conn = ConnState::Connecting; app.effects.push(Effect::LoadRooms);
//                clear HistoryState::Failed entries so reopening retries (D-16); Ok(())
```

## Sidebar Categories: findings for D-10

- **Minimum version:** 8.9.0. At tag 8.8.0 `docs/features/custom-sidebar-categories.md` returns 404 and `publishFields.ts` has no `category: 1`; at 8.9.0 and 8.9.1 the doc exists and `subscriptionFields` includes `category: 1` [VERIFIED: raw.githubusercontent.com checks this session].
- **License:** "Requires the `experimental-enterprise-features` license module." [CITED: docs/features/custom-sidebar-categories.md @9.0.0]. `POST /experimental/rooms.setCategory` "Requires `experimental-enterprise-features` license." Categories are per-user and "client-only in meaning"; they "carry no access, security or membership semantics" and "Assignment is exclusive — a room lives in exactly one place at a time (one custom category, *or* Favorites, *or* its system group)."
- **There is no read endpoint of its own.** The ordered category list is the user preference `sidebarCategories: ISidebarCategory[]` (`{ _id, name, default?, showUnreads?, keepUnreadsOnTop? }`), read via `GET /api/v1/users.getPreferences` → `preferences.sidebarCategories`; entries with `default: true` are system-group metadata and are filtered out by the web client (`rawCategories.filter((entry) => !entry.default)`). A room's membership is `subscription.category` (present in `subscriptions.get`) [VERIFIED: ISubscription.ts `category?: string;`, publishFields `category: 1`, users.ts `users.getPreferences`, useUserSidebarCategories.ts].
- **Reachable without a license?** The `users.getPreferences` handler has no license check [VERIFIED: users.ts handler body], so the app can always read it. The web client itself ignores categories when not enterprise (`isEnterprise ? { rawCategories, customCategories } : { rawCategories, customCategories: [] }`). Writing the preference works without a license (the persistence tests are not behind `IS_EE`) [CITED: apps/meteor/tests/end-to-end/api/sidebar-custom-categories.ts], but assigning rooms does not. So on an unlicensed server both the list and every `subscription.category` are empty or absent, which is the Favorites fallback automatically; a lapsed license could leave stale categories, which is accepted.
- **Recommended rule (extends D-10, needs confirmation, see A6):** `Folders = [Favorites (if any)] + custom categories (array order)`; rooms in a category are not in Favorites (exclusive), so no duplicates. Dropping Favorites whenever categories exist would push favorited rooms into Recent, a regression against the official client. If the author wants the literal D-10, use categories only when at least one custom category exists, else Favorites.
- **Local testing without a license:** set `sidebarCategories` through `POST /api/v1/users.setPreferences { data: { sidebarCategories: [...] } }` and set `category` on the subscription directly in MongoDB (see Docker section). Unit-test the folder builder with JSON fixtures regardless.

## Docker Rocket.Chat fixture (DLVR-01)

**Not executed in this session** (Docker is available, but port 3000 is already in use and the daemon hosts unrelated containers, so no images were pulled). Everything below is assembled from Rocket.Chat source and docs; the plan must include a `checkpoint:human-verify` that brings it up once and records the fixtures.

- **Images:** `docker.io/rocketchat/rocket.chat:8.9.1` (tag exists; has Categories) and `docker.io/mongodb/mongodb-community-server:8.0-ubi8` (tag exists). Rocket.Chat 9.0.0 (released 2026-11-10) lists "MongoDB: `8.0`"; server code treats Mongo `<7.0.0` as unsupported and `<8.0.0` as deprecated [VERIFIED: GitHub release notes 9.0.0; serverRunning.ts `semver.satisfies(mongoSemver, '<7.0.0')`]. Make the tag a variable (`RELEASE`) and do one compatibility pass against 9.0.0 (it changes REST failure status codes; the status-first mapping above covers it).
- **Replica set:** Rocket.Chat needs a MongoDB replica set. The official `rocketchat-compose` starts `mongod --replSet rs0 --bind_ip_all` and initiates it with `rs.initiate({_id: "rs0", members: [{ _id: 0, host: "mongodb:27017" }]})` from an init container [CITED: github.com/RocketChat/rocketchat-compose compose.database.yml]. A single-container healthcheck that initiates on first run is a simpler equivalent. The CI compose runs one Rocket.Chat container with `MONGO_URL=mongodb://mongo:27017/rocketchat?replicaSet=rs0` and an empty `TRANSPORTER`, so NATS is not required for a monolith [CITED: Rocket.Chat docker-compose-ci.yml @9.0.0].
- **Host port:** use `127.0.0.1:${HOST_PORT:-3100}:3000`; **port 3000 is already listening on this machine** [VERIFIED: `ss -ltn` showed `*:3000`]. Set `ROOT_URL=http://localhost:3100`.
- **Admin without the wizard:** env vars `ADMIN_USERNAME`, `ADMIN_PASS`, `ADMIN_EMAIL`, `ADMIN_NAME` create an admin on first start only when no admin exists; the username defaults to `admin` [VERIFIED: server/startup/initialData.ts `insertAdminUserFromEnv`]. `OVERWRITE_SETTING_<SettingId>` sets any setting from env (docs example `OVERWRITE_SETTING_SMTP_Host`) [CITED: docs.rocket.chat deploy-with-docker-docker-compose]. Useful overrides for a throwaway dev server: `OVERWRITE_SETTING_Show_Setup_Wizard=completed` (setting `Show_Setup_Wizard`, default `'pending'` [VERIFIED: settings/general.ts:38]; value `completed` is A7), `OVERWRITE_SETTING_Accounts_TwoFactorAuthentication_Enabled=false`, `OVERWRITE_SETTING_API_Enable_Rate_Limiter=false` [VERIFIED: setting ids read in settings/accounts.ts and api/ApiClass.ts `settings.get<boolean>('API_Enable_Rate_Limiter')`].
- **Why disable 2FA:** `POST /api/v1/users.generatePersonalAccessToken` is declared `twoFactorRequired: true`; `checkCodeForUser` returns early when `Accounts_TwoFactorAuthentication_Enabled` is false, and `Accounts_TwoFactorAuthentication_Enforce_Password_Fallback` defaults to `true`, so with 2FA on even a fresh user needs `x-2fa-code` (sha-256 of the password) and `x-2fa-method: password` unless inside the short remember window after account creation [VERIFIED: users.ts:1352-1366; 2fa/code/index.ts; settings/accounts.ts:92].
- **PAT creation:** needs permission `create-personal-access-tokens`, default roles `['admin', 'user']` [VERIFIED: constant/permissions.ts:26]; body `{ "tokenName": "...", "bypassTwoFactor": true }`; response `{ "token": "…" }`; the token is shown only once; names must be unique (`error-token-already-exists`) [VERIFIED: users.ts tokenNameBodySchema, generateToken.ts]. Use it with `X-User-Id: <userId>` and `X-Auth-Token: <token>` [CITED: docs.rocket.chat manage-personal-access-tokens: "you must provide two headers: `x-User-Id` and `x-Auth-Token`"]. The user id comes from the login response `data.userId` (and `/api/v1/me` `_id`).
- **Default permissions** let a normal `user` create channels (`create-c`), private groups (`create-p`) and DMs (`create-d`) [VERIFIED: permissions.ts:22-25], so the seed can run as a normal user, not only admin.
- **Seed script (recommended: `dev/seed.py`, Python stdlib only, run with `python3 -I`, matching `scripts/smoke_pty.py`):**
  1. Poll `GET /api/info` (no auth, returns `{ "version": "...", "success": true }` [VERIFIED: server/api/default/info.ts]) until 200.
  2. `POST /api/v1/login {"user","password"}` as admin; `POST /api/v1/users.create` for `dev` (the user whose PAT the app uses), `alice`, `bob` with `{email,name,password,username,verified:true}` (required: email, name, password, username [VERIFIED: UserCreateParamsPOST.ts]).
  3. As `dev`: create a public channel, a private group (`groups.create`), a DM with alice (`im.create {username}` [VERIFIED: DmCreateProps `username: string`]), post more than 120 messages in one channel to exercise paging (`chat.postMessage`), post a few as `alice` to get a second author. Body shapes for `channels.create`, `groups.create`, `chat.postMessage`, `rooms.favorite` are A3.
  4. Encrypted room: create a private group `secrets`, then set `encrypted: true` directly: `docker compose exec mongodb mongosh --quiet rocketchat --eval 'db.rocketchat_room.updateOne({name:"secrets"},{$set:{encrypted:true}})'`. `rooms.get` reads room documents through a projection that includes `encrypted`, so the DB write is visible immediately [VERIFIED: publishFields `encrypted: 1`; roomsGetMethod reads `Rooms.findBySubscriptionUserId`]. (The API route `rooms.saveRoomSettings` with `E2E_Enable` is the alternative; A8.)
  5. Favorite one room (`f`), mark one room unread by posting as alice after `dev` last read, close one DM (`open:false`) to prove it is skipped.
  6. Create the PAT (`bypassTwoFactor: true`) and write `ROCKETTUI_SERVER_URL`, `ROCKETTUI_USER_ID`, `ROCKETTUI_TOKEN` to `dev/.env.local` with mode 0600; add `dev/.env.local` to `.gitignore`. The token must never be printed to a shared log or committed (CLAUDE.md).
  7. Categories fixture (optional, license-free): `users.setPreferences` with `sidebarCategories`, then `docker compose exec mongodb mongosh … 'db.rocketchat_subscription.updateOne({rid:"<id>","u._id":"<devId>"},{$set:{category:"<catId>"}})'`.
- **Fixture capture:** after seeding, `curl` the four endpoints with the PAT and store the JSON (with message text replaced by neutral lorem) under `src/api/fixtures/`. These become the unit-test inputs, so the mapping is tested against real server output, not only against hand-written JSON.

Compose skeleton (assembled; see A4):
```yaml
services:
  mongodb:
    image: docker.io/mongodb/mongodb-community-server:8.0-ubi8
    command: ["mongod", "--replSet", "rs0", "--bind_ip_all"]
    volumes: ["mongodb_data:/data/db"]
    healthcheck:
      test: ["CMD", "mongosh", "--quiet", "--eval",
             "try { rs.status().ok } catch (e) { rs.initiate({_id:'rs0',members:[{_id:0,host:'mongodb:27017'}]}).ok }"]
      interval: 5s
      timeout: 10s
      retries: 20
  rocketchat:
    image: docker.io/rocketchat/rocket.chat:${RELEASE:-8.9.1}
    depends_on: { mongodb: { condition: service_healthy } }
    environment:
      ROOT_URL: http://localhost:${HOST_PORT:-3100}
      PORT: "3000"
      MONGO_URL: mongodb://mongodb:27017/rocketchat?replicaSet=rs0
      ADMIN_USERNAME: admin
      ADMIN_PASS: ${ADMIN_PASS:?set a throwaway admin password}
      ADMIN_EMAIL: admin@example.invalid
      OVERWRITE_SETTING_Show_Setup_Wizard: completed
      OVERWRITE_SETTING_Accounts_TwoFactorAuthentication_Enabled: "false"
      OVERWRITE_SETTING_API_Enable_Rate_Limiter: "false"
    ports: ["127.0.0.1:${HOST_PORT:-3100}:3000"]
volumes: { mongodb_data: {} }
```
Bind to `127.0.0.1` only; the throwaway admin password comes from the environment, not the file.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| reqwest default TLS = native-tls (OpenSSL) | reqwest 0.13: rustls default, aws-lc-rs provider, `rustls-platform-verifier` roots | 0.13.0 (2025-12-30) | "rustls defaults" (D-08) needs no feature flags; old feature name `rustls-tls` is gone (now `rustls`); `json`/`query` are opt-in |
| `getChannelHistory`/`loadHistory` DDP methods | REST `*.history` (and new `rooms.history` in 9.0.0) | 9.0.0 removes 157 Meteor methods | Confirms REST is the right path; `rooms.history` too new to depend on |
| 400 for missing session / no permission | 401 / 403 with `error-unauthenticated` / `error-forbidden`; 401 bodies drop `status`, `message` | 9.0.0 (2026-11-10) | Map on HTTP status, not body text; older servers still send 401 for bad tokens |
| No sidebar categories | Per-user Categories (Enterprise `experimental-enterprise-features`) | 8.9.0 | `subscription.category` + `sidebarCategories` preference |

**Deprecated/outdated:** the `developer.rocket.chat/apidocs/get-subscriptions` page documents the DDP method (`subscriptions/get`, `$date` objects, array result) and is marked stale; the REST route wraps the array as `{ update, remove }`. Do not copy its examples. `Bitnami` MongoDB images used in old compose files are not recommended; use `mongodb/mongodb-community-server`.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Default REST rate limit is 10 calls per 60 s per endpoint. Docs only give 10/60000 as an *example* value; source reads the numbers from settings | Pitfall 10 | Low; the client maps 429 generically. Affects only how soon a user hits it |
| A2 | REST JSON dates are ISO-8601 strings (standard `JSON.stringify` of `Date`); EJSON `{"$date": ms}` is the DDP form | Code Examples (ApiDate) | Low: the lenient untagged enum handles both; confirm with the captured fixtures |
| A3 | Request bodies for `channels.create`, `groups.create`, `chat.postMessage` (`roomId`,`text`) and `rooms.favorite` (`roomId`,`favorite`) used by the seed script | Docker section | Low: seed fails loudly on first run; fix then |
| A4 | The compose skeleton starts cleanly as written (command override of the Mongo image, healthcheck-initiated replica set) | Docker section | Medium for schedule only; plan has a human-verify checkpoint |
| A5 | Endpoints and fields are also present on servers older than 6.5.0 (verified: `encrypted`, `lm`, `teamMain` in `publishFields` at 6.5.0, 7.0.0, 9.0.0; `subscriptions.get` wrapper shape 6.5.0–9.0.0; `rooms.get` added in 0.72.0 per docs) | Summary | Low; company server version is unknown until Phase 6 |
| A6 | Folders = Favorites (if any) + custom categories, instead of the literal D-10 "categories OR Favorites" | Sidebar Categories | Medium: user-visible; needs author confirmation (Open Question 1) |
| A7 | `Show_Setup_Wizard` accepts the value `completed` | Docker section | Low: if not, the wizard shows in the browser but REST/PAT flow is unaffected |
| A8 | `rooms.saveRoomSettings { rid, encrypted: true }` marks a room encrypted when `E2E_Enable` is on (the MongoDB update is the recommended path and verified-by-reading) | Docker section | Low |
| A9 | The `name` of a DM subscription is the other participant's username (group DMs: comma-joined usernames); `fname` may be the real name | Pattern 3 | Low-Medium: D-09 already decided "other user's username"; confirm with the DM fixture |
| A10 | Mapping text for system message types beyond `uj`/`ul` (generic fallback `* system message`) is acceptable to the author | Pitfall 12 | Low |
| A11 | Treating a missing matching `rooms.get` record as "skip the subscription" is acceptable | Pattern 3 | Low; only hits a race between the two calls |

## Open Questions

1. **D-10 interpretation: union or literal?**
   - What we know: assignment to Favorites vs a category is exclusive in the official client; the union loses nothing.
   - What's unclear: the author wrote "categories, otherwise Favorites".
   - Recommendation: implement the union (A6) and mention it in the plan summary; one `if` flips it to the literal reading.
2. **`Message` needs a way to mark system messages (D-15).**
   - What we know: `Message` has no kind; `Message { … }` literals appear in 10 places (`app/update.rs`, `app/messages.rs`, `data/fake.rs`, `data/mod.rs`, `ui/messages.rs` ×2, `ui/mod.rs`, `action.rs`, `ui/wrap.rs` ×4).
   - Recommendation: add `pub kind: MessageKind` (`Normal | System`) and update the literals mechanically (or add a `Message::normal(..)` helper first so later literals are short). Rendering switches to the `messages.notice` style for `System`. Do not fake it with an empty author.
3. **`history_size` larger than the server cap.**
   - Recommendation: send `count = history_size`, accept a shorter page, end on an empty page (decided above). If the author wants "always exactly `history_size` messages", the loop-until-full variant is a small later addition.
4. **Company server version and license state are unknown** (Phase 6). Nothing here blocks Phase 3; the 8.9.1 vs 9.0.0 compatibility pass in the Docker fixture is the proxy.
5. **Does `:reconnect` also refresh cached history?** Recommendation: no; it reloads rooms and clears only `Failed` entries (D-13: cached history is refreshed by realtime in Phase 5).

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain (`cargo`, `rustc`, `clippy`) | Build and tests | ✓ | cargo 1.99.1 / rustc 1.99.1 / clippy 0.1.99 (project `rust-version` 1.88) | — |
| Docker daemon + `docker compose` | DLVR-01 fixture | ✓ | Docker 29.9.2, Compose 5.6.0 (daemon running, unrelated containers present) | `podman` 5.8.9 is installed |
| Free host port 3000 | Default Rocket.Chat port | ✗ (in use) | — | Use `HOST_PORT=3100` (already the recommended default) |
| Docker Hub access for `rocketchat/rocket.chat:8.9.1`, `mongodb/mongodb-community-server:8.0-ubi8` | Fixture bring-up | ✓ tags exist (HTTP 200 / listed) | — | `registry.rocket.chat/rocketchat/rocket.chat` |
| crates.io access | New dependencies | ✓ (scratch builds succeeded) | — | — |
| Python 3 (stdlib only) | `dev/seed.py`, `scripts/smoke_pty.py`, mock server | ✓ | 3.14.10 | — |
| `curl`, `jq` | Fixture capture | ✓ | curl 8.15.0, jq 1.8.1 | Python `urllib` |
| `mongosh` | Encrypted-room and category fixtures | via `docker compose exec mongodb mongosh` | in the Mongo image | — |

**Missing dependencies with no fallback:** none.
**Missing dependencies with fallback:** host port 3000 (use 3100).

## Validation Architecture

Nyquist validation is enabled (`workflow.nyquist_validation: true` in `.planning/config.json`).

### Test Framework
| Property | Value |
|----------|-------|
| Framework | libtest (`cargo test`) + `insta` 1 snapshots; pty e2e via `scripts/smoke_pty.py` (stdlib Python) [VERIFIED: 02-VALIDATION.md "Test Infrastructure"] |
| Config file | none (`insta = "1"` in `[dev-dependencies]`); add `wiremock = "0.6"` |
| Quick run command | `cargo test --lib` |
| Full suite command | `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check` |
| e2e command | `cargo build && python3 -I scripts/smoke_pty.py target/debug/rockettui <scenario>` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| AUTH-01 | env overrides file per key; file-only works; URL/user/token missing → error problems that name key and place; bad/userinfo/non-http URL rejected; empty env = unset | unit (pure core) | `cargo test --lib -- config::connection config::tests` | ❌ Wave 0 |
| AUTH-01 | client sends `X-User-Id` and `X-Auth-Token` on every call | unit (wiremock header matchers) | `cargo test --lib -- api::tests::sends_auth_headers` | ❌ Wave 0 |
| AUTH-02 | 0o644 / 0o640 / 0o604 config with token in file → warning; 0o600 → none; token from env + 0o644 → none; no token → none | unit (temp dir + `set_permissions`) | `cargo test --lib -- config::paths::tests::perm_` | ❌ Wave 0 |
| AUTH-02 | warning is persistent across key presses and drawn in the status bar; stderr line after exit | reducer + snapshot; pty | `cargo test --lib -- ui::status::tests::perm_warning` ; `python3 -I scripts/smoke_pty.py … perm_warning` | ❌ Wave 0 |
| AUTH-03 | sentinel token and sentinel URL userinfo absent from `Debug`/`Display` of `Secret`, `Connection`, `Sources`, `Config`, every `ApiError`, every new `Action` variant; sentinel message body absent from `Debug` of wire/domain/Action | unit | `cargo test --lib -- redaction` | ❌ Wave 0 |
| AUTH-03 | 401 against the real binary: screen and stderr contain no token | pty with Python mock server | `python3 -I scripts/smoke_pty.py … unauthorized` | ❌ Wave 0 |
| AUTH-03 | cross-origin 307 is not followed; second origin gets 0 requests | unit (two wiremock servers) | `cargo test --lib -- api::tests::no_redirect_follow` | ❌ Wave 0 |
| ROOM-01 | fixture JSON (subs + rooms) → rooms with right kinds (channel, private, DM, discussion, team), names, unread bool, sorted by `lm`; `open:false` and `status` and `t:"l"` skipped | unit (fixtures from the Docker seed) | `cargo test --lib -- api::map::tests` | ❌ Wave 0 |
| ROOM-01 | folders: Favorites only when no categories; categories in order; exclusive membership; `getPreferences` 400 → Favorites | unit | `cargo test --lib -- api::map::tests::folders` | ❌ Wave 0 |
| ROOM-02 | `Select` on a room opens it and pushes exactly one `Effect::LoadHistory`; reopen does not; Failed reopen retries | reducer unit | `cargo test --lib -- app::history::tests` | ❌ Wave 0 |
| ROOM-03 | encrypted room: marker in sidebar, placeholder "encrypted, not supported", **no effect pushed and no HTTP request** (wiremock `expect(0)`) | reducer + snapshot + wiremock | `cargo test --lib -- app::history::tests::encrypted ui::messages::tests::encrypted` | ❌ Wave 0 |
| MSG-01 | history response (newest first) → oldest-first messages; system/attachment/e2e text; thread replies not requested | unit (fixture) | `cargo test --lib -- api::map::tests::history` | ❌ Wave 0 |
| MSG-01 | older page: `latest` equals the raw ts of the oldest loaded message; cursor stays on the same message after prepend; empty page → exhausted → no further request; `count` > cap handled | reducer unit + wiremock query matchers | `cargo test --lib -- app::history::tests::older api::tests::latest_param` | ❌ Wave 0 |
| MSG-01 | Loading… / Could not load history / empty-room placeholders | snapshot | `cargo test --lib -- ui::messages::tests` | ❌ Wave 0 |
| D-06 | 401/unreachable/timeout/429/3xx → distinct `ApiError`, status-bar text, `:reconnect` and `:retry` re-run the load; `:q` still works | unit + reducer | `cargo test --lib -- api::tests::error_mapping app::command::tests::reconnect` | ❌ Wave 0 |
| DLVR-01 | rooms, history, scrolling and encrypted placeholder work against the Docker server | manual UAT (human-verify at end of phase) with `dev/` fixture; automated twin against the mock server | `python3 -I scripts/smoke_pty.py … connect_read` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test --lib`
- **Per wave merge:** `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check`
- **Phase gate:** full suite green, pty scenarios `quit`, `panic`, `insert`, `config_error`, `config_keymap` (existing, harness adapted) plus new `connect_read`, `unauthorized`, `perm_warning`; then the manual Docker UAT.

### Wave 0 Gaps
- [ ] `wiremock = "0.6"` dev-dependency; `reqwest`, `serde`, `serde_json` dependencies.
- [ ] `src/api/fixtures/*.json` captured from the seeded Docker server (needs the `dev/` compose + seed first; hand-written interim fixtures are fine for the tracer slice, replace before phase end).
- [ ] `scripts/smoke_pty.py`: `wait_ready` must stop depending on seeded rooms. Add a stdlib `http.server` mock Rocket.Chat (serves the fixtures, checks `X-Auth-Token`) started by the harness on an ephemeral port and passed via `ROCKETTUI_SERVER_URL`, `ROCKETTUI_USER_ID`, `ROCKETTUI_TOKEN`; existing scenarios run against it.
- [ ] A sentinel-based redaction test module covering the new types (pattern exists in `src/data/mod.rs` and `src/action.rs`).
- [ ] Rewrite `config::tests::server_url_is_unknown_key` (now a known key).
- [ ] `dev/compose.yml`, `dev/seed.py`, `dev/README.md`, `.gitignore` entry for `dev/.env.local`.

## Security Domain

`security_enforcement` is enabled in `.planning/config.json` (`security_asvs_level: 1`, `security_block_on: high`).

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | yes (client side) | PAT only, sent as `X-User-Id`/`X-Auth-Token`; never in URL; token in `Secret` then a sensitive `HeaderValue`; no password handling |
| V3 Session Management | no | Stateless REST with a long-lived PAT; no session cookies (reqwest `cookies` feature off) |
| V4 Access Control | no (server enforces) | Client maps 401/403 to messages; no privilege logic |
| V5 Input Validation | yes | Server JSON is untrusted: lenient serde, bounded sizes, `sanitize_line` for every name and body; URL/config validation with typed errors; header-value construction can fail safely |
| V6 Cryptography | yes (delegated) | TLS via rustls with platform verifier and standard verification (D-08); no custom crypto; never disable verification |
| V7 Error Handling and Logging | yes | Typed generic `ApiError`; no log file in this phase; no URL, token or message text in any error, `Debug` or `Display` |
| V9 Communications | yes | `redirect::Policy::none()`; http allowed only because D-02 permits it for local Docker (warn the user in docs that the token travels in clear on `http://` off-localhost) |
| V14 Configuration | yes | env over file; `chmod 600` warning (AUTH-02); the seed script writes `dev/.env.local` at 0600 and git-ignores it |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Token forwarded on cross-host redirect | Information Disclosure | `redirect::Policy::none()` (reproduced leak with default policy) |
| Token or URL in `Debug`/`Display` (struct, error, Action) | Information Disclosure | `Secret` with redacting `Debug` and no `Display`; `set_sensitive(true)`; typed `ApiError`; sentinel tests |
| Raw `config.toml` text in `Sources` `Debug` | Information Disclosure | Hand-written `Debug`; existing "never use toml `Display`" rule extends to `token` and `user_id` |
| Terminal escape injection via room/user/message text | Tampering | Existing `ui::sanitize` on every drawn string; new placeholders and system lines also pass through it |
| Malicious or buggy server returns a huge or malformed body | Denial of Service | Total timeout 15 s; cap the body (read with a size limit, e.g. 8 MiB, via `content_length` pre-check plus bounded `chunk()` loop); `BadResponse` on decode failure; one bad message date or field does not fail the page |
| Cleartext token on `http://` | Information Disclosure | Allowed by D-02 for local Docker; document it; consider a one-line status warning when scheme is `http` and the host is not loopback (optional hardening, not required by the decisions) |
| Credential in `server_url` userinfo | Information Disclosure | Reject userinfo at validation; never echo the URL in errors |
| Token committed or leaked by the dev fixture | Information Disclosure | `dev/.env.local` mode 0600 and git-ignored; seed prints no token to stdout beyond writing the file |

## Sources

### Primary (HIGH confidence)
- Rocket.Chat source at tag 9.0.0 (read from raw.githubusercontent.com this session): `apps/meteor/server/api/v1/{subscriptions,rooms,im,chat,channels,groups,users}.ts`, `server/publications/{subscription,room}/index.ts`, `lib/publishFields.ts`, `server/api/lib/getPaginationItems.ts`, `server/api/ApiClass.ts`, `server/api/default/info.ts`, `server/meteor-methods/messages/getChannelHistory.ts`, `server/startup/{initialData,serverRunning}.ts`, `server/settings/{general,accounts}.ts`, `server/lib/2fa/{twoFactorRequired,code/index,code/PasswordCheckFallback}.ts`, `server/lib/authorization/constant/permissions.ts`, `packages/core-typings/src/{ISubscription,IRoom,RoomType,IUser,IMessage/*}.ts`, `packages/rest-typings/src/v1/{channels/ChannelsHistoryProps,users/UserCreateParamsPOST,dm/DmCreateProps}.ts`, `docs/features/custom-sidebar-categories.md`, `apps/meteor/tests/end-to-end/api/sidebar-custom-categories.ts`, `docker-compose-ci.yml`; tags 6.5.0, 7.0.0, 7.10.0, 8.0.0, 8.8.0, 8.9.0, 8.9.1 spot-checked.
- GitHub release notes for Rocket.Chat 9.0.0 (status-code changes, MongoDB 8.0, `rooms.history`).
- reqwest v0.13.5: `Cargo.toml` `[features]`, `CHANGELOG.md`, `src/redirect.rs` (`remove_sensitive_headers`); crates.io API for versions and `rust_version`.
- Scratch-crate experiments run this session: sensitive header Debug redaction, error text containing the URL, cross-host redirect header leak and `Policy::none()`, URL building/validation behaviors, chrono rfc3339 under the project feature set, wiremock with reqwest 0.13.5, clean rustls build.
- `gsd-tools query package-legitimacy check` for all crates in the audit table.
- Repository files read this session: `.planning/phases/03-connect-read/03-CONTEXT.md`, `REQUIREMENTS.md`, `STATE.md`, `.claude/CLAUDE.md`, `Cargo.toml`, `src/{lib,main,action,event,test_util}.rs`, `src/app/{mod,update,command,messages,sidebar}.rs`, `src/data/{mod,fake}.rs`, `src/config/{mod,settings,diag,paths,theme}.rs`, `src/ui/{status,messages}.rs`, `src/input/actions.rs`, `scripts/smoke_pty.py`, `.planning/phases/02-configuration-system/02-VALIDATION.md`.

### Secondary (MEDIUM confidence)
- docs.rocket.chat: manage-personal-access-tokens (headers, "Ignore Two Factor Authentication"), deploy-with-docker-docker-compose (compose structure, `OVERWRITE_SETTING_`, Mongo version), rate-limiter page.
- developer.rocket.chat/apidocs: authentication-api and rate-limiting sections (headers; "By default, rate limiting is enabled for all endpoints"; `x-ratelimit-*` headers).
- github.com/RocketChat/rocketchat-compose (`compose.database.yml`, `compose.yml`, `.env.example`).
- Docker Hub tag listings for `rocketchat/rocket.chat` and `mongodb/mongodb-community-server`.

### Tertiary (LOW confidence)
- developer.rocket.chat `get-subscriptions` page (documents the DDP method, flagged stale; used only to learn it was the wrong shape).
- Training knowledge for the seed request bodies marked `[ASSUMED]` in the Assumptions Log.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH. Versions from the registry; features read from the crate manifest; build and test exercised in a scratch crate.
- Architecture: HIGH for fit with the existing code (every integration point read); MEDIUM for the exact effect/state shape, which is a recommendation.
- Pitfalls: HIGH. The redirect leak, error-URL text, capped counts and the subscription/room field split are each backed by source or a run.
- Docker seed: MEDIUM. Env vars, permissions, 2FA and image tags verified; the compose file and a few REST bodies are not executed.

**Research date:** 2026-11-16
**Valid until:** 2026-12-16 (Rocket.Chat 9.0.0 is six days old; re-check REST status-code behavior and the 8.9.x line before Phase 6)
