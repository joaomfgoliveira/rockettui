---
phase: "03"
slug: connect-read
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-11-16"
---

# Phase 3 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | libtest (`cargo test`) + `insta` 1 snapshots; pty e2e via `scripts/smoke_pty.py` (stdlib Python) |
| **Config file** | none (`insta = "1"` in `[dev-dependencies]`); Wave 0 adds `wiremock = "0.6"` |
| **Quick run command** | `cargo test --lib` |
| **Full suite command** | `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check` |
| **Estimated runtime** | ~60 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --lib`
- **After every plan wave:** Run `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check`
- **Before `/gsd-verify-work`:** Full suite green, pty scenarios `quit`, `panic`, `insert`, `config_layout`, `config_keymap`, `config_error`, `connect_read`, `missing_creds`, `config_creds`, `perm_warning`, `unauthorized`, `docker_read`
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

Refined by the planner on 2026-11-16: one row per task of plans 03-01..03-07 (task id = `<plan>-T<n>`). Every test file and scenario below is created by the task in its row (❌ W0 = does not exist yet at plan time).

| Task ID | Plan | Wave | Requirement | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------------|-----------|-------------------|-------------|--------|
| 03-01-T1 | 03-01 | 1 | AUTH-01, ROOM-01, AUTH-03 | env PAT → rooms in the real binary; `X-User-Id`/`X-Auth-Token` sensitive on every call; redirects never followed; missing URL/user/token → error screen naming the env var; bad / userinfo / non-http URL rejected without echo; room mapping and skips; all-or-nothing load | unit + wiremock + pty | `cargo test --lib -- config::connection api:: app::tests::conn_state` ; `python3 -I scripts/smoke_pty.py target/debug/rockettui connect_read` ; `... missing_creds` | ❌ W0 | ⬜ pending |
| 03-01-T2 | 03-01 | 1 | ROOM-01 (D-10) | folders = Favorites + categories (A6); getPreferences failure → Favorites only | unit + wiremock + pty | `cargo test --lib -- api::map::tests::folders api::tests::preferences` | ❌ W0 | ⬜ pending |
| 03-02-T1 | 03-02 | 1 | DLVR-01 | local Rocket.Chat on 127.0.0.1 only; PAT reads subscriptions.get; state files 0600 and git-ignored | integration (Docker) | `python3 -I dev/seed.py check` | ❌ W0 | ⬜ pending |
| 03-02-T2 | 03-02 | 1 | DLVR-01 | full seed; captured fixtures scrubbed (no lastMessage, no md, no secrets) | integration (Docker) | `python3 -I dev/seed.py capture` | ❌ W0 | ⬜ pending |
| 03-03-T1 | 03-03 | 2 | ROOM-02, ROOM-03, MSG-01 | Enter opens a room and issues exactly one history load via the type-correct endpoint; reopen does not refetch; encrypted room → zero requests; Failed reopen retries | reducer unit + wiremock + pty | `cargo test --lib -- app::history api::map::tests::history api::tests::history effect::tests` ; `... connect_read` | ❌ W0 | ⬜ pending |
| 03-03-T2 | 03-03 | 2 | ROOM-03, MSG-01 (D-16) | `encrypted, not supported` / `Loading…` / `Could not load history` / `No messages yet` | snapshot + pty | `cargo test --lib -- ui::messages::tests::placeholder` | ❌ W0 | ⬜ pending |
| 03-04-T1 | 03-04 | 3 | MSG-01 (D-14) | older page uses the raw `latest`; cursor stable on prepend; dedupe; empty page = exhausted; count above cap handled | reducer unit + wiremock + pty | `cargo test --lib -- app::history api::tests::latest_param api::tests::count_above_cap` ; `... connect_read` | ❌ W0 | ⬜ pending |
| 03-04-T2 | 03-04 | 3 | MSG-01 (D-15) | system one-liners, attachment tags, `[encrypted message]`, ciphertext never drawn | unit + snapshot + pty | `cargo test --lib -- api::map::tests::history_kinds ui::wrap::tests::system_row_style ui::messages::tests::message_kinds` | ❌ W0 | ⬜ pending |
| 03-05-T1 | 03-05 | 4 | AUTH-01, AUTH-03 | config.toml credentials; env overrides file per key; values never echoed (Report, Sources Debug) | unit + pty | `cargo test --lib -- config::` ; `... config_creds` | ❌ W0 | ⬜ pending |
| 03-05-T2 | 03-05 | 4 | AUTH-02 | 0o644/0o640/0o604 with file token → persistent warning + stderr line; 0o600 or env token → none; cleartext http warning for non-loopback hosts | unit + snapshot-free render + pty | `cargo test --lib -- config::paths::tests::perm config::tests::perm_warning_rules config::tests::cleartext_warning ui::status::tests::warning_persists` ; `... perm_warning` | ❌ W0 | ⬜ pending |
| 03-06-T1 | 03-06 | 5 | AUTH-01 (D-06), ROOM-02 (D-16) | `:reconnect` and `:retry` reload; failed history retried; no automatic retry | unit + pty | `cargo test --lib -- app::command` ; `... unauthorized` | ❌ W0 | ⬜ pending |
| 03-06-T2 | 03-06 | 5 | AUTH-03 | distinct ApiError per class; cross-origin redirect not followed; GET only; sentinel sweep over every new type; hostile text sanitized; no logging crate | unit (wiremock) | `cargo test --lib -- api::tests::error_mapping api::tests::no_redirect_follow api::tests::only_get_requests api::tests::client_debug_redacts redaction` | ❌ W0 | ⬜ pending |
| 03-07-T1 | 03-07 | 6 | DLVR-01 | the real binary against the seeded Docker server: rooms, folders, open, paging to the start, kinds, encrypted placeholder; token never on screen | pty (Docker) | `python3 -I scripts/smoke_pty.py target/debug/rockettui docker_read` | ❌ W0 | ⬜ pending |
| 03-07-T2 | 03-07 | 6 | DLVR-01 | mapping tested on captured real responses; 9.0.0 compatibility recorded; author UAT | unit + pty + manual | `cargo test --lib -- api::map::tests::docker_fixtures` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `wiremock = "0.6"` dev-dependency; `reqwest` (0.13, `json`+`query`), `serde`, `serde_json` dependencies (03-01-T1)
- [ ] `src/api/fixtures/*.json` hand-written fixtures (03-01-T1, 03-01-T2, 03-03-T1, 03-04-T2); captured Docker fixtures in `src/api/fixtures/docker/` (03-02-T2) tested in 03-07-T2 (kept alongside, not replacing the hand-written ones, which cover skip cases a seed cannot produce)
- [ ] `scripts/smoke_pty.py`: `wait_ready` no longer depends on seeded fake rooms; stdlib `http.server` mock Rocket.Chat on an ephemeral port, passed via `ROCKETTUI_SERVER_URL`, `ROCKETTUI_USER_ID`, `ROCKETTUI_TOKEN` (03-01-T1)
- [ ] Sentinel-based redaction test module `src/redaction.rs` covering new types (03-06-T2; per-type sentinel tests also land in 03-01-T1 and 03-05-T1)
- [ ] Rewrite the Phase 2 test that asserted `server_url` is an unknown key into `config::tests::server_url_is_known_key` (03-05-T1)
- [ ] `dev/compose.yml`, `dev/seed.py`, `dev/README.md`, `.gitignore` entry `/dev/state/` (credentials live in `dev/state/<project>/`, 03-02-T1)

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Real Docker Rocket.Chat: authenticate, list channels/groups/DMs, open a room, scroll to older history, see the encrypted placeholder; judge folder layout (A6) and D-15 wording | DLVR-01 | The author's own use against a real server; folder layout and wording are product judgments (the automated twin is `docker_read`, 03-07-T1) | Follow `dev/README.md`: `python3 -I dev/seed.py up`, `seed`, then `set -a; . dev/state/rockettui-dev/.env.local; set +a; cargo run`, and walk the five ROADMAP success criteria (human-check in 03-07-T2) |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
