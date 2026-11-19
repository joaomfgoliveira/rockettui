---
phase: 04-send-messages
plan: 01
subsystem: api
tags: [rocket-chat, chat.sendMessage, reqwest, wiremock, tea-reducer, redaction, python-probe]

requires:
  - phase: 03-connect-read
    provides: api::Client with redirect-disabled capped GET, effect::run and spawn, history mapping, redaction sweep
provides:
  - dev/probe_send.py, the Wave 0 live probe of the send contract (P0-P7), with the recorded server answers
  - Client::send_message (POST chat.sendMessage with a client _id) and read_json shared by GET and POST
  - map::message, the one per-message mapper used by history and the send answer
  - Effect::SendMessage with the redacting SendText; Action::MessageSent and Action::SendFailed
  - send::on_sent (in-place swap by room and id), send::on_failed and send::send_error_text (fixed reasons)
  - App.id_prefix, App.own_username slot, session_id_prefix (per-run random client id prefix)
  - sentinel sweep over every new send type
affects: [04-02, 04-03, 04-04, 04-05, 04-06, 04-07, 04-08]

actuals:
  tokens: 14000
  tasks: 3
  commits: 4

tech-stack:
  added: []
  patterns:
    - "Send result applied by (room, id), never by the current room"
    - "Hand-written Debug on every type that can hold message text (SendText prints its byte length)"
    - "Closed set of fixed status reasons per ApiError kind, never server text and never ApiError's Display"
    - "Per-process id prefix drawn once in run(), never in the reducer; tests keep the deterministic local- prefix"

key-files:
  created:
    - dev/probe_send.py
    - src/app/send.rs
  modified:
    - dev/README.md
    - src/api/wire.rs
    - src/api/map.rs
    - src/api/mod.rs
    - src/effect.rs
    - src/action.rs
    - src/input/actions.rs
    - src/app/mod.rs
    - src/app/update.rs
    - src/lib.rs
    - src/redaction.rs

key-decisions:
  - "SendText Debug prints SendText(<N> bytes) with N the byte count in angle brackets, following the plan's literal form (Paste uses Paste(<N bytes>))"
  - "on_failed keeps its (room, id) parameters as _room and _id: the failure only sets the status line until SendState arrives in plan 04-02"
  - "A 400 from chat.sendMessage maps to ApiError::BadResponse through the existing status_error, so a read-only, muted, archived or oversize room reads `message not sent: unexpected response`; plan 04-03 can refine the wording"
  - "session_id_prefix draws two RandomState hash_one values; no new dependency"

patterns-established:
  - "Pattern: one per-message mapper (map::message) shared by every path that decodes a message"
  - "Pattern: wiremock round-trip test drives effect::run and then update on an app_80x24() app"

requirements-completed: [MSG-02]

coverage:
  - id: D1
    description: "Wave 0 probe confirms chat.sendMessage keeps the client _id on rockettui-dev 8.9.1 and records every other server answer"
    requirement: MSG-02
    verification:
      - kind: other
        ref: "python3 -I dev/probe_send.py (last line PROBE OK)"
        status: pass
    human_judgment: false
  - id: D2
    description: "Client::send_message posts exactly {_id, rid, msg} with byte-exact text and maps the answer; a redirect forwards nothing"
    requirement: MSG-02
    verification:
      - kind: unit
        ref: "src/api/mod.rs#send_message_contract, send_message_errors"
        status: pass
    human_judgment: false
  - id: D3
    description: "A send result is applied by room and id: in-place swap on success, fixed `message not sent: <reason>` status on failure, no history reload"
    requirement: MSG-02
    verification:
      - kind: integration
        ref: "src/effect.rs#send_round_trip_swaps_in_place, src/app/send.rs#on_sent_other_room_and_unknown_id, fail_status_has_no_body"
        status: pass
    human_judgment: false
  - id: D4
    description: "No send type prints message text; client ids are unique per run"
    requirement: MSG-02
    verification:
      - kind: unit
        ref: "src/redaction.rs#sentinel_sweep, src/lib.rs#session_id_prefix_shape, src/app/mod.rs#send_fields_default"
        status: pass
    human_judgment: false

duration: 5min
completed: 2026-11-19
status: complete
plan_head_before: 6a81de62bf7f60119d44498f5765f5d445894e30
plan_head_after: f630bb98dc436765e46aed9ef9d3a281b3263654
commits: 4
---

# Phase 4 Plan 1: Send Wire, Client and Result Path Summary

**chat.sendMessage with a client-chosen _id confirmed live on Rocket.Chat 8.9.1, then wired end to end: Effect::SendMessage through Client::send_message, answered by an in-place swap by room and id or a fixed status reason, with a per-run random id prefix.**

