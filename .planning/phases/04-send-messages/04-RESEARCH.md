# Phase 4: Send Messages - Research

**Researched:** 2026-11-17
**Domain:** Rocket.Chat REST send path (reqwest + serde) inside an existing Elm/TEA ratatui app: optimistic pending messages, per-room serialized sends, idempotent retry, per-room drafts, user-facing vocabulary (keymap, theme, config)
**Confidence:** HIGH for the server contract (read from the Rocket.Chat 8.9.1 and 9.0.0 source and official docs) and for the in-repo seams (all read this session); MEDIUM overall because the live duplicate-`_id`, read-only, muted and oversize probes could NOT be run (see "Live verification status")

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Send flow & echo**
- **D-01:** Sending is **optimistic**. Enter appends the message immediately (extending the existing `local-N` path) drawn **dim with a "sending" marker**, then it is replaced by the server's message on success. The marker is a new `[markers]`/theme element; document any new `theme.toml` names (user-facing vocabulary, Phase 2).
- **D-02:** The input is **cleared on Enter** and stays in Insert mode so the user can keep typing.
- **D-03:** Sends are **queued in order per room** (serialized), so ordering is preserved on slow networks. Several can be pending at once.
- **D-04:** On success the pending message is **swapped in place** for the message returned by `chat.postMessage` (server `_id`, server `ts`), keeping its position. **No history refetch.**
- **D-05:** After a send, the messages pane follows the newest message. Behavior is **configurable**: a new `config.toml` setting chooses between *always jump to bottom on send* and *only follow if already at the bottom*. Both must be implemented; default is always-follow (existing `follow_newest`). Key name/values are Claude's discretion but are user-facing vocabulary, validated with a clear config error on bad values.

**Failure & retry**
- **D-06:** A failed send (401, unreachable, TLS, timeout, 4xx/5xx) leaves the pending message in the pane marked **failed** (theme marker) **plus** a status-bar error (`status.error`, Phase 3 D-06). A Normal-mode key on that message **retries** it; another key **discards** it. Key names go through the Phase 2 keymap vocabulary (new curated action names, user-rebindable; document them).
- **D-07:** Idempotence: the client generates the message `_id` and passes it in `chat.postMessage`; a retry reuses the same `_id`. A **timeout/unknown outcome is shown as "unconfirmed"**, not "failed", since the server may have received it. Researcher must confirm the server honors a client-supplied `_id` (and what it returns on duplicate) against the local Docker server; if it does not, fall back to plain retry and note the duplicate risk.
- **D-08:** Send recovery is **independent of `:reconnect`**: `:reconnect` stays the rooms/history reload only (Phase 3 D-06).

**Draft & Enter semantics**
- **D-09:** Enter sends. The **newline key keeps the existing `InsertNewline` binding** from the Phase 1/2 keymap (user-rebindable); just document it. Empty/whitespace-only drafts are not sent (existing behavior).
- **D-10:** Drafts are kept **per room in memory** (not persisted to disk) and restored when switching back. The input bar shows the draft of the open room.
- **D-11:** **Encrypted rooms refuse send** with an "encrypted, not supported" status-bar message (Phase 3 D-12 already blocks history). **No send with no room open** keeps the existing "No room open" error. No client-side permission pre-check: read-only/muted/permission rejections surface as a failed message with a short error.
- **D-12:** **No client-side length cap** beyond the existing 64 KiB input cap; the server decides and a rejection surfaces as a failed message.

**Dedupe with Phase 5 realtime**
- **D-13:** The message store **ignores any message whose `_id` is already present**. Because the client-supplied `_id` (D-07) is the pending message's final id, Phase 5's DDP echo of own messages is a no-op; Phase 5 only needs the append path.

**Security (carried forward)**
- **D-14:** Message bodies never appear in logs, `Debug`, `Display` or error strings (Phase 3 D-07, PROJECT.md Security). Send errors are typed/generic. Any pending-send state types use a redacting `Debug`. Outgoing text is sent as-is; incoming/echoed text still goes through `ui/sanitize.rs`.

### Claude's Discretion
- `api/` method layout for `chat.postMessage`, request/response types, timeouts, serialization mechanism for the per-room send queue, pending-message state representation (e.g. a `MessageKind`/status field vs side map), Effect/Action names (`SendMessage`, `MessageSent`, `SendFailed`), exact marker glyphs and default keys for retry/discard.

### Deferred Ideas (OUT OF SCOPE)
- Edit/delete own messages, threads, reactions (MSG-05) — v2.
- Persisting drafts to disk across restarts — new capability.
- Client-side permission/read-only pre-check — revisit if failed-send UX is noisy.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| MSG-02 | User can send a message to the selected room | Endpoint is `chat.sendMessage` (NOT `chat.postMessage`, which cannot carry `_id`); `Client::send_message`, `Effect::SendMessage`, `Action::MessageSent/SendFailed`; pending-state machine on `Message.send`; reconcile-before-retry via `chat.getMessage`; swap-in-place on 200; failure mapping (400 -> new `ApiError::Rejected`); queue gating by `pump()`; drafts, follow policy, vocabulary additions |
</phase_requirements>

## Summary

