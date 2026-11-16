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
- **Before `/gsd-verify-work`:** Full suite green, pty scenarios `quit`, `panic`, `insert`, `config_error`, `config_keymap`, `connect_read`, `unauthorized`, `perm_warning`
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

Task IDs are assigned by the planner; rows below are keyed by requirement and are refined into task rows once PLAN.md files exist.

| Requirement | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|-------------|-----------------|-----------|-------------------|-------------|--------|
| AUTH-01 | env overrides file per key; missing URL/user/token reported with key and place; bad / userinfo / non-http URL rejected | unit | `cargo test --lib -- config::connection config::tests` | ❌ W0 | ⬜ pending |
| AUTH-01 | `X-User-Id` and `X-Auth-Token` sent on every call | unit (wiremock) | `cargo test --lib -- api::tests::sends_auth_headers` | ❌ W0 | ⬜ pending |
| AUTH-02 | 0o644/0o640/0o604 file with token → warning; 0o600 or env token → none | unit | `cargo test --lib -- config::paths::tests::perm_` | ❌ W0 | ⬜ pending |
| AUTH-02 | warning persists across key presses; stderr line after exit | reducer + snapshot; pty | `cargo test --lib -- ui::status::tests::perm_warning` | ❌ W0 | ⬜ pending |
| AUTH-03 | sentinel token / URL userinfo / message body absent from `Debug` and `Display` of every new type | unit | `cargo test --lib -- redaction` | ❌ W0 | ⬜ pending |
| AUTH-03 | cross-origin redirect not followed (token not leaked) | unit (two wiremock servers) | `cargo test --lib -- api::tests::no_redirect_follow` | ❌ W0 | ⬜ pending |
| AUTH-03 | 401 against the real binary: screen and stderr contain no token | pty + mock server | `python3 -I scripts/smoke_pty.py target/debug/rockettui unauthorized` | ❌ W0 | ⬜ pending |
| ROOM-01 | subscriptions + rooms joined on rid → kinds, names, unread bool, sort by activity; `open:false` skipped; folders = categories or Favorites | unit (fixtures) | `cargo test --lib -- api::map::tests` | ❌ W0 | ⬜ pending |
| ROOM-02 | select opens room and issues exactly one history load; reopen does not refetch | reducer unit | `cargo test --lib -- app::history::tests` | ❌ W0 | ⬜ pending |
| ROOM-03 | encrypted room: marker, placeholder, zero HTTP requests | reducer + snapshot + wiremock `expect(0)` | `cargo test --lib -- app::history::tests::encrypted ui::messages::tests::encrypted` | ❌ W0 | ⬜ pending |
| MSG-01 | history mapped oldest-first; older page uses raw `latest`; empty page = exhausted; cursor stable on prepend | unit + wiremock | `cargo test --lib -- api::map::tests::history app::history::tests::older` | ❌ W0 | ⬜ pending |
| MSG-01 | Loading… / Could not load history / empty placeholders | snapshot | `cargo test --lib -- ui::messages::tests` | ❌ W0 | ⬜ pending |
| D-06 | 401 / unreachable / timeout / 429 / 3xx → distinct `ApiError`; `:reconnect` and `:retry` reload | unit | `cargo test --lib -- api::tests::error_mapping app::command::tests::reconnect` | ❌ W0 | ⬜ pending |
| DLVR-01 | rooms, history, scroll and encrypted placeholder work against the Docker server | manual UAT + pty twin | `python3 -I scripts/smoke_pty.py target/debug/rockettui connect_read` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `wiremock = "0.6"` dev-dependency; `reqwest` (0.13, `json`+`query`), `serde`, `serde_json` dependencies
- [ ] `src/api/fixtures/*.json` (hand-written interim fixtures for the tracer; replace with ones captured from the seeded Docker server before phase end)
- [ ] `scripts/smoke_pty.py`: `wait_ready` must stop depending on seeded fake rooms; add a stdlib `http.server` mock Rocket.Chat on an ephemeral port, passed via `ROCKETTUI_SERVER_URL`, `ROCKETTUI_USER_ID`, `ROCKETTUI_TOKEN`
- [ ] Sentinel-based redaction test module covering new types (pattern exists in `src/data/mod.rs`, `src/action.rs`)
- [ ] Rewrite `config::tests::server_url_is_unknown_key` (`server_url` becomes a known key)
- [ ] `dev/compose.yml`, `dev/seed.py`, `dev/README.md`, `.gitignore` entry for `dev/.env.local`

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Real Docker Rocket.Chat: authenticate, list channels/groups/DMs, open a room, scroll to older history, see the encrypted placeholder | DLVR-01 | Needs a running local server with seeded data | Follow `dev/README.md`: start the compose stack, run `dev/seed.py`, export the generated `dev/.env.local`, run the binary, walk the five ROADMAP success criteria |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