## Performance

- **Duration:** 5 min
- **Started:** 2026-11-19T19:11:10Z
- **Completed:** 2026-11-19T19:16:13Z
- **Tasks:** 3
- **Files modified:** 13 (2 created, 11 modified)

## Accomplishments

- The Wave 0 probe ran against the real local server: the server stores a message under the client's _id and returns it unchanged (P1), so D-15 holds and no blocker exists.
- One send is one POST. The body is built from Serialize structs and holds exactly `_id`, `rid` and `msg`; text with a quote, a newline, `é` and an emoji arrives byte for byte (wiremock `body_json`).
- The answer comes back as Action::MessageSent or Action::SendFailed and is applied by room and id: success replaces the held message at the same index (server time and author, no history reload), failure sets `message not sent: <reason>` and leaves the messages unchanged.
- A 3xx is `Redirected`; the redirect target receives no request, so neither text nor credentials are forwarded.
- Client ids carry a per-process random prefix (`rt` + 32 hex digits) while tests keep `local-`, and the sentinel sweep covers SendText, Effect::SendMessage, MessageSent and SendFailed.

## Probe results (rockettui-dev 8.9.1)

Output of `python3 -I dev/probe_send.py`, copied as printed (statuses, key names and closed classes only):

```
P0 GET me status=200 keys=_id,_updatedAt,active,avatarUrl,email,emails,isOAuthUser,name,roles,services,settings,status,success,username has_username=yes
setup rooms=3 archived=yes
P1 POST chat.sendMessage status=200 keys=message,success message_keys=_id,_updatedAt,channels,md,mentions,msg,rid,ts,u,urls user_keys=_id,name,username id_match=yes ts=iso_ms error_class=absent
P2 POST chat.sendMessage status=400 keys=error,success error_type=absent error_class=type_error
P3a GET chat.getMessage status=200 keys=message,success id_match=yes
P3b GET chat.getMessage status=400 keys=success error_class=absent
P4 POST chat.sendMessage status=400 keys=error,success error_type=absent error_class=readonly
P5a POST chat.sendMessage status=400 keys=error,success error_type=absent error_class=muted
P5b POST chat.sendMessage status=400 keys=error,success error_type=absent error_class=archived
P6 setting max_allowed_size=5000
P6 POST chat.sendMessage status=400 keys=details,error,errorType,success error_type=error-message-size-exceeded error_class=size_exceeded size=limit_plus_one
P7 POST chat.postMessage status=400 keys=error,errorType,success error_type=invalid-params error_class=invalid_params
cleanup deleted=4/4
PROBE OK
```

| Probe | RESEARCH expects | Observed | Consequence |
|-------|------------------|----------|-------------|
| P1 chat.sendMessage, fresh _id | 200, message._id == client id, ts iso_ms, message.u.username present | 200, id_match yes, ts iso_ms, user_keys `_id,name,username` | As expected: D-15 holds, Task 2 proceeds |
| P1 ts shape | iso_ms | iso_ms | No change |
| P2 same _id again (A1) | 400, error_class type_error | 400, error_class type_error, no errorType | As expected: a repeat is a 400 with an unhelpful TypeError-style error; plan 04-03's reconcile (chat.getMessage first) is needed, as planned |
| P3a chat.getMessage known id | 200, id_match yes | 200, id_match yes | Plan 04-03 can reconcile |
| P3b chat.getMessage unknown id | 400 | 400 (body keys `success` only) | As expected: any non-200 means absent |
| P4 read-only | 400 readonly | 400 readonly | As expected |
| P5a muted | 400 muted | 400 muted (the mute call worked, not SKIPPED) | As expected |
| P5b archived | 400 archived | 400 archived | As expected |
| P6 oversize | 400 size_exceeded | 400, errorType `error-message-size-exceeded`, limit 5000 | As expected |
| P7 replaced endpoint with _id | 400 invalid_params | 400, errorType `invalid-params` | As expected: D-15 stays locked |
| P0 /api/v1/me | 200, has_username yes | 200, has_username yes | Plan 04-06 can read the own username |

