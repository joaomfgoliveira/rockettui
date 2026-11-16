# Phase 3: Connect & Read - Context

**Gathered:** 2026-11-16
**Status:** Ready for planning

<domain>
## Phase Boundary

User authenticates with a Personal Access Token (user id + token) and reads real rooms and message history from a local Docker Rocket.Chat via REST. Covers AUTH-01, AUTH-02, AUTH-03, ROOM-01, ROOM-02, ROOM-03, MSG-01, DLVR-01. Read-only: no sending (Phase 4), no realtime/DDP (Phase 5), no company server (Phase 6).

</domain>

<decisions>
## Implementation Decisions

### Credentials & server config (AUTH-01, AUTH-02)
- **D-01:** User id and token come from env vars `ROCKETTUI_USER_ID` / `ROCKETTUI_TOKEN` or `user_id` / `token` keys in `config.toml`. **Env vars override config** when both exist.
- **D-02:** Server URL is `server_url` in `config.toml`, overridable by `ROCKETTUI_SERVER_URL`. Validated at load: must be an http/https URL; `http://` is allowed (local Docker). This fulfils Phase 2 D-12's deferral.
- **D-03:** Missing or incomplete URL/user id/token is handled by the **existing Phase 2 config error screen** (D-02 there): it names exactly what is missing and where to set it; any key quits non-zero. No fake-data fallback.
- **D-04:** AUTH-02 permission check is **warn-only**. It runs only when the token actually comes from the config file (not from env). A group/other-readable file yields a persistent status-bar warning plus a stderr line after exit; the app still starts.

### Connection & failure UX (AUTH-03)
- **D-05:** The TUI starts immediately with an empty sidebar and a "Connecting…" status. REST tasks run in the background and deliver results through the existing Action channel (`RoomsLoaded` etc.). No blocking pre-TUI login.
- **D-06:** Runtime failures (401, unreachable, TLS, timeout) show a short actionable **status-bar error** using the Phase 2 `status.error` style; the app stays usable (`:q`). A **`:reconnect`** (alias `:retry`) command re-runs the load, added to the table-driven command dispatch. REST only; realtime reconnect stays MSG-04 in Phase 5.
- **D-07:** **No log file in Phase 3.** Credentials live in a secret wrapper with a redacting `Debug` (same approach as `Message` in Phase 1), a test asserts neither `Debug` nor `Display` of config/credentials/errors contains the token, and error messages are typed/generic with no message content.
- **D-08:** HTTP client is `reqwest` with rustls defaults and standard certificate verification. **No** `insecure_skip_verify` or `ca_cert` option now (private CA, if needed, is Phase 6).

### Room list mapping (ROOM-01..03)
- **D-09:** Only the user's **joined rooms** appear (subscriptions), sorted by last activity (Phase 1 D-11). No directory browsing or joining. DMs are named by the other user's username. Subscriptions with `open: false` are skipped. Discussions and teams are included (Phase 1 D-12).
- **D-10:** The sidebar keeps its **Unread / Folders / Recent** layout. Rocket.Chat 8.9+ (Enterprise) has user-defined sidebar **Categories**; the researcher must find the API and minimum server version. If the category API is available, Folders = the user's categories. Otherwise (e.g. local Docker without a license) Folders = a single **Favorites** group (subscription `f` flag). Sidebar fold/unfold code is reused unchanged either way.
- **D-11:** `Room.unread` is set from subscription `unread > 0` as a boolean only. No counts or mentions. Opening a room does **not** mark it read on the server.
- **D-12:** Encrypted rooms (`encrypted: true`) are listed with the Phase 2 `[markers]` encrypted marker. Opening one shows an "encrypted, not supported" placeholder in the messages pane and **never requests history**.

