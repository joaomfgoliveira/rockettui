---
phase: "04"
slug: "send-messages"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-11-17"
---

# Phase 04 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded from `04-RESEARCH.md` § Validation Architecture. Task IDs are filled in once PLAN.md files exist.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | libtest (`cargo test`) + `insta` 1 snapshots + `wiremock` 0.6; pty e2e via `scripts/smoke_pty.py` (stdlib Python) |
| **Config file** | none (`[dev-dependencies] insta = "1"`, `wiremock = "0.6"` in Cargo.toml) |
| **Quick run command** | `cargo test --lib` |
| **Full suite command** | `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check` |
| **Estimated runtime** | ~60 seconds (baseline: 363 tests pass, clippy and fmt clean) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --lib`
- **After every plan wave:** Run `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check`
- **Before `/gsd-verify-work`:** Full suite green, pty scenarios green, manual UAT in the official client done
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 04-01-01 | 04-01 | 1 | MSG-02 | T-04-02, T-04-08 | Wave 0 live probe P0-P7 of chat.sendMessage, chat.getMessage and /me; prints statuses, key names and closed classes only; deletes its rooms | live probe | `python3 -I dev/probe_send.py` | ❌ W0 | ⬜ pending |
| 04-01-02 | 04-01 | 1 | MSG-02 | T-04-04, T-04-05 | Request body is exactly the send contract; auth headers; no redirects | integration (wiremock) | `cargo test --lib api::tests::send` | ❌ W0 | ⬜ pending |
| 04-01-02 | 04-01 | 1 | MSG-02 | T-04-06 (result side), T-04-07 | A send effect reaches the server and its answer is applied by room and id: swap in place, or a fixed status error with no body | integration (wiremock, effect to reducer) | `cargo test --lib -- effect::tests::send app::send` | ❌ W0 | ⬜ pending |
| 04-01-03 | 04-01 | 1 | MSG-02 | T-04-01 | Client ids get a per-run random prefix; no new send type prints message text | unit (sentinel sweep) | `cargo test --lib -- tests::session_id_prefix redaction` | ✅ extend | ⬜ pending |
| 04-02-01 | 04-02 | 2 | MSG-02 | T-04-06, T-04-24 | Enter appends a pending message with a client id and pushes exactly one send effect; input cleared, stays in Insert; success marks it Sent in place and starts the next queued one | unit (reducer) | `cargo test --lib -- app::send app::update::tests::submit` | ❌ W0 | ⬜ pending |
| 04-02-01 | 04-02 | 2 | MSG-02 | — | End to end against the mock: type, Enter, row swapped for the server copy, request recorded once with the exact body | pty | `python3 -I scripts/smoke_pty.py target/debug/rockettui send` | ❌ W0 | ⬜ pending |
| 04-02-02 | 04-02 | 2 | MSG-02 | T-04-03, T-04-06 | Blank draft, no room, encrypted room: no effect, fixed status text, draft kept; per-room ordered queue, a failure never blocks it (D-16); sending while history loads (D-19) | unit | `cargo test --lib -- app::send::tests::guards app::send::tests::queue app::send::tests::send_while` | ❌ W0 | ⬜ pending |
| 04-03-02 | 04-03 | 3 | MSG-02 | T-04-07, T-04-11 | Failure mapping Failed vs Unconfirmed (5xx Unconfirmed per D-18); status text never contains server text, the body or a reconnect hint | unit | `cargo test --lib app::send::tests::fail` | ❌ W0 | ⬜ pending |
| 04-03-01 | 04-03 | 3 | MSG-02 | T-04-10, T-04-12 | retry/discard semantics and default bindings `r` / `dd` | unit | `cargo test --lib -- app::send::tests::retry app::send::tests::discard input::keymap` | ❌ W0 | ⬜ pending |
| 04-03-02 | 04-03 | 3 | MSG-02 | T-04-09 | Reconcile before resending an Unconfirmed message; no duplicate POST | integration (wiremock) | `cargo test --lib effect::tests::reconcile` | ❌ W0 | ⬜ pending |
| 04-01-03, 04-02-01, 04-03-02, 04-06-02 | 04-01, 04-02, 04-03, 04-06 | 1-4 | MSG-02 | T-04-01, T-04-24, T-04-11, T-04-20 | Message text never appears in Debug/Display of new effect, actions, states or errors | unit (sentinel sweep) | `cargo test --lib redaction` | ✅ extend | ⬜ pending |
| 04-05-01, 04-05-02, 04-05-03 | 04-05 | 4 | MSG-02 | T-04-15, T-04-16, T-04-17 | D-13 id dedupe; D-10 per-room drafts; D-05 follow policy and config validation | unit | `cargo test --lib -- app::messages app::tests config::settings app::send::tests::follow` | ⚠️ partial | ⬜ pending |
| 04-03-01, 04-04-01, 04-04-02 | 04-03, 04-04 | 3 | MSG-02 | T-04-13 | New keymap actions, markers and theme elements validate and round-trip | unit | `cargo test --lib -- input::actions config::theme config::keymap` | ✅ extend | ⬜ pending |
| 04-04-01, 04-04-02 | 04-04 | 3 | MSG-02 | T-04-14 | Pending/failed/unconfirmed rows render in both layouts; row model equals renderer | snapshot (insta) | `cargo test --lib -- ui::messages ui::wrap` | ❌ W0 | ⬜ pending |
| 04-06-01, 04-06-02 | 04-06 | 4 | MSG-02 | T-04-19, T-04-21 | Own username from /me once per connection and on :reconnect; pending rows authored by it; :reconnect leaves sends untouched (D-08 as clarified by D-20) | unit + pty | `cargo test --lib -- app::update::tests::me app::command && python3 -I scripts/smoke_pty.py target/debug/rockettui send_author` | ❌ W0 | ⬜ pending |
| 04-03-01, 04-03-02 | 04-03 | 3 | MSG-02 | T-04-09 | End to end against the mock: retry and discard a failed send; an unconfirmed send is reconciled with one POST | pty | `python3 -I scripts/smoke_pty.py target/debug/rockettui send_retry && python3 -I scripts/smoke_pty.py target/debug/rockettui send_unconfirmed` | ❌ W0 | ⬜ pending |
| 04-07-01, 04-07-02 | 04-07 | 5 | MSG-02 | T-04-25, T-04-26 | `:q` with undelivered messages warns with a count only and does not quit; a repeated `:q` or `:q!` quits (D-20); other commands disarm the warning | unit + pty | `cargo test --lib app::command && python3 -I scripts/smoke_pty.py target/debug/rockettui quit_undelivered` | ❌ W0 | ⬜ pending |
| 04-08-01 | 04-08 | 6 | MSG-02 | T-04-22, T-04-23 | End to end against the local Docker server (8.9.1 and 9.0.0): send persists; a restarted binary shows it in rt-private | pty | `RT_PROJECT=rockettui-dev python3 -I scripts/smoke_pty.py target/debug/rockettui docker_send` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `dev/` live-probe script (probes P1-P7 from the research), reading `dev/state/<project>/` itself and printing only statuses, key names and shapes, never secrets or message bodies. **Runs first**: it confirms the unverified server behavior (assumption A1) before reducer work builds on it.
- [ ] `src/app/send.rs` test module (queue, swap, failure, retry, discard, drafts, follow policy)
- [ ] wiremock tests for the send and reconcile endpoints in `src/api/mod.rs` and `src/effect.rs`, including the effect-to-reducer round trip (plan 04-01)
- [ ] `scripts/smoke_pty.py`: `start_mock` serves the send POST and the message GET (it returns 405 on any non-GET today); new `send`, `quit_undelivered` and `docker_send` scenarios added to `SCENARIOS`
- [ ] new `insta` snapshots for pending, failed and unconfirmed rows (`cargo insta review`)
- [ ] update the existing `Message` struct literals (19 literals in 12 files at 4e6f940; 18 gain `send`, the one in submit_input is deleted; plan 04-02) and the `submit_appends_local_message` test
- [ ] `src/app/command.rs` tests for the D-20 quit warning (plan 04-07)

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| The sent message is visible in the official Rocket.Chat client (success criterion 2) | MSG-02 | Needs the official web/desktop client | Plan 04-08 Task 2 (checkpoint:human-verify): run the local `rockettui-dev` stack, send from rockettui in room `rt-private` as the seeded `dev` user (rt-private, not rt-public, so the docker_read newest-page checks stay stable), open `http://localhost:3100` in a browser and confirm the message appears once |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