No contradiction: every answer matched RESEARCH. Because every 4xx here (P2 to P6) is a plain 400, the client maps all of them to `ApiError::BadResponse` through the existing `status_error`; plan 04-03 decides whether the wording needs to distinguish them (read-only, muted, archived and oversize are told apart only by the server's `error` text, which this plan never decodes).

## Task Commits

1. **Task 1 (Wave 0): live probe of the send contract** - `21969be` (feat)
2. **Task 2 (tracer): effect, client, wire, map, action and reducer send path** - `602341f` (feat)
3. **Task 3 (TDD): per-run id prefix, own-username slot, redaction sweep**
   - RED `ffee707` (test): `session_id_prefix_shape` and `send_fields_default` fail on their assertions
   - GREEN `f630bb9` (feat)

**Plan metadata:** the docs commit that holds this file (docs: complete plan).

## TDD notes (Task 3)

- **RED:** the two new tests ran against compiling stubs (`session_id_prefix` returned `""`, `App.id_prefix` started empty). `tests::session_id_prefix_shape` failed with `left: 0, right: 34` and `app::tests::send_fields_default` with `left: "", right: "local-"`; both are the planned assertions. The extended `sentinel_sweep` passed already because it sweeps types shipped by Task 2 (a regression guard, not new behavior). Semantic assessment: the target tests executed and failed for the intended reason; cargo's console report is not one of the formats `gsd_run check tdd-red-evidence` reads, so it was not run through the classifier (`workflow.tdd_mode` is off and this is not a `type: tdd` plan).
- **GREEN:** `session_id_prefix` formats two `RandomState::hash_one` draws; `with_config` sets `local-`. All 374 tests pass.
- **REFACTOR:** none needed.

## Files Created/Modified

- `dev/probe_send.py` - Wave 0 probe P0-P7; secrets read in-process, every line passes `out()` which refuses a secret value, rooms it creates are deleted in a finally block
- `dev/README.md` - "Probe the send contract" section
- `src/api/wire.rs` - `MessageResp { message }`
- `src/api/map.rs` - `map::message`, extracted from the history loop; `message_single` test
- `src/api/mod.rs` - `read_json`, `SendBody`/`SendMsg`, `Client::send_message`; `send_message_contract` and `send_message_errors`
- `src/effect.rs` - `SendText`, `Effect::SendMessage`, run arms, the four send tests
- `src/action.rs` - `MessageSent`, `SendFailed` and their redacting Debug arms
- `src/input/actions.rs` - the two variants in the unnamed list
- `src/app/send.rs` - `on_sent`, `on_failed`, `send_error_text` and their tests
- `src/app/mod.rs` - `pub mod send`, `id_prefix`, `own_username`, LOCAL_AUTHOR doc
- `src/app/update.rs` - the two reducer arms
- `src/lib.rs` - `session_id_prefix`, set once in `run`
- `src/redaction.rs` - sweep entries for the new send types

## Decisions Made

- `SendText` Debug is `SendText(<N> bytes)` with N the byte count inside angle brackets, as the plan wrote it (the existing `Paste` arm uses `Paste(<N bytes>)`); the redaction tests assert the byte count form, not a particular layout elsewhere.
- `on_failed` takes `(room, id, error)` for the later plans but uses only the error in this plan, so the two parameters are `_room` and `_id`.
- All 4xx answers share `ApiError::BadResponse` for now; nothing decodes the server's error text (T-04-07).

## Deviations from Plan

None - plan executed exactly as written.

The plan's `<verify>` for Task 1 lists the probe's printed lines, including "SKIPPED" for P5a; the probe was not skipped, the mute call worked. `cargo fmt` also reformatted nothing outside the files this plan touched.

## Issues Encountered

None.

## Known Stubs

None. `Effect::SendMessage.reconcile` is accepted and ignored on purpose (this plan always POSTs); plan 04-03 gives it the chat.getMessage-first branch, as the plan states. `App.own_username` stays `None` until plan 04-06, with the documented `LOCAL_AUTHOR` fallback. Nothing pushes `Effect::SendMessage` yet: Insert-mode Enter still appends locally until plan 04-02.

## Threat Flags

None beyond the plan's register. T-04-01 (Debug of send types), T-04-02 (probe output), T-04-04 (exact body), T-04-05 (redirect), T-04-07 (fixed status text) and T-04-08 (probe deletes only its own rooms, loopback only) are mitigated and covered by tests or by the probe's own output rule; T-04-SC-01 holds (Cargo.toml unchanged).

## Next Phase Readiness

Ready for plan 04-02: `Effect::SendMessage`, the two Actions, `send::on_sent`/`on_failed` and `App.id_prefix` exist, so `send::submit` can build ids as `id_prefix` + `local_seq`, push the effect and add `SendState`. Plan 04-03 reads the probe table above (P2 and P3).

## Self-Check: PASSED

All ten created or modified source files exist on disk and the four commits (`21969be`, `602341f`, `ffee707`, `f630bb9`) are ancestors of HEAD. `cargo test` (374 passed), `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, the pty scenarios `insert`, `connect_read`, `unauthorized` and `quit` (all `SMOKE OK`) and `python3 -I dev/probe_send.py` (`PROBE OK`) pass; `find src -name '*.snap.new'` prints nothing and `git status` lists nothing under `dev/state`.
