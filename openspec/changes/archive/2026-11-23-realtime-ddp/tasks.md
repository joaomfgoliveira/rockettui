# Tasks

Local Docker Rocket.Chat only (`dev/seed.py`, 8.9.1 and 9.0.0). Nothing here points at the company server. Run `cargo test` and `cargo clippy` after each group.

## 1. Dependency and DDP protocol layer

- [x] 1.1 Add `tokio-tungstenite` with the TLS feature matching `reqwest`, and add the `tokio` features the task needs. Verify `cargo build` succeeds and `cargo tree -d` shows no second TLS stack.
- [x] 1.2 Create `src/realtime/` with serde types for the DDP frames used (`connect`, `connected`, `method`, `result`, `sub`, `ready`, `nosub`, `changed`, `ping`, `pong`), parsing tolerant of unknown frames. Verify unit tests parse sample frames (shapes taken from `dev/probe_ddp.py` output) and skip an unknown frame without error.
- [x] 1.3 Build the login frame in one place with a hand-written `Debug` that never prints the token. Verify a test that formats the frame and the connection config shows no token, and add the module to the `redaction` tests.
- [x] 1.4 Derive the WebSocket URL from the configured server URL (`http`->`ws`, `https`->`wss`, path `/websocket`). Verify unit tests for http, https, trailing slash and a sub-path install.

## 2. Reducer side (no network yet)

- [x] 2.1 Verify how "loaded" is tracked (`app.history` versus `app.messages`) and write a failing test first: a live message for a never-opened room must not make that room skip its first history fetch. Verify the test fails against a naive `merge_incoming` call.
- [x] 2.2 Add realtime `Action` variants (connected, disconnected, message, subscription update, rejected) as event Actions that no keymap row can bind, with a hand-written `Debug` that never prints message text. Verify the keymap tests that enumerate bindable actions still pass.
- [x] 2.3 Handle a live message in the reducer: append through `merge_incoming` only when the room's history is loaded, otherwise update unread only; keep the cursor message and follow rules intact. Verify tests for the open room, another loaded room, an unloaded room, a duplicate id, an echo of a pending send, and a reader scrolled up.
- [x] 2.4 Handle live unread: apply `alert`/`unread` from a subscription update to the sidebar state, including the server clearing it; an open room stays flagged. Verify sidebar tests for set, clear, the open room, and "opening does not mark read" unchanged.
- [x] 2.5 Add per-room stale tracking (`HashSet<RoomId>` on `App`): reopening a stale room refetches its newest page and merges by id, then clears the flag; a non-stale room still does not refetch. Verify the `message-history` reopen scenarios.
- [x] 2.6 Gap recovery on every `RtConnected`: push `Effect::LoadHistory` for the open room, mark other loaded rooms stale; a merged page that overlaps nothing held drops and reloads that room. Verify tests for a short gap, no gap, and a gap larger than `history_size` (no hole, no duplicate).
- [x] 2.7 Add the connection status (live, reconnecting, unavailable) shown in the status line, cleared when live updates resume. Verify snapshot tests for each state and that a rejected login shows "live updates unavailable" and no token.

## 3. The realtime task

- [x] 3.1 Implement connect, `login {resume: <PAT>}`, subscribe to `stream-room-messages ["__my_messages__", false]` and `stream-notify-user "<uid>/subscriptions-changed"`, then the read loop that maps frames to Actions via `api::map` (sanitizing and encrypted-message rules included). Verify with a fake in-process WebSocket server test that a message frame yields one `RtMessage` and an unparseable frame is skipped without logging its content.
- [x] 3.2 Keep the task able to send extra `sub` frames after connect (the seam for `live-edits-deletes`) without building any per-room subscription. Verify a test sends an extra `sub` through that seam.
- [x] 3.3 Heartbeat: answer each DDP `ping` with `pong` at once; treat a closed socket or 75 s of silence as dropped. Verify fake-server tests (scaled-down real-time timings: a paused clock fires the handshake timeout while the fake server waits on its socket): ping answered, silence detected, close detected.
- [x] 3.4 Reconnect with exponential backoff and jitter (ceiling about 30 s, reset after a connection stays up); emit `RtConnected` and `RtDisconnected`; stop retrying after a rejected login until `:reconnect`. Verify fake-server tests of the backoff sequence, reset, and the rejected-login stop.
- [x] 3.5 Wire the task into the run loop: spawn after the rooms load, `:reconnect` cancels the old task and starts a new one, quit stops it. Verify a test or smoke run that `:reconnect` leaves exactly one live connection.

## 4. Live verification against local Docker

- [x] 4.1 Add a `docker_realtime` scenario next to `docker_send`: with the seeded server, a message posted by `alice` appears live in the open room, a DM and a private group raise the unread flag, and the echo of the app's own send does not duplicate. Verify it passes on 8.9.1.
- [x] 4.2 Add a reconnect scenario: stop the Rocket.Chat container, post while it is down (or restart it), bring it back, and confirm the open room catches up without duplicates and a stale room refetches on open, including a gap larger than `history_size`. Verify it passes on 8.9.1.
- [x] 4.3 Run both scenarios on 9.0.0. Verify they pass, and note any difference in `dev/README.md`.
- [x] 4.4 Document `dev/probe_ddp.py` in `dev/README.md` (what each probe asks, localhost-only guard, how to run it on both versions). Verify the documented commands run as written.
- [x] 4.5 Run the pty smoke harness (`scripts/smoke_pty.py`) and `cargo clippy`; verify the token and message text never appear in any output captured during the scenarios.

## Workflow follow-up

- Update `openspec/specs/` through archive so `realtime-messages` is added and `message-history` and `rooms` are updated.
- Next changes: `live-edits-deletes`, then `edit-delete-own-messages`; `company-server` re-runs the probe against the company server only once policy is confirmed.
