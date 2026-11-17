# Phase 4: Send Messages - Context

**Gathered:** 2026-11-17
**Status:** Ready for planning

<domain>
## Phase Boundary

User composes a message in Insert mode and sends it to the open room via REST (`chat.postMessage`); it appears in the room and in the official client. Covers MSG-02. Replaces the Phase 1 local-append in `submit_input` (D-15). No realtime/DDP (Phase 5), no edit/delete/threads/reactions (MSG-05), no company server (Phase 6).

</domain>

<decisions>
## Implementation Decisions

### Send flow & echo
- **D-01:** Sending is **optimistic**. Enter appends the message immediately (extending the existing `local-N` path) drawn **dim with a "sending" marker**, then it is replaced by the server's message on success. The marker is a new `[markers]`/theme element; document any new `theme.toml` names (user-facing vocabulary, Phase 2).
- **D-02:** The input is **cleared on Enter** and stays in Insert mode so the user can keep typing.
- **D-03:** Sends are **queued in order per room** (serialized), so ordering is preserved on slow networks. Several can be pending at once.
- **D-04:** On success the pending message is **swapped in place** for the message returned by `chat.postMessage` (server `_id`, server `ts`), keeping its position. **No history refetch.**
- **D-05:** After a send, the messages pane follows the newest message. Behavior is **configurable**: a new `config.toml` setting chooses between *always jump to bottom on send* and *only follow if already at the bottom*. Both must be implemented; default is always-follow (existing `follow_newest`). Key name/values are Claude's discretion but are user-facing vocabulary, validated with a clear config error on bad values.

### Failure & retry
- **D-06:** A failed send (401, unreachable, TLS, timeout, 4xx/5xx) leaves the pending message in the pane marked **failed** (theme marker) **plus** a status-bar error (`status.error`, Phase 3 D-06). A Normal-mode key on that message **retries** it; another key **discards** it. Key names go through the Phase 2 keymap vocabulary (new curated action names, user-rebindable; document them).
- **D-07:** Idempotence: the client generates the message `_id` and passes it in `chat.postMessage`; a retry reuses the same `_id`. A **timeout/unknown outcome is shown as "unconfirmed"**, not "failed", since the server may have received it. Researcher must confirm the server honors a client-supplied `_id` (and what it returns on duplicate) against the local Docker server; if it does not, fall back to plain retry and note the duplicate risk.
- **D-08:** Send recovery is **independent of `:reconnect`**: `:reconnect` stays the rooms/history reload only (Phase 3 D-06).

### Draft & Enter semantics
- **D-09:** Enter sends. The **newline key keeps the existing `InsertNewline` binding** from the Phase 1/2 keymap (user-rebindable); just document it. Empty/whitespace-only drafts are not sent (existing behavior).
- **D-10:** Drafts are kept **per room in memory** (not persisted to disk) and restored when switching back. The input bar shows the draft of the open room.
- **D-11:** **Encrypted rooms refuse send** with an "encrypted, not supported" status-bar message (Phase 3 D-12 already blocks history). **No send with no room open** keeps the existing "No room open" error. No client-side permission pre-check: read-only/muted/permission rejections surface as a failed message with a short error.
- **D-12:** **No client-side length cap** beyond the existing 64 KiB input cap; the server decides and a rejection surfaces as a failed message.

### Dedupe with Phase 5 realtime
- **D-13:** The message store **ignores any message whose `_id` is already present**. Because the client-supplied `_id` (D-07) is the pending message's final id, Phase 5's DDP echo of own messages is a no-op; Phase 5 only needs the append path.

### Security (carried forward)
- **D-14:** Message bodies never appear in logs, `Debug`, `Display` or error strings (Phase 3 D-07, PROJECT.md Security). Send errors are typed/generic. Any pending-send state types use a redacting `Debug`. Outgoing text is sent as-is; incoming/echoed text still goes through `ui/sanitize.rs`.

### Post-research amendments (user-confirmed 2026-11-17, after 04-RESEARCH.md)
These override any conflicting wording in D-03, D-04, D-07 and the canonical refs above.
- **D-15:** (amends D-04, D-07 and the canonical ref) Sending uses REST **`chat.sendMessage`** with body `{"message":{"_id","rid","msg"}}`, **not `chat.postMessage`**: `chat.postMessage` rejects a client-supplied `_id` (400), `chat.sendMessage` accepts it. D-07's "fall back to plain retry" is therefore NOT used; the client-generated `_id` stays. Wherever D-04 says "returned by `chat.postMessage`", read `chat.sendMessage`. Before resending an **Unconfirmed** message, call `GET chat.getMessage?msgId=<id>`: 200 means it was delivered (swap it in, do not POST again); anything else means resend. Research inferred from server source (not yet observed live) that a duplicate `_id` returns a misleading 400, so a live probe against the local server is the first Wave 0 task.
- **D-16:** (refines D-03) A **Failed or Unconfirmed message does NOT block** later queued messages in the same room. Sends stay serialized (at most one in flight per room), but the queue continues past a failed message. A retried message may therefore land after messages typed later; this is accepted.
- **D-17:** The logged-in user's **username is fetched once per connection** (`GET /api/v1/me`, best effort, alongside the Phase 3 connect-time loads and re-run by `:reconnect`) and **kept in memory** in app state. It is never fetched per send and not written to disk. Pending rows use it as their author, so there is no `"me"` placeholder and no author flip on swap. Only if that call has failed or not yet resolved does the pending row fall back to `LOCAL_AUTHOR`, and it should pick up the real name once it arrives.