### History loading & scroll (MSG-01)
- **D-13:** History is fetched **on first open** of a room, `history_size` newest messages (config, default 50), and **cached per room**. Reopening does not refetch (realtime keeps it fresh in Phase 5). No startup prefetch of all rooms.
- **D-14:** Scrolling to the oldest loaded message (`gg` or cursor at the top) **loads the next older page** (`history_size` each), prepending while keeping the cursor on the same message. Includes handling "no more history".
- **D-15:** Rendering stays text-only. System messages show a dim one-liner (e.g. "* joined the room"); attachments show `[attachment: name]`. No markdown, reactions, edit indicators or threads (out of scope). All text goes through the existing sanitization (`ui/sanitize.rs`).
- **D-16:** While history loads, the messages pane shows a "Loading…" placeholder. On failure it shows "Could not load history" plus the status-bar error. Reopening the room or `:reconnect` retries. Empty rooms keep Phase 1's empty state.

### Claude's Discretion
- HTTP client module layout (`api/` per the planned structure), request/response types, timeouts, user-agent string (generic, no personal details).
- Exact REST endpoints (`subscriptions.get` / `rooms.get` / `channels|groups|im.history` or `chat.syncMessages`) — researcher to confirm against the Docker server version.
- Env var and error wording details, loading placeholder styling via new theme elements if needed (document any new `theme.toml` names; they are user-facing vocabulary per Phase 2).
- Docker Rocket.Chat setup/seed instructions for DLVR-01.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Project definition
- `.planning/PROJECT.md` — constraints (Security, Config, Architecture TEA)
- `.planning/REQUIREMENTS.md` — AUTH-01..03, ROOM-01..03, MSG-01, DLVR-01
- `.planning/ROADMAP.md` §Phase 3 — goal and success criteria
- `docs/objective.md` — security principles and planned `api/` module

### Prior decisions
- `.planning/phases/01-modal-tui-shell/01-CONTEXT.md` — D-11/D-12/D-13 (sidebar sections, room kinds, replaceable data-source seam), D-16 (table-driven commands)
- `.planning/phases/02-configuration-system/02-CONTEXT.md` — D-01..D-03 (config error screen), D-12/D-13 (`history_size`, server URL deferred to Phase 3)

### External
- Rocket.Chat docs: Accessibility and Appearance (sidebar Categories, 8.9.0+ Enterprise) — https://docs.rocket.chat/docs/accessibility-and-appearance
- Rocket.Chat REST API docs (subscriptions, history, PAT auth headers `X-User-Id` / `X-Auth-Token`) — researcher to locate current version

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `src/data/mod.rs`: `Room`, `RoomKind`, `Folder`, `Message` with redacting `Debug`; the data-source seam (`fake::seed()`) to replace.
- `src/action.rs`: `RoomsLoaded`, `FoldersLoaded` and other `*Loaded` event Actions already exist.
- `src/config/` (`settings.rs`, `diag.rs`, `paths.rs`): load/validate pipeline and the error-screen reporting to extend with `server_url`, credentials.
- `src/ui/sanitize.rs`, `src/ui/status.rs`, Phase 2 `status.error` theme element.
- `src/app/command.rs`: table-driven commands for `:reconnect`.

### Established Patterns
- Elm/TEA: one `App`, one `Action` enum, one channel, one reducer; reducer never reads clocks or does IO.
- Redacting hand-written `Debug` for anything carrying user text or secrets.
- Config errors collected in one pass, shown on an in-TUI error screen; unknown keys ignored.

### Integration Points
- `src/main.rs`/`run`: spawn REST tasks that send Actions on the existing channel.
- New `api/` module (reqwest + serde); add `reqwest` (rustls) to `Cargo.toml`.
- Sidebar/messages state in `src/app/` consumes loaded rooms and per-room history cache.

</code_context>

<specifics>
## Specific Ideas

- Author noted the latest Rocket.Chat supports user-created sidebar folders (Categories) that can hold users, groups, discussions and channels; the Folders section should use them if the server exposes them.

</specifics>

<deferred>
## Deferred Ideas

- Browsing/joining public rooms from the directory — new capability.
- Unread counts, mentions, mark-as-read on open — v2 (MSG-06).
- `ca_cert` / insecure TLS options — revisit in Phase 6 if the company server needs a private CA.
- File logging (tracing) — revisit with DDP in Phase 5 if needed.

</deferred>

---

*Phase: 3-Connect & Read*
*Context gathered: 2026-11-16*