**Headline finding that contradicts CONTEXT.md:** D-07 and the canonical refs assume `chat.postMessage` accepts a client `_id`. It does not. The official docs list no `_id` parameter for `chat.postMessage`, and the server's schema for it is `additionalProperties: false`, so an `_id` in its body is a 400. The endpoint that accepts a client-supplied `_id` is **`chat.sendMessage`** (body `{"message": {"rid", "_id", "msg"}}`); the server source says so in a comment. D-07's own fallback ("if it does not, plain retry with duplicate risk") is therefore unnecessary: use `chat.sendMessage` and keep the idempotence design. This affects D-04 (the swap source is `chat.sendMessage`'s `message`), the canonical ref, and the REST type names, but no other decision.

**Second finding:** posting the same `_id` twice does **not** return the stored message. In the server source, a duplicate makes `sendMessage()` return `undefined`, and the REST handler then runs `normalizeMessagesForUser([undefined])`, which dereferences `undefined.starred`. The expected outcome (inferred from source, not observed live) is a **400 failure with an unhelpful error text** even though the message exists. So a naive "retry with the same `_id`" turns an unconfirmed-but-delivered message into a spurious failure. The prescriptive fix: retrying an **unconfirmed** message first does `GET chat.getMessage?msgId=<id>`; a 200 means it was delivered (swap it in), anything else means resend with `chat.sendMessage`.

**Primary recommendation:** Implement send as a **state machine on the messages themselves**: add `send: SendState` (`Sent` default, `Queued`, `Sending`, `Failed`, `Unconfirmed`) to `Message`; the reducer's single `pump(room)` function derives "what to send next" purely from the room's message vector (at most one `Sending` per room; a `Failed`/`Unconfirmed` message blocks later `Queued` ones), so ordering (D-03), swap-in-place (D-04), retry/discard (D-06) and dedupe (D-13) all work on one source of truth with no side queue and no new dependency. Use `chat.sendMessage`, generate ids from a random per-session prefix (std `RandomState`, set in `run()`, never in the reducer) plus the existing counter, and add one new `ApiError::Rejected` for send-path 4xx.

### Live verification status (required by the brief, D-07)

| Item the brief asked to confirm | Status | Evidence |
|---|---|---|
| Server's `chat.postMessage` honors client `_id` | **Refuted by source and docs**; not probed live | Docs list no `_id` param; 8.9.1 schema `additionalProperties: false`; source comment "`chat.sendMessage` allows for passing a value for `_id` and the other one doesn't" |
| `chat.sendMessage` honors client `_id` | CITED (docs + 8.9.1/9.0.0 source); **not probed live** | `isChatSendMessageProps` `_id: {type:'string', nullable:true}`; `sendMessage()` inserts `message` with its `_id` |
| Duplicate `_id` response | CITED by source reading, **inferred 400 with an unhelpful error, not observed** | `messageAlreadyExists ... return;` then `filterStarred(undefined)` |
| 401 on both endpoints | **VERIFIED live** against the local 8.9 server (unauthenticated and bogus-credential POSTs) | body `{"success":false,"error":"You must be logged in to do this.","status":"error","message":"You must be logged in to do this."}`, HTTP 401 |
| Success body shape, `ts` format | CITED (docs + source); not probed live | see Pattern 2 |
| Read-only / muted / oversize failure bodies | CITED (source throws); not probed live | see Pitfall 4 |
| Timeout / unknown outcome retry safety | Reasoned from source; unit-testable with wiremock delay | see Pattern 4 |

**Why the live probes were not run:** the project's dev stack was found **already running** (`rockettui-dev-rocketchat-1`, `rockettui-dev-mongodb-1`, up about 7 hours; the brief said none was) and its throwaway credentials live in `dev/state/rockettui-dev/.env.local`, but a harness secret-read guard blocked the Bash command that would read that file, and I did not route around it. I changed nothing on the server and **left the stack as I found it (up)**. The probes are specified in the Validation Architecture (Wave 0: a `dev/` probe script that reads the state file itself, the way `scripts/smoke_pty.py` `docker_env` already does, and prints statuses and shapes only). The planner should make that probe the first task so the D-07 assumptions are confirmed before the reducer work is built on them.

## Project Constraints (from CLAUDE.md)

Read from `/home/joliveira/personal-projects/rockettui/.claude/CLAUDE.md` this session:
- Tech stack is author-chosen: Rust, `ratatui` + `crossterm`, `tokio`, `reqwest` + `serde`, `tokio-tungstenite`, `serde` + `toml`. Do not add alternatives.
- Architecture: Elm/TEA, one `App` state, one `Action` enum, one channel, one reducer. IO only through `Effect`.
- Security: normal client with the user's own account; revocable PAT never committed or logged; **never log message contents**; credentials from env var or `chmod 600` config file.
- Config: XDG `~/.config/rockettui/`, TOML, invalid entries yield clear errors, not crashes.
- Policy: confirm company policy before pointing at the company server (Phase 6; this phase uses only the local Docker server).
- GSD workflow enforcement: file changes go through a GSD command (`/gsd-execute-phase` for planned work).

## Architectural Responsibility Map

The app is a single-process TUI, so "tiers" are the project's own layers.

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Compose, Enter-to-send, draft per room | Reducer (`app/update.rs`, `app/mod.rs`) | `InputBuffer` | The reducer owns all state; `InputBuffer::take()` already clears on submit |
| Pending-message state machine and per-room queue | Reducer (new `app/send.rs`) | `data::Message.send` | State lives in `App.messages`; `pump()` is pure and pushes `Effect`s |
| HTTP send / reconcile | `api::Client` + `effect::run` | — | Only the client holds the token; effects return `Action`s on the one channel |
| Id generation (randomness) | Run loop (`lib.rs`) | `App.id_prefix` field | The reducer never reads a clock or randomness (existing rule for `Clock`) |
| Dedupe by `_id` | Message store helpers (`app/messages.rs`) | `history::on_loaded` | Shared with Phase 5's append path (D-13) |
| Pending/failed/unconfirmed drawing | `ui/wrap.rs::render_rows` | `config/theme.rs` | Row model and renderer must share one function (existing invariant) |
| Follow-newest policy | Reducer (`app/messages.rs`) | `config/settings.rs` | Setting is read by the reducer, validated at load |
| Retry/discard keys | Keymap (`input/keymap.rs`, `input/actions.rs`) | Reducer | Vocabulary is user-facing and rebindable |
| Server-side permission/size/read-only checks | Rocket.Chat server | — | D-11/D-12: the client does not pre-check |

## Standard Stack

### Core (all already in `Cargo.toml`; no new dependency is needed)
| Library | Version in repo | Purpose in this phase | Why |
|---------|-----------------|-----------------------|-----|
| reqwest | 0.13 (`json`, `query`) | `POST chat.sendMessage` with `.json(&body)`; `GET chat.getMessage` | [VERIFIED: /home/joliveira/personal-projects/rockettui/Cargo.toml `reqwest = { version = "0.13", features = ["json", "query"] }`]. `json` is already enabled, so `RequestBuilder::json` is available |
| serde / serde_json | 1.0.229 / 1.0.154 | `#[derive(Serialize)]` request body, `Deserialize` response reusing `WireMsg` | [VERIFIED: Cargo.toml]. Never hand-build JSON strings (injection-safe) |
| tokio | 1 (`rt-multi-thread`,`macros`,`sync`,`time`) | Effect tasks; `tokio::time` only if a test needs delays | [VERIFIED: Cargo.toml] |
| chrono | 0.4.45 | Already parses the response `ts` through `ApiDate` | [VERIFIED: src/api/wire.rs `ApiDate`] |
| wiremock (dev) | 0.6.5 | Mock `POST /api/v1/chat.sendMessage`, `GET chat.getMessage`, 400/401/429/500, delayed response | [VERIFIED: Cargo.toml `wiremock = "0.6"`; `body_partial_json` exists at wiremock-0.6.5/src/matchers.rs:817] |
| insta (dev) | 1 | Snapshot the pending/failed/unconfirmed rows | [VERIFIED: Cargo.toml; 15 snapshots exist under src/ui/snapshots] |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| std `RandomState` for the id prefix | `getrandom` / `rand` / `uuid` crate | `getrandom 0.4.3` is already in Cargo.lock transitively, but making it a direct dependency needs justification and the id only needs uniqueness, not secrecy. `RandomState::new().hash_one(..)` is OS-seeded per process (3 runs of a scratch program gave 3 different pairs) [VERIFIED: scratch program run this session]. Use std |
| `Message.send` field | `MessageKind::Pending(..)` variant or a side map `HashMap<id, SendState>` | A `MessageKind` variant avoids editing 15 literals but mixes origin with state and makes `kind == System` checks awkward; a side map can desync from the vector on dedupe/swap/discard and the renderer would need it in `RowCtx`. See Pattern 1 |
| `chat.sendMessage` | `chat.postMessage` | postMessage cannot take `_id`, so no idempotence (see Summary) |
| Always reconcile before resend | Resend blindly | Blind resend of an unconfirmed message yields a spurious 400 when the first attempt landed (see Pattern 4) |

**Installation:** none. `Cargo.toml` is unchanged by this phase.

**Version verification:** versions above are the ones already pinned and compiling in the repo (`cargo test` ran: 363 passed, 0 failed; `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt --check` clean, this session). No registry lookup was needed because nothing is added.

## Package Legitimacy Audit

No external package is installed by this phase, so there is nothing to run through the legitimacy gate.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| (none new) | — | — | — | — | — | — |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none
If the planner decides to add `getrandom` (not recommended), it must be gated behind a `checkpoint:human-verify` and run through `gsd-tools query package-legitimacy check --ecosystem crates getrandom` first.

## Architecture Patterns

### System Architecture Diagram

```
 Insert-mode Enter
        |
        v
 Action::SubmitInput --> submit_input(app)
        |   guards: mode==Insert, draft not blank, room open (D-11), room not encrypted (D-11)
        |   body = app.input.take()  (D-02: cleared, stays in Insert)
        |   id   = format!("{app.id_prefix}{seq}")       <- id_prefix set once by run() (random)
        v
 push Message{ id, author:"me", ts: clock, body, send: Queued } into app.messages[room]
        |   (store helper: ignore if id already present, D-13)
        |   follow policy (D-05): always -> follow_newest ; if_at_bottom -> only if was at bottom
        v
 pump(app, room)  <-- also called after every MessageSent / SendFailed / retry / discard
        |   pure: if any msg in room is Sending -> stop
        |         first non-Sent msg in vec order:  Queued -> mark Sending, push Effect::SendMessage
        |                                           Failed/Unconfirmed -> BLOCK (head of line)
        v
 app.effects --(drained only by run() in lib.rs)--> effect::spawn(client, tx, Effect)
                                                         |
                       Effect::SendMessage{room,id,text,reconcile}
                                                         v
                      reconcile? --yes--> GET chat.getMessage?msgId=id
                                              | 200 -> Action::MessageSent{room,id,message}
                                              | other -> continue
                      POST chat.sendMessage {"message":{"_id","rid","msg"}}
                                              | 200 + message -> Action::MessageSent
                                              | error         -> Action::SendFailed{room,id,error}
                                                         |
                                                         v  (one channel, back into the reducer)
   MessageSent: replace msg with same id in place (send: Sent, server ts/author)  -> pump(room)
   SendFailed : msg.send = outcome_unknown(error) ? Unconfirmed : Failed
                status.error line (fixed text, no body)                           (blocks queue)
        ^
 Normal mode, Messages focus, cursor on a Failed/Unconfirmed msg:
   retry_send   -> send = Queued (+ reconcile flag if it was Unconfirmed) -> pump
   discard_send -> remove message, fix cursor index                       -> pump
```

### Recommended Project Structure
```
src/
├── app/send.rs        # NEW: submit(), pump(), on_sent(), on_failed(), retry(), discard(), new_id()
├── app/messages.rs    # + push_unique / replace_by_id / is_at_bottom (D-13, D-05)
├── app/mod.rs         # + drafts, id_prefix; open_room swaps drafts (D-10)
├── app/update.rs      # submit_input delegates to send::submit; new Action arms
├── api/mod.rs         # + ApiError::Rejected, Client::send_message, Client::get_message, post_json
├── api/map.rs         # extract a single-message mapper from `history` (reuse kind_and_body)
├── api/wire.rs        # + SendResp { message: WireMsg } (Deserialize only)
├── effect.rs          # + Effect::SendMessage, run() arms, redacting Debug
├── action.rs          # + MessageSent, SendFailed, RetrySend, DiscardSend (+ Debug arms)
├── data/mod.rs        # + SendState, Message.send
├── input/{actions,keymap}.rs   # + retry_send, discard_send names and default rows
├── config/{settings,theme}.rs  # + follow_on_send; markers sending/failed/unconfirmed; messages.pending/failed
├── ui/wrap.rs         # render_rows draws the marker + pending style
├── lib.rs             # run(): session id prefix; no other change (effects already drained)
└── redaction.rs       # add Effect::SendMessage and the new Actions to the sentinel sweep
```

### Pattern 1: Pending state on the message (`Message.send`)
**What:** a Copy enum with no text, so a derived `Debug` is safe, stored on each `Message`.
**Source of truth read:** `/home/joliveira/personal-projects/rockettui/src/data/mod.rs:58-69`:
```rust
/// One chat message. Its `Debug` is written by hand and prints no body or author text.
#[derive(Clone, PartialEq)]
pub struct Message {
    pub id: String,
    pub room_id: RoomId,
    pub author: String,
    /// Unix seconds.
    pub ts: i64,
    pub body: String,
    /// `System` marks a server system message, drawn dim per D-15.
    pub kind: MessageKind,
}
```
**Why a field:** `pump()` needs the order of pending messages relative to each other and to server messages, retry/discard need to find the message by id, the renderer needs the state per message, and D-04 requires the swap to keep the vector position (the per-room cursor `MsgView.cursor` is a vector index, `/home/joliveira/personal-projects/rockettui/src/app/messages.rs:15-18`: `pub struct MsgView { pub cursor: usize, pub top_row: usize, }`). One vector holding everything keeps all of those consistent. Cost: 15 existing `kind: MessageKind` struct literals (src/app/update.rs, redaction.rs, data/fake.rs, data/mod.rs, ui/mod.rs, ui/messages.rs, action.rs, app/history.rs, ui/wrap.rs, api/mod.rs, plus `api/map.rs:217-224`) need `send: SendState::Sent` (mechanical; do it in the first plan, compiler-guided).
```rust
// data/mod.rs
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SendState {
    #[default]
    Sent,        // a server message (history, realtime, confirmed send)
    Queued,      // waiting for its turn in the room's queue (drawn like Sending)
    Sending,     // the one request in flight for this room
    Failed,      // definitely not delivered (401, 400/Rejected, connect error, 429, redirect)
    Unconfirmed, // outcome unknown (timeout, 5xx, dropped connection, undecodable 2xx)
}
// Message gains `pub send: SendState`; add `.field("send", &self.send)` to the hand-written Debug.
```
Do **not** add the body to any new type's `Debug`.

### Pattern 2: The wire contract for send (CITED, Rocket.Chat 8.9.1 and 9.0.0 source)
Request, `POST {base}/api/v1/chat.sendMessage`, JSON:
```json
{ "message": { "_id": "<client id>", "rid": "<room id>", "msg": "<text>" } }
```
- `_id`: `{type:'string', nullable:true}`, no pattern or length rule in the schema; the server only requires it not to match an existing message [CITED: apps/meteor/packages/rest-typings chat.ts `chatSendMessageSchema` at tag 8.9.1; docs: "ID for the new message. Must not match an existing message."].
- **Do not send `ts`, `u`, `md`, `urls`, `pinned`:** 8.9.1 only logs a deprecation, **9.0.0 rejects** them with 400 [CITED: docs "Since version 9.0.0, fields the server sets ... are rejected"; 9.0.0 chat.ts `splitSendMessageFields`/"cannot be set through 'chat.sendMessage'"]. Sending only `_id`, `rid`, `msg` is valid on both.
- Success: HTTP 200 `{"message": {rid, msg, ts, u:{_id,username,name}, _id, _updatedAt, md, mentions, channels, urls}, "success": true}` where `message._id` is the supplied id. `ts` is a JSON-serialized Date, an ISO 8601 string with milliseconds, which the existing `ApiDate::Iso` handles, `ApiDate::Ejson` covers the other form [CITED: docs example + 8.9.1 handler `API.v1.success({ message })`; VERIFIED in repo: /home/joliveira/personal-projects/rockettui/src/api/wire.rs:11-19 `pub enum ApiDate { Iso(String), Ejson { #[serde(rename = "$date")] ms: i64 }, }`]. Note `chat.postMessage` instead returns a top-level numeric `ts` plus `channel`; irrelevant once `chat.sendMessage` is used.
- Rate limit: `chat.sendMessage` has its own limiter `{ numRequestsAllowed: 5, intervalTimeInMS: 1000, bypassPermissions: ['send-many-messages'] }` in addition to the global one [CITED: 8.9.1 chat.ts]. Because sends are serialized per room, a single room cannot exceed this unless responses return in under ~200 ms; map 429 to a send failure, never auto-retry.
- Size: `msg.length > settings Message_MaxAllowedSize` throws `error-message-size-exceeded` [CITED: executeSendMessage in apps/meteor/server/meteor-methods/messages/sendMessage.ts]. The default value (commonly 5000) is [ASSUMED] (A2).

Reuse `WireMsg` for the response so the existing lenient decoding and `kind_and_body` apply:
```rust
// api/wire.rs  (Deserialize only; no Debug, message text is never printed)
#[derive(Deserialize)]
pub struct SendResp { pub message: WireMsg }
// api/mod.rs request body: Serialize only
#[derive(serde::Serialize)] struct SendBody<'a> { message: SendMsg<'a> }
#[derive(serde::Serialize)] struct SendMsg<'a> { #[serde(rename = "_id")] id: &'a str, rid: &'a str, msg: &'a str }
```
`map::history` builds `Message` inside a loop at `/home/joliveira/personal-projects/rockettui/src/api/map.rs:204-232`; extract the per-message construction (author `msg.u.username.unwrap_or_else(|| "unknown".to_string())`, `ts: msg.ts.secs().unwrap_or(0)`, `kind_and_body`) into `pub fn message(room: &RoomId, msg: WireMsg) -> Message` and have both `history` and `send_message` call it, with `send: SendState::Sent`.

### Pattern 3: Serializing sends per room inside TEA (D-03)
`effect::spawn` starts one tokio task per effect (`/home/joliveira/personal-projects/rockettui/src/effect.rs:76-83`), so two effects pushed back to back would race and could land out of order. The serialization therefore has to be in the reducer: never have more than one `Effect::SendMessage` in flight per room. `pump` derives this from the vector, no extra queue type:
```rust
// app/send.rs (sketch; names are Claude's discretion)
pub fn pump(app: &mut App, room: &RoomId) {
    let Some(msgs) = app.messages.get_mut(room) else { return };
    if msgs.iter().any(|m| m.send == SendState::Sending) { return; }
    let Some(next) = msgs.iter_mut().find(|m| m.send != SendState::Sent) else { return };
    if next.send != SendState::Queued { return; } // Failed/Unconfirmed at the head blocks the room
    next.send = SendState::Sending;
    let effect = Effect::SendMessage { room: room.clone(), id: next.id.clone(),
        text: SendText::new(next.body.clone()), reconcile: next.needs_reconcile /* see below */ };
    app.effects.push(effect);
}
```
Head-of-line blocking is the recommended policy: a `Failed` or `Unconfirmed` message holds back the `Queued` ones behind it (they stay drawn as sending), so retrying the head and then resuming automatically preserves order; discarding the head releases the rest. Otherwise D-03's ordering guarantee breaks the moment the user retries an earlier message after a later one went through. Surface this to the author as a policy decision (Open Question 2); it is within "serialization mechanism" discretion.

`reconcile` needs one bit of memory: whether the message was `Unconfirmed` when it went back to `Queued`. Either keep the distinct state `SendState::UnconfirmedRetry` (not drawn differently) or set `reconcile` from the state at the moment `retry` runs and carry it in a private field. The simplest is to pass it straight through: `retry()` sets `Queued` and stores `reconcile = (previous == Unconfirmed)` on a small `HashSet<String>` of ids in `App` (`reconcile_ids`), which `pump` consults and clears. Pick one; both are pure.

`MessageSent`/`SendFailed` arrive on the channel for any room, not just the open one, so handlers address by `(room, id)`, never by `current_room`.

### Pattern 4: Retry and the "unconfirmed" outcome (D-06, D-07)
Classify the error that ended a send (pure fn on `ApiError`, new, in `api/mod.rs`):
- **Failed (definitely not stored):** `Unauthorized`, `Forbidden`, `NotFound`, `RateLimited`, `Redirected`, `Rejected`, `Client`, and a connect failure. 4xx responses come from validation/permission code that runs before the insert [CITED: executeSendMessage order: size check, `canSendMessageAsync`, then `sendMessage`/insert].
- **Unconfirmed (may be stored):** `Timeout`, `Server(_)` (5xx), `BadResponse`/`TooLarge` on a 2xx, and any non-connect transport error. Today `send_error` maps every non-timeout, non-connect, non-builder, non-decode error to `Unreachable` ([VERIFIED: src/api/mod.rs:106-119] `} else { ApiError::Unreachable }`), so a connection reset mid-request is indistinguishable from "cannot connect". Add a variant for it (suggested `ApiError::ConnectionLost`, fixed text "connection lost while sending") and map `is_connect()` only to `Unreachable`. This also improves the history error text and needs a `status_mapping`-style test plus the `ALL_ERRORS` arrays in `src/redaction.rs` and `src/api/mod.rs` tests updated.

Retry of an `Unconfirmed` message must **reconcile first** because a duplicate `_id` post does not return the stored message (see Summary):
1. `GET chat.getMessage?msgId=<id>` (query, auth headers as usual). 200 with `message` -> the first attempt landed: emit `MessageSent` with that message and do not post again. Per source, `chat.getMessage` returns `API.v1.failure()` (400) when `getSingleMessage` finds nothing [CITED: 8.9.1/9.0.0 chat.ts `'chat.getMessage'`], so any non-200 means "not there" (or an error that the POST will then surface).
2. Otherwise `POST chat.sendMessage`.
A retry of a `Failed` message skips step 1 (`reconcile: false`).

Idempotence when step 2 races with a late first attempt: the second insert hits `messageAlreadyExists` -> `undefined` -> 400. Treat a 400 from a **reconciling** send as "ask once more": run the `getMessage` again and, if 200, report `MessageSent`; if still absent, report `Failed(Rejected)`. This closes the only window. [ASSUMED behavior of the duplicate 400; verify with probe P2.]

### Pattern 5: Per-room drafts (D-10)
`App.input` stays the live buffer. Add `drafts: HashMap<RoomId, InputBuffer>` ([VERIFIED: `InputBuffer` is `#[derive(Clone)]`, has `impl Default`, and a redacting `Debug` at src/app/input_buf.rs:13-37]). `App::open_room` is the single place a room becomes current from the sidebar ([VERIFIED: src/app/sidebar.rs:262 `app.open_room(id.clone());` and src/app/mod.rs:208-211 `pub fn open_room(&mut self, id: RoomId) { self.current_room = Some(id); self.focus = Focus::Messages; }`]). Make it: if the new id equals the current one, do nothing to the draft; else stash `std::mem::take(&mut self.input)` under the old id (skip when empty, so the map does not grow), then `self.input = self.drafts.remove(&id).unwrap_or_default()`, then `self.input.settle(width, height)` from `input_viewport()` because the input height may change. A draft typed with no room open (old room `None`) is carried into the first opened room unless that room already has a stored draft. Keep it in memory only.

### Pattern 6: Dedupe by `_id` (D-13) and the Phase 5 hand-off
Add one helper in `app/messages.rs`, e.g. `pub fn push_unique(app, room, msg) -> bool` (skip when any message in the room has the same `_id`) and `pub fn replace_by_id(app, room, msg) -> bool` (in-place swap). `history::on_loaded` already filters by id for the first page and the older page ([VERIFIED: src/app/history.rs:152-156 `.filter(|m| !have.contains(&m.id))` and 195-198 `.filter(|m| have.insert(m.id.clone()))`]); leave it as is. **Refinement of D-13 for Phase 5 (flag, do not lose):** "ignore if already present" is wrong when the present copy is still pending. If the DDP echo arrives while the REST response is lost (Unconfirmed), ignoring it leaves the message marked Unconfirmed forever though the server delivered it. The Phase 5 incoming path should be: id absent -> append; id present and `send == Sent` -> ignore; id present and pending -> replace with the server message (confirm it) and `pump`. Implement the helper this way now (a one-line difference) and note it for Phase 5.

### Pattern 7: Follow policy (D-05)
Existing helper: `/home/joliveira/personal-projects/rockettui/src/app/messages.rs:156-158` `pub fn follow_newest(app: &mut App, room: &RoomId) { app.msg_views.remove(room); }`. Setting (recommended names, Claude's discretion): `follow_on_send = "always" | "if_at_bottom"`, default `"always"`, validated with the existing `Reader::one_of` exactly like `message_layout` ([VERIFIED: src/config/settings.rs:163-174 `reader.one_of(value, "message_layout", &[("compact", MessageLayout::Compact), ("two_line", MessageLayout::TwoLine)])`]); add the key to `KNOWN_KEYS` (settings.rs:102-113) or it is reported as unknown, add a `Settings` field and `Default`. "At the bottom" must be computed **before** appending the pending message: no `msg_views` entry means the default view (newest), otherwise `view.top_row >= total_rows.saturating_sub(viewport_h)` using `messages::row_starts` and `messages::viewport` (last row visible, which is stricter and more truthful than "cursor on the last message"). When not following, leave `msg_views` untouched: an append does not shift existing indices, and the in-place swap keeps them too.

### Anti-Patterns to Avoid
- **Spawning one effect per queued message at Enter time:** tasks race, order is lost. Only `pump` pushes `Effect::SendMessage`, and only when the room has no `Sending` message.
- **Deriving `Debug` on `Effect`:** `Effect` is `#[derive(Clone, Debug, PartialEq)]` today ([VERIFIED: src/effect.rs:11]). Adding a `text: String` field would print the body in any `{:?}`/assert failure. Wrap the text in a `SendText` newtype with a hand-written `Debug` (`<N bytes>`) and keep `PartialEq` for tests.
- **Using `chat.postMessage` with `_id`:** 400 invalid-params.
- **Reusing `ApiError::Display` as the send error text:** `RateLimited` says "wait a minute, then :reconnect" and `Server` says "try :reconnect later" ([VERIFIED: src/api/mod.rs:57-62, 69-71]); D-08 says recovery is independent of `:reconnect`. Build a separate fixed `send_error_text(error, unconfirmed)` for the status bar (e.g. "message not sent: <fixed reason>. r retries, dd discards"), no server text.
- **Displaying the server's `error` string:** it is untrusted server text; map to a closed enum or run it through `sanitize_line` at minimum. Recommended: closed set only (Rejected/Forbidden/etc.), never the raw string.
- **Trusting `current_room` in `MessageSent`/`SendFailed` handlers.**
- **Removing a message without fixing `MsgView.cursor`** (discard shifts later indices; see Pitfall 6).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| JSON request body | String formatting of `{"message":...}` | `#[derive(Serialize)]` struct + `RequestBuilder::json` | Quoting/escaping of arbitrary user text (quotes, newlines, unicode) is exactly what serde handles |
| Response decoding | New wire types | `WireMsg` + extracted `map::message` | Already lenient, already handles `ts` forms, attachments and system types |
| Per-room queue | A second `VecDeque<PendingSend>` plus worker tasks | `Message.send` + `pump()` | Single source of truth; no desync on swap/discard/dedupe; no new async plumbing |
| Request timeouts | Manual `select!` timers | `Client` already sets connect 5 s and total 15 s ([VERIFIED: src/api/mod.rs:25-27 `CONNECT_TIMEOUT` 5 s, `REQUEST_TIMEOUT` 15 s]) | A timeout already maps to `ApiError::Timeout` |
| Idempotence | Client-side "have I sent this" heuristics | Client `_id` + `chat.getMessage` reconcile | The server is the authority on what exists |
| Random id | A custom PRNG | `RandomState::new().hash_one(..)` in the run loop | OS-seeded, std only (see Alternatives) |
| Config enum parsing | New parser | `Reader::one_of`, `Reader::string` | Same diagnostics, line/col, unknown-key warnings |
| Marker validation | New width/sanitize checks | the `parse_markers` path in `config/theme.rs` | Already rejects control/bidi chars and caps width at 4 |

**Key insight:** every hard part of this phase (ordering, idempotence, sanitation, config diagnostics, mock servers) already has a project seam; the work is wiring one new state field through them, not building new machinery.

## Runtime State Inventory

Not applicable: this is a new-capability phase, not a rename/refactor/migration. Nothing stored, registered or cached under a renamed identifier. (Drafts are in-memory only by D-10.)

## Common Pitfalls

### Pitfall 1: Planning on `chat.postMessage`
**What goes wrong:** the `_id` in the body is rejected (400 invalid-params), or, if `_id` is dropped, retries duplicate messages.
**Why it happens:** CONTEXT.md and the canonical refs name the wrong endpoint.
**How to avoid:** `chat.sendMessage` with `{"message": {...}}`. Update the canonical ref list and the Effect/doc comments accordingly.
**Warning signs:** a wiremock test matching `/api/v1/chat.postMessage` passes while the real server answers 400.

### Pitfall 2: Retrying an unconfirmed send yields a spurious failure
**What goes wrong:** the first attempt timed out but landed; the retry with the same `_id` hits `messageAlreadyExists`, the handler later dereferences `undefined`, and the user sees "failed" for a message that is in the room.
**How to avoid:** reconcile with `chat.getMessage` before resending an `Unconfirmed` message (Pattern 4). Confirm P2 live.
**Warning signs:** after a forced timeout, retry shows an error but the official client shows the message.

### Pitfall 3: `Effect` or `Action` printing the body
**What goes wrong:** `#[derive(Debug)]` on `Effect` (currently derived) leaks the text into any panic message or failed assertion; `Action` has a hand-written exhaustive `Debug`, so a new variant forces a decision but a careless arm could still print it.
**How to avoid:** `SendText` newtype with redacting `Debug`; `Action::MessageSent` prints `MessageSent(<room>, <id>)` only (the `Message` field already redacts); `SendFailed` prints room, id, error kind. Extend the sentinel sweep in `/home/joliveira/personal-projects/rockettui/src/redaction.rs` (its header: "Every new type that holds a secret or message text must be added to `sentinel_sweep` here.") with a `SendMessage` effect, `MessageSent`, `SendFailed`, and a `Message` in each `SendState`.

### Pitfall 4: A 400 is mapped to "is server_url a Rocket.Chat server?"
**What goes wrong:** `status_error` turns every unexpected 4xx into `ApiError::BadResponse` ([VERIFIED: src/api/mod.rs:101 `_ if status.is_client_error() => Some(ApiError::BadResponse),`]). On the send path almost every real failure is a 400: read-only room ("You can't send messages because the room is readonly."), muted (`You_have_been_muted`), archived (`room_is_archived`), blocked (`room_is_blocked`), oversize (`error-message-size-exceeded`), and on 8.9.1 even `error-not-allowed`, because only `unauthorized`/`forbidden` codes become 403 there; 9.0.0 maps the `error-not-allowed`/`not-allowed` family to 403 [CITED: 8.9.1 and 9.0.0 ApiClass.ts catch block; `refusalErrorCodes` in 9.0.0]. All carry `{"success":false,"error":"<text>"}` (+ `errorType`), HTTP 400.
**How to avoid:** add `ApiError::Rejected` ("server rejected the message: room may be read-only or muted, or the text is too long") and map send-path 4xx (other than 401/403/404/429) to it in the POST path only; keep the GET `status_mapping` test (`code(400) == BadResponse`) unchanged. Optional refinement: classify the body's `error` by substring into a closed enum (TooLong / ReadOnly / Muted / Other) without storing the text.

### Pitfall 5: Own username shown as "me", then flips on the swap
**What goes wrong:** the pending message uses `LOCAL_AUTHOR` (`"me"`, [VERIFIED: src/app/mod.rs:63-64 `pub const LOCAL_AUTHOR: &str = "me";`]); the swapped server message carries the real username, so the author column width and the author color (keyed on the name via `author_color`) change on every send.
**How to avoid / options:** (a) accept it (simplest); (b) recommended polish: learn the own username once (e.g. `GET /api/v1/me`, an `Effect`/`Action` pair, 5 lines of wire type) and use it for pending messages, falling back to `"me"`. Phase 5 will also need "which messages are mine". Flag as Open Question 3.

### Pitfall 6: Cursor indices after discard (and any removal)
**What goes wrong:** `MsgView.cursor` is a vector index; removing a message before the cursor shifts the message under it.
**How to avoid:** in `discard`, adjust the stored view: cursor index > removed index -> `cursor - 1`; equal -> stays (clamped by `view()`); then rely on `messages::view` to rescroll. Append (send) and in-place swap never shift indices. Add a test mirroring the existing cursor-anchor tests in `src/app/history.rs`.

### Pitfall 7: Status-bar error vanishes on the next key
**What goes wrong:** `Action::Key` clears `status_msg` first ([VERIFIED: src/app/update.rs:19-20 `Action::Key(ev) => { app.status_msg = None;`]), so the D-06 status line lasts until the next key press. The `Failed` marker on the message is what persists.
**How to avoid:** expected and acceptable; do not rely on the status line for state, and do not add a persistent warning (that is `app.warnings`, config only). Test that the marker, not the status line, carries the state.

### Pitfall 8: Pending message hidden behind a history placeholder
**What goes wrong:** the messages pane draws "Loading…" or "Could not load history" instead of the messages while `HistoryState` is `Loading`/`Failed` ([VERIFIED: src/ui/messages.rs `Some(HistoryState::Loading) => Some("Loading…")`, `Some(HistoryState::Failed) => Some("Could not load history")`]). A send in such a room is allowed (nothing forbids it), is queued and sent, but the user sees no pending row.
**How to avoid:** acceptable for a first cut (the status bar still reports failures); note it in UAT. Alternative: block send while `Loading` with a status message. Surface to the author (Open Question 4).

### Pitfall 9: Failed message cannot be edited
**What goes wrong:** the draft is cleared at Enter (D-02), so a too-long or rejected message can only be retried unchanged or discarded and retyped. An oversize message (server limit well below the 64 KiB client cap, D-12) will be a common trigger.
**How to avoid:** none required by CONTEXT; mention in UAT. Deferred-idea candidate: "discard returns the text to the draft".

### Pitfall 10: Counted retry/discard
**What goes wrong:** `Action::Counted(n, inner)` repeats non-motion actions n times ([VERIFIED: src/app/update.rs:29-41]), so `3dd` discards up to three failed messages in a row.
**How to avoid:** acceptable and vim-like; just make sure each repetition re-reads the cursor message and that `retry` on a non-failed message is a silent no-op.

### Pitfall 11: Quitting with pending sends
**What goes wrong:** `:q` drops queued, in-flight and unconfirmed messages silently (in-memory only).
**How to avoid:** known limitation for this phase; optionally warn in the status bar on `:q` when any non-`Sent` message exists (Open Question 5).

### Pitfall 12: Test and harness fixtures only serve GET
**What goes wrong:** `scripts/smoke_pty.py` `start_mock` answers every non-GET with 405 (`if self.command != "GET": self.reply(405, {"success": False})`, scripts/smoke_pty.py:259-290). The existing `insert` scenario still passes (the optimistic row is drawn), but it now silently exercises a failed send.
**How to avoid:** extend the mock to serve `POST /api/v1/chat.sendMessage` (record the JSON body, echo a message) and `GET /api/v1/chat.getMessage`; add `send` and `docker_send` scenarios.

## Code Examples

### The send client (sketch)
```rust
// api/mod.rs  -- source: reqwest RequestBuilder::json (json feature enabled in Cargo.toml)
pub async fn send_message(&self, room: &RoomId, id: &str, text: &str) -> Result<Message, ApiError> {
    let body = SendBody { message: SendMsg { id, rid: &room.0, msg: text } };
    let request = self.http.post(self.endpoint("chat.sendMessage")).json(&body);
    let resp: wire::SendResp = self.read_json(request, /*client_error=*/ ApiError::Rejected).await?;
    Ok(map::message(room, resp.message))
}
```
Refactor `get_json` ([VERIFIED: src/api/mod.rs:216-243]) into `read_json(request_builder, client_error)` (status check, content-length cap, chunked 8 MiB cap, `serde_json::from_slice`) so GET and POST share the body-cap and redirect behavior; `get_json` becomes a 4-line wrapper that passes `ApiError::BadResponse`. The doc comment "Only GET is ever sent in Phase 3" must be updated. Redirects stay disabled (`Policy::none()`), which matters more for a POST: a 307/308 would otherwise re-send the body (and the token headers) to another host; a 3xx maps to `Redirected` -> `Failed`.

### Effect and fail-closed path
```rust
// effect.rs
Effect::SendMessage { room: RoomId, id: String, text: SendText, reconcile: bool }
// in run(): the Err(client) branch must also map it
Effect::SendMessage { room, id, .. } => Action::SendFailed { room, id, error },
```
`SendText` equality is needed for `assert_eq!(app.effects, [Effect::SendMessage{..}])` tests like the existing `first_open_issues_one_load` in `src/app/history.rs`.

### Id generation without touching the reducer
```rust
// lib.rs, next to unix_now(): the only place that reads OS randomness
fn session_id_prefix() -> String {
    use std::hash::{BuildHasher, RandomState};
    let a = RandomState::new().hash_one(0u8);
    let b = RandomState::new().hash_one(1u8);
    format!("rt{a:016x}{b:016x}")   // any string is a valid _id (schema has no pattern)
}
// App gains `id_prefix: String` (default "local-" so tests keep deterministic ids);
// run() sets `app.id_prefix = session_id_prefix();` next to `app.zone = Zone::Local`.
// new_id = format!("{}{}", app.id_prefix, app.local_seq)   // local_seq already exists
```
[VERIFIED: scratch program printed three different pairs over three runs, this session.]

### Vocabulary additions (user-facing; each needs doc comments and validation tests)
- **Actions** (`input/actions.rs`: `ACTION_NAMES`, `action_from_name`, `name_of`, whose `name_of` match is exhaustive with no wildcard so the compiler forces a decision for each new `Action`): `retry_send`, `discard_send`. Defaults in `default_bindings()` (Normal mode): `r` -> `retry_send`, `dd` -> `discard_send`. [VERIFIED: no Normal-mode `r`, `d` or `x` row exists in src/input/keymap.rs:80-99; only `j k h l gg G <C-d> <C-u> <C-w>h/j/l <CR> za : <Esc> <C-c> i a o`.] `dd` is a two-key sequence whose first key `d` is unbound alone, so it is a pure prefix with the existing sequence timeout; `config/keymap.rs` already reports prefix clashes, lockout and duplicates, so the new rows must keep `keymap_g_is_prefix`-style tests green. Both act only with the messages pane focused and the cursor message `Failed`/`Unconfirmed`; otherwise a silent no-op (like `h`/`l` in that pane, D-19). The insert newline binding is already `(Mode::Insert, "<A-CR>", Action::InsertNewline)` and `(Mode::Insert, "<C-j>", Action::InsertNewline)` ([VERIFIED: src/input/keymap.rs:105-106]); D-09 needs documentation only.
- **Markers** (`config/theme.rs`: `MARKER_KEYS`, `Markers`, `slot_mut`, `Default`; cap `MAX_MARKER_WIDTH = 4`): `sending`, `failed`, `unconfirmed` (Queued and Sending share `sending`). Suggested defaults `"…"`, `"!"`, `"?"` (single column; final glyph choice is Claude's discretion). Marker strings are rejected, not sanitized, if `sanitize_line` would change them (Phase 2 decision), so keep them plain.
- **Styles** (`ELEMENT_PATHS`, `Theme` fields, `style_slot_mut`, `Default`): `messages.pending` (default dim, applied to author and body of `Queued`/`Sending`) and `messages.failed` (default `status_error`-like red, applied to `Failed` and `Unconfirmed`). D-01 says "dim", matching `messages.notice`'s default `plain.add_modifier(Modifier::DIM)`.
- **Setting:** `follow_on_send` (Pattern 7).
- There are no user-facing docs files in the repo (only `docs/objective.md`); the vocabulary is documented by the const lists and doc comments above. There is a test in `src/config/theme.rs` that builds a complete `[markers]` TOML from `Markers::default()` (around line 1016); extend it, and the `marker_defaults_match_phase1` style tests, rather than skipping them.
- **Where the marker is drawn (recommended):** in Compact layout it replaces the time cell for a non-`Sent` message (padded to `time_col`; `App::row_ctx` must raise `time_col` to the widest marker of any non-`Sent` message in the room so the body x stays aligned; default one-column markers never change it); in TwoLine it follows the author in the header. Do it inside `wrap::render_rows` so the row model and renderer cannot disagree (the module's stated invariant, src/ui/wrap.rs:1-2).

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `chat.sendMessage` tolerated server-owned fields (`ts`, `u`, ...) with a deprecation log | 9.0.0 rejects them | Rocket.Chat 9.0.0 | Send only `_id`, `rid`, `msg` |
| Auth/permission failures were 400 with error codes | 401/403 for `error-unauthenticated`, `error-not-allowed` family | 9.0.0 | Map on HTTP status first; treat other 4xx as `Rejected` |
| 401 bodies carried `status`/`message` | slimmer 401 bodies | 9.0.0 | Do not parse bodies for control flow (Phase 3 rule stands) |

**Deprecated/outdated:** `chat.postMessage` with `_id` is not a thing; webhook-style `channel` + `text` posting is for integrations and cannot be made idempotent.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Duplicate `_id` on `chat.sendMessage` returns a 400 failure (TypeError text), not a 200 | Summary, Pattern 4, Pitfall 2 | If it instead returned 200 with the stored message, the reconcile step would be redundant (harmless); if it returned 5xx the Unconfirmed mapping would apply. Probe P2 resolves it. Inferred from reading source, not observed |
| A2 | `Message_MaxAllowedSize` default is about 5000 characters | Pattern 2, Pitfall 9 | Only affects UAT sizing; the client does not enforce it (D-12) |
| A3 | The company server supports `chat.sendMessage` and `chat.getMessage` (long-standing endpoints) | Pattern 4 | Phase 6 concern; the local 8.9 server answers both with 401 unauthenticated, which proves the routes exist there |
| A4 | The server does not validate `_id` format (any string accepted) | Pattern 2, id generation | If a deployment enforced Random.id-style 17-char ids, the prefix scheme would be rejected; probe P1 uses the real format of the generated id |
| A5 | The default global REST rate limit on a production server will be reachable by fast typists only rarely | Pattern 2 | A 429 shows as Failed with retry; no data loss |
| A6 | `/api/v1/me` returns `username` for the own account (for Pitfall 5 polish) | Pitfall 5 | Optional feature only |
| A7 | Muting a user for probe P5 uses an admin method not verified here | Validation Architecture | Probe P5 may need a different admin call or may be dropped |
| A8 | Default marker glyphs `…`, `!`, `?` render acceptably in the author's terminal | Code Examples | Cosmetic; configurable |
| A9 | Trimming trailing whitespace before send is desirable (official client trims) but CONTEXT says "as-is" | Open Question 6 | Cosmetic; keep as-is unless the author says otherwise |

## Open Questions (RESOLVED)

> All six resolved 2026-11-17 by user decisions in 04-CONTEXT.md (D-15 to D-20). Recommendations below that differ from the resolution are superseded by it.

1. **Endpoint switch (needs author acknowledgement).** RESOLVED by D-15: switch to `chat.sendMessage`.
   - What we know: `chat.postMessage` cannot carry `_id`; `chat.sendMessage` can; D-07's fallback would otherwise apply.
   - What's unclear: whether the author wants D-07 reworded in CONTEXT.md.
   - Recommendation: proceed with `chat.sendMessage` (keeps the idempotence the author asked for); correct the canonical ref and D-04's "returned by chat.postMessage" wording in the plan's decision log.

2. **Head-of-line blocking policy (D-03 follow-through).** RESOLVED by D-16: failed/unconfirmed messages do NOT block the queue (the "block" recommendation below is superseded).
   - What we know: a failed head with later queued messages either blocks them (preserves order), or lets them through (order inversion on retry).
   - Recommendation: block, resume automatically on retry, release on discard.

3. **Own username for pending rows.** RESOLVED by D-17: fetched once per connection, kept in memory.
   - Recommendation: add the small `GET /api/v1/me` load (fallback `"me"`) if the author dislikes the author flip; otherwise accept.

4. **Send while history is Loading/Failed.** RESOLVED by D-19: allowed; the placeholder may hide the pending row.
   - Recommendation: allow (nothing in CONTEXT forbids it); pending row is hidden by the placeholder, status bar still reports failures.

5. **Warn on `:q` with undelivered messages.** RESOLVED by D-20: warn (status line, repeat or `:q!` to quit). Recommendation: a one-line status message, optional.

6. **Trailing whitespace/newlines.** RESOLVED by locked D-14: send as-is, no trim. Recommendation: send as-is per D-14 unless the author wants `trim_end`.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| rustc / cargo | build, tests | yes | 1.99.1 (project `rust-version = "1.88"`) | — |
| Docker daemon + `rockettui-dev` stack | live probes, `docker_send` scenario, UAT in the official client | yes, **already running** (rocketchat 8.9.1 + mongodb, up ~7 h at start of research; `GET /api/info` answered `"version":"8.9"`) | 8.9.1 | `python3 -I dev/seed.py up` (stack state dir already exists) |
| Rocket.Chat 9.0.0 stack | compatibility pass (as in Phase 3) | not running | — | `RT_PROJECT=rockettui-dev9 RT_RELEASE=9.0.0 RT_HOST_PORT=3101 python3 -I dev/seed.py up` then `seed` |
| Python 3 (stdlib) | `scripts/smoke_pty.py`, `dev/seed.py` | yes | — | — |
| Official Rocket.Chat web/desktop client | success criterion 2 (manual) | not checked; browser at `http://localhost:3100` with the seeded `dev` user (credentials in `dev/state/rockettui-dev/`, not read) | — | UAT by the author |
| GitHub raw access (for source citations) | research only | yes (used) | — | — |

**Missing dependencies with no fallback:** none for planning. Live probes need a script that reads the state file itself (Wave 0).
**Missing dependencies with fallback:** 9.0.0 stack (start on demand).

## Validation Architecture

`workflow.nyquist_validation` is `true` in `/home/joliveira/personal-projects/rockettui/.planning/config.json`.

### Test Framework
| Property | Value |
|----------|-------|
| Framework | libtest (`cargo test`) + `insta` 1 snapshots + `wiremock` 0.6.5; pty e2e via `scripts/smoke_pty.py` (stdlib Python) |
| Config file | none (`[dev-dependencies] insta = "1"`, `wiremock = "0.6"` in Cargo.toml) |
| Quick run command | `cargo test --lib` |
| Full suite command | `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check` |
| Baseline (this session) | 363 tests pass; clippy clean; fmt clean |
| E2E | `cargo build && python3 -I scripts/smoke_pty.py target/debug/rockettui <scenario>` (existing: quit, panic, insert, config_layout, config_keymap, config_error, connect_read, missing_creds, config_creds, perm_warning, unauthorized, docker_read) |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| MSG-02 | Enter in Insert appends a `Queued`->`Sending` message with a client id and pushes exactly one `Effect::SendMessage`; input cleared, mode stays Insert | unit (reducer) | `cargo test --lib app::send` | no, Wave 0 (rewrite `submit_appends_local_message` in `src/app/update.rs`) |
| MSG-02 | Blank draft, no room, encrypted room: no effect; fixed status errors; draft kept | unit | `cargo test --lib app::send` | partly (`submit_blank_is_noop`, `submit_without_room_errors_and_keeps_draft` exist) |
| MSG-02 | Three Enters: one `Sending`, two `Queued`; completion of one pumps the next; strict order | unit | `cargo test --lib app::send::queue` | no, Wave 0 |
| MSG-02 | `MessageSent` swaps in place (id, ts, author), index and cursor unchanged, `Sent`; works when the room is not current | unit | `cargo test --lib app::send::swap` | no, Wave 0 |
| MSG-02 | Failure mapping: `Unauthorized/Rejected/Unreachable/429` -> `Failed`; `Timeout/5xx/ConnectionLost` -> `Unconfirmed`; head blocks queue; status error text has no body | unit | `cargo test --lib app::send::fail` | no, Wave 0 |
| MSG-02 | `retry_send` on `Failed`: `Queued`, no reconcile; on `Unconfirmed`: reconcile flag set; `discard_send` removes and fixes cursor; both no-ops elsewhere; `dd`/`r` default bindings; counted discard | unit | `cargo test --lib app::send::retry` | no, Wave 0 |
| MSG-02 | `chat.sendMessage` request body is exactly `{"message":{"_id","rid","msg"}}`, auth headers, POST, no redirects followed | integration (wiremock) | `cargo test --lib api::tests::send` | no, Wave 0 |
| MSG-02 | Success body maps to a `Message` (`ts` ISO ms and EJSON forms), 400 -> `Rejected`, 401 -> `Unauthorized`, 429, 500 -> Unconfirmed, delayed response (> total timeout) -> `Timeout` -> Unconfirmed | integration (wiremock; `Client::with_timeouts`) | `cargo test --lib api::tests::send` | no, Wave 0 |
| MSG-02 | Reconcile: `chat.getMessage` 200 -> `MessageSent` and **no POST**; 400 -> POST follows | integration (wiremock `received_requests`) | `cargo test --lib effect::tests::reconcile` | no, Wave 0 |
| MSG-02 | Redaction: `Effect::SendMessage`, `MessageSent`, `SendFailed`, every `SendState`, `ApiError::Rejected` never show the `BODY` sentinel in `Debug`/`Display`; rendered screen sanitizes echoed text | unit (sentinel sweep) | `cargo test --lib redaction` | exists, extend `src/redaction.rs` |
| MSG-02 | D-13: `push_unique` ignores a present id; replaces when present is pending; history merge unchanged | unit | `cargo test --lib app::messages` | partly |
| MSG-02 | D-10 drafts: switch A->B->A restores; same-room reopen keeps; empty drafts not stored; no-room draft carried to first room; viewport re-settled | unit | `cargo test --lib app::tests` | no, Wave 0 |
| MSG-02 | D-05 policy: `always` jumps to bottom, `if_at_bottom` follows only when at bottom; bad value -> config error with file/line | unit | `cargo test --lib config::settings app::messages` | no, Wave 0 |
| MSG-02 | Vocabulary: new action names round-trip (`names_round_trip`), new marker/element names parse, over-wide or control-char markers rejected, `vim-extras` row-count test still holds | unit | `cargo test --lib input::actions config::theme config::keymap` | exists, extend |
| MSG-02 | Rows for Queued/Sending/Failed/Unconfirmed in Compact and TwoLine, body x stays aligned, row model equals renderer | snapshot (`insta`) | `cargo test --lib ui::messages ui::wrap` (`cargo insta review` for new snaps) | no, Wave 0 (15 existing snaps) |
| MSG-02 | End to end against the mock: type, Enter, row appears dim then confirmed, request recorded once | pty | `python3 -I scripts/smoke_pty.py target/debug/rockettui send` | no, Wave 0 (extend `start_mock` for POST + getMessage) |
| MSG-02 | End to end against the real local server: send appears and persists; reopen room shows it from history | pty | `RT_PROJECT=rockettui-dev python3 -I scripts/smoke_pty.py target/debug/rockettui docker_send` | no, Wave 0 |
| MSG-02 (success criterion 2) | The sent message is visible in the official client | manual UAT | author opens `http://localhost:3100`, room `rt-public` as `dev` | manual-only (needs the official client) |

### Live probes to run first (Wave 0, against `rockettui-dev` only, using a `dev/` script that reads the state file internally and prints only HTTP status, key names and shapes, never values)
- **P1** `POST chat.sendMessage` with a fresh `_id`, `rid` of a throwaway room, `msg`: expect 200, `message._id` equals the supplied id, `message.ts` ISO string with milliseconds, `u.username`.
- **P2** the same body again: record status and the `error` text (expected 400, TypeError-like; this is A1).
- **P3** `GET chat.getMessage?msgId=<id>` right after P1 (expect 200) and for an unknown id (expect 400).
- **P4** read-only room (create with `readOnly`, add `dev` as a plain member): record status/error (expected 400, "You can't send messages because the room is readonly.").
- **P5** muted user (A7) and archived room: record status/error.
- **P6** text longer than `Message_MaxAllowedSize`: record status/error (expected `error-message-size-exceeded`).
- **P7** `chat.postMessage` with `_id` in the body: confirm the 400 invalid-params that proves the endpoint switch.
- Clean up any probe rooms afterwards (admin API), so the seeded dataset and `docker_read` stay unchanged. Do not touch other Docker containers.

### Sampling Rate
- **Per task commit:** `cargo test --lib`
- **Per wave merge:** `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check`
- **Phase gate:** full suite green; pty scenarios `quit`, `panic`, `insert`, `send`, `config_layout`, `config_keymap`, `config_error`, `connect_read`, `missing_creds`, `config_creds`, `perm_warning`, `unauthorized`, `docker_read`, `docker_send`; manual UAT in the official client before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `dev/` probe script (P1-P7), reading `dev/state/<project>/` itself, printing no secrets or message bodies
- [ ] `src/app/send.rs` test module (queue, swap, failure, retry, discard, drafts, follow policy)
- [ ] wiremock tests for `chat.sendMessage`/`chat.getMessage` in `src/api/mod.rs` and `src/effect.rs`
- [ ] `scripts/smoke_pty.py`: `start_mock` serves `POST chat.sendMessage` and `GET chat.getMessage`; new `send` and `docker_send` scenarios; add to `SCENARIOS`
- [ ] new `insta` snapshots for pending/failed/unconfirmed rows (review with `cargo insta review`)
- [ ] update existing literals (`Message { .. kind }` x15) and the `submit_appends_local_message` test
- Framework install: none.

## Security Domain

`security_enforcement` is `true` (ASVS level 1, block on high) in `.planning/config.json`.

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | yes (unchanged) | PAT headers set once in `Client::with_timeouts`, marked sensitive; the token never leaves `api::Client` |
| V3 Session Management | no | stateless PAT |
| V4 Access Control | yes (server-enforced) | Server decides read-only/muted/permission (D-11); client shows a fixed rejection, no pre-check |
| V5 Input Validation | yes | outgoing body is typed (`serde::Serialize`), 64 KiB `InputBuffer` cap, `sanitize` on the way in; echoed text sanitized on the way out (`ui/sanitize.rs`); config values validated with `Reader` |
| V6 Cryptography | no | TLS via reqwest/rustls defaults, no custom crypto (id randomness is for uniqueness only, not secrecy) |
| V7 Error handling/logging | yes | typed `ApiError`, no `reqwest::Error` formatting, no log file, redacting `Debug` on every new type, sentinel sweep |
| V9 Communications | yes | `redirect::Policy::none()` already set, so a POST body and token headers are never replayed to another host on 307/308; plain http only on loopback (existing warning) |

### Known Threat Patterns for this stack
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Message body leaks via `Debug` of `Effect`/`Action`/pending types or a panic message | Information disclosure | `SendText` newtype with redacting `Debug`; hand-written `Action` arms; extend `src/redaction.rs` sentinel sweep |
| Body or token in error text (`reqwest::Error` embeds the URL) | Information disclosure | `send_error` is the single mapping point; fixed `ApiError` strings |
| Server-supplied error text containing terminal escapes shown in the status bar | Tampering / spoofing | never display the server `error` string; closed enum or fixed text; status text already passes through `sanitize_line` (src/ui/status.rs) |
| POST replay to another host on redirect | Information disclosure | redirects disabled; 3xx -> `Redirected` -> `Failed` |
| Duplicate delivery from retry | Integrity (user-visible spam) | client `_id` + reconcile via `chat.getMessage` before resend; head-of-line blocking |
| Out-of-order delivery | Integrity | one in-flight send per room, enforced by `pump()` |
| Request flood from a stuck queue | Denial of service (self / server rate limit) | no automatic retry (Phase 3 rule), one in-flight per room, 429 -> `Failed` |
| Oversized body / memory | Denial of service | 64 KiB input cap; 8 MiB response cap already enforced in the shared reader |
| Sending to the wrong room after a switch | Integrity | the effect carries the room id captured at Enter; results are addressed by `(room, id)`, never by `current_room` |
| Probe script leaking credentials | Information disclosure | the probe reads `dev/state/` itself and prints no secrets/bodies; never commit its output |

## Sources

### Primary (HIGH confidence)
- Repo files read in full or by range this session: `src/app/update.rs`, `src/effect.rs`, `src/action.rs`, `src/data/mod.rs`, `src/api/mod.rs` (1-560), `src/api/wire.rs`, `src/api/map.rs` (120-260), `src/app/mod.rs`, `src/app/messages.rs` (1-260), `src/app/history.rs` (1-330), `src/app/input_buf.rs` (1-60, 295-325), `src/config/settings.rs` (1-200), `src/config/theme.rs` (1-260), `src/input/actions.rs`, `src/input/keymap.rs`, `src/ui/wrap.rs` (1-200), `src/ui/messages.rs` (1-60), `src/ui/status.rs`, `src/lib.rs`, `src/redaction.rs` (1-140), `scripts/smoke_pty.py` (structure and mock), `dev/README.md`, `dev/compose.yml`, `Cargo.toml`, `.planning/config.json`
- Live local server (8.9): `GET /api/info` (`"version":"8.9"`), unauthenticated and bogus-credential `POST chat.sendMessage`, `POST chat.postMessage`, `GET chat.getMessage` all HTTP 401 with the body quoted above
- Rocket.Chat source at tags 8.9.1 and 9.0.0 (raw.githubusercontent.com/RocketChat/Rocket.Chat): `apps/meteor/server/api/v1/chat.ts` (`chat.sendMessage`, `chat.postMessage`, `chat.getMessage`), `apps/meteor/server/meteor-methods/messages/sendMessage.ts` (`executeSendMessage`), `apps/meteor/server/lib/messages/sendMessage.ts` (`sendMessage`, `messageAlreadyExists`), `apps/meteor/server/lib/utils/lib/normalizeMessagesForUser.ts`, `apps/meteor/server/lib/authorization/canSendMessage.ts`, `apps/meteor/server/api/ApiClass.ts` (error mapping), `packages/rest-typings/src/v1/chat.ts` (schemas)
- Rocket.Chat developer docs: https://developer.rocket.chat/reference/api/rest-api/endpoints/messaging/chat-endpoints/send-message and .../postmessage

### Secondary (MEDIUM confidence)
- `.planning/phases/03-connect-read/03-RESEARCH.md` for local-server facts (limiter off locally, 9.0.0 status changes)

### Tertiary (LOW confidence)
- none relied on

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH, nothing new; all pinned and passing
- Architecture: HIGH for the state-machine design (verified against the actual reducer/effect/row-model seams); MEDIUM for the duplicate-`_id` behavior (source-inferred)
- Pitfalls: HIGH for in-repo ones; MEDIUM for server failure bodies until probes P2-P6 run

**Research date:** 2026-11-17
**Valid until:** 30 days for the in-repo findings; re-check the server contract if the target server version changes (Phase 6)