- **D-18:** (amends D-06 and D-07, user-confirmed 2026-11-17 after plan check) A **5xx answer is Unconfirmed, not Failed** (the server may have stored the message before erroring), same as timeout and lost connection: it shows the "unconfirmed" marker, and retry first reconciles with `GET chat.getMessage` before re-POSTing. Every other 4xx answer, 401, and unreachable/TLS errors stay **Failed** as D-06 says.
- **D-19:** (refines D-01, user-confirmed) While a room's history is Loading or Failed, the messages pane shows its placeholder and **may hide the pending row**; sending still works and failures still reach the status bar. The row appears once history loads. No special rendering is added for this state.
- **D-20:** (new, user-confirmed) **`:q` with undelivered messages warns**: if any message in any room is Queued, Sending, Failed or Unconfirmed, `:q` shows a one-line status message (without message text) and does not quit; a repeated `:q` or `:q!` quits anyway. Clarification of D-08: `:reconnect` must not touch sends; reloading the username (D-17) is allowed.

- **D-21:** (amends D-13, user-confirmed 2026-11-17 after the second plan check) The message store's dedupe by `_id` is state-aware: incoming `_id` **absent → append**; **present and already Sent → ignore**; **present and still pending (Queued/Sending/Failed/Unconfirmed) → replace in place and mark Sent**. This lets Phase 5's realtime echo confirm a send whose server reply was lost. D-13's "ignore any message whose `_id` is already present" applies only to the already-Sent case.

### Claude's Discretion
- `api/` method layout for `chat.sendMessage` (see D-15; the original text said `chat.postMessage`), request/response types, timeouts, serialization mechanism for the per-room send queue, pending-message state representation (e.g. a `MessageKind`/status field vs side map), Effect/Action names (`SendMessage`, `MessageSent`, `SendFailed`), exact marker glyphs and default keys for retry/discard.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Project definition
- `.planning/PROJECT.md` — constraints (Security, Config, TEA architecture)
- `.planning/REQUIREMENTS.md` — MSG-02 (MSG-03/04 are Phase 5)
- `.planning/ROADMAP.md` §Phase 4 — goal and success criteria
- `docs/objective.md` — security principles

### Prior decisions
- `.planning/phases/01-modal-tui-shell/01-CONTEXT.md` — D-14 (multiline input), D-15 (local append being replaced), D-17 (Esc/Ctrl-c), D-18 (input motions)
- `.planning/phases/02-configuration-system/02-CONTEXT.md` — config error screen, keymap/theme vocabularies, `[markers]`
- `.planning/phases/03-connect-read/03-CONTEXT.md` — D-06 (status-bar errors, `:reconnect`), D-07 (redaction), D-12 (encrypted rooms), D-13 (history cache)
- `.planning/phases/03-connect-read/03-RESEARCH.md` — REST endpoints and local server facts

### External
- Rocket.Chat REST `chat.postMessage` docs (fields `roomId`, `text`, `_id`; PAT headers `X-User-Id` / `X-Auth-Token`) — researcher to locate current version and confirm `_id` behavior

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `src/app/update.rs` `submit_input` (~L202): current local append with `local-N` id, `LOCAL_AUTHOR`, `messages::follow_newest`; the seam to replace.
- `src/app/input_buf.rs` `InputBuffer` (`take()`, redacting Debug, 64 KiB cap).
- `src/effect.rs`: `Effect` enum + `run`/`spawn` and `App.effects` outbox drained only by `run()`; add a send effect here.
- `src/api/mod.rs` `Client` (headers hold the token, redirects disabled, no `reqwest::Error` formatting); add `post_message`.
- `src/data/mod.rs` `Message`/`MessageKind` (redacting Debug), `src/ui/messages.rs`, `src/app/messages.rs`, `src/app/history.rs` (per-room cache).
- `src/action.rs`, `src/app/command.rs`, `src/config/keymap.rs` / `theme.rs` / `settings.rs` for new actions, markers, and the scroll-follow setting.

### Established Patterns
- Elm/TEA: reducer is pure; IO via `Effect`, results come back as `Action`s on the one channel.
- Hand-written redacting `Debug` for user text; typed generic errors.
- New user-facing names (keymap actions, theme elements, settings) are vocabulary: document them, validate with clear config errors.

### Integration Points
- Replace `submit_input`'s local append with push `Effect::SendMessage`; handle `MessageSent`/`SendFailed` in the reducer; add id-based dedupe in the message store (shared with Phase 5).

</code_context>

<specifics>
## Specific Ideas

- Success criterion 2 is verified by seeing the message in the official Rocket.Chat client (manual UAT against the local Docker server).

</specifics>

<deferred>
## Deferred Ideas

- Edit/delete own messages, threads, reactions (MSG-05) — v2.
- Persisting drafts to disk across restarts — new capability.
- Client-side permission/read-only pre-check — revisit if failed-send UX is noisy.

</deferred>

---

*Phase: 4-Send Messages*
*Context gathered: 2026-11-17*
