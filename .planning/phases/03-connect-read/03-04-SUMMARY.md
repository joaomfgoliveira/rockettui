---
phase: 03-connect-read
plan: 04
subsystem: api
tags: [rocket-chat, rest, history, paging, tea, ratatui, insta, wiremock]

requires:
  - phase: 03-connect-read
    provides: "HistoryState cache, Effect::LoadHistory with before, Client::load_history sending latest verbatim, Action::HistoryLoaded/HistoryFailed, pty mock history routes (plan 03-03)"
provides:
  - "history::maybe_load_older: moving the messages cursor onto the oldest loaded message requests the next older page (latest = raw server timestamp), one request in flight per room"
  - "Older-page merge: prepend with id de-dup, cursor and screen row kept, oldest_ts moved back, exhausted on an empty or all-duplicate page, Beginning of history status"
  - "MessageKind { Normal, System } and Message.kind"
  - "D-15 text rendering: system one-liners, [attachment: <name>] tags, [encrypted message] for E2E leftovers (ciphertext never mapped into a body)"
  - "pty mock generates uj, attachment and E2E messages; connect_read checks paging, text and absence of ciphertext"
affects: [03-05 reconnect, 03-06 redaction and hostile-text sweep, 04 send]

actuals:
  tokens: 12600
  tasks: 2
  commits: 3
plan_head_before: ef1ed7dc566d67e0ae910da3312ef3d3af2a43a9
plan_head_after: 174159478f8ac301c45cc1fca94c080cf697a750

tech-stack:
  added: []
  patterns:
    - "Paging decisions are pure reducer code: maybe_load_older reads state and pushes an Effect, run() is the only IO"
    - "Prepend keeps the cursor on the same screen row: read the cursor row offset before the merge, rebuild top_row from the new row_starts"
    - "Kind and body of a wire message are decided once in api::map (kind_and_body); the UI only styles by Message.kind"

key-files:
  created:
    - src/api/fixtures/history_kinds.json
    - src/ui/snapshots/rockettui__ui__messages__tests__message_kinds_52x20.snap
  modified:
    - src/app/history.rs
    - src/app/update.rs
    - src/api/mod.rs
    - src/api/wire.rs
    - src/api/map.rs
    - src/data/mod.rs
    - src/data/fake.rs
    - src/action.rs
    - src/app/messages.rs
    - src/ui/messages.rs
    - src/ui/mod.rs
    - src/ui/wrap.rs
    - scripts/smoke_pty.py

key-decisions:
  - "An older page that adds no message (empty, or only ids already held) ends the history; a shorter page than asked never does (RESEARCH Pitfall 6, A-DUP)"
  - "A system type wins over a content object; a content object without a system type is treated as E2E (A-E2E); E2E leftovers keep kind Normal with the fixed text [encrypted message]"
  - "Attachment names are de-duplicated across file, files and attachments titles, because the server lists an uploaded file in all three"
  - "System messages draw author and text with messages.notice, outside the author palette, no new theme element"

patterns-established:
  - "Older-page request carries the raw timestamp string of the oldest held message, never a value rebuilt from whole seconds"
  - "No automatic retry: a failed older page only clears loading_older, moving onto the top message again retries"

requirements-completed: [MSG-01]

coverage:
  - id: D1
    description: "Moving the messages cursor onto the oldest loaded message loads the next older page with latest = that message's raw timestamp; one request in flight; no request away from the top"
    requirement: MSG-01
    verification:
      - kind: unit
        ref: "src/app/history.rs#older_page_trigger"
        status: pass
      - kind: unit
        ref: "src/app/history.rs#no_trigger_away_from_top"
        status: pass
      - kind: unit
        ref: "src/app/history.rs#no_trigger_outside_a_loaded_room"
        status: pass
      - kind: integration
        ref: "src/api/mod.rs#latest_param"
        status: pass
      - kind: integration
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui connect_read"
        status: pass
    human_judgment: false
  - id: D2
    description: "The older page is prepended without duplicates and the cursor stays on the same message at the same screen row"
    requirement: MSG-01
    verification:
      - kind: unit
        ref: "src/app/history.rs#older_prepend_keeps_cursor"
        status: pass
      - kind: unit
        ref: "src/app/history.rs#older_prepend_keeps_screen_row_when_cursor_moved_down"
        status: pass
      - kind: unit
        ref: "src/app/history.rs#older_dedupe"
        status: pass
      - kind: integration
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui connect_read"
        status: pass
    human_judgment: false
  - id: D3
    description: "End of history: an empty or all-duplicate page marks the room exhausted and shows Beginning of history; a short page (count above the server cap) is not the end; a failed page can be retried"
    requirement: MSG-01
    verification:
      - kind: unit
        ref: "src/app/history.rs#exhausted_stops"
        status: pass
      - kind: unit
        ref: "src/app/history.rs#older_failure_allows_retry"
        status: pass
      - kind: integration
        ref: "src/api/mod.rs#count_above_cap"
        status: pass
    human_judgment: false
  - id: D4
    description: "System messages, attachments and E2E leftovers map to the D-15 text, with the E2E ciphertext never reaching a body or the terminal"
    requirement: MSG-01
    verification:
      - kind: unit
        ref: "src/api/map.rs#history_kinds"
        status: pass
      - kind: unit
        ref: "src/api/map.rs#system_texts"
        status: pass
      - kind: unit
        ref: "src/api/map.rs#content_object_is_encrypted_even_without_type"
        status: pass
      - kind: integration
        ref: "python3 -I scripts/smoke_pty.py target/debug/rockettui connect_read"
        status: pass
    human_judgment: false
  - id: D5
    description: "System lines draw dim in both layouts; the kinds snapshot shows system, attachment and encrypted rows"
    requirement: MSG-01
    verification:
      - kind: unit
        ref: "src/ui/wrap.rs#system_row_style"
        status: pass
      - kind: unit
        ref: "src/ui/messages.rs#message_kinds_52x20"
        status: pass
    human_judgment: false
  - id: D6
    description: "Wording of the system one-liners (au, ru, r, room_changed_topic, generic) and how scrolling past the top feels against the real Docker server"
    verification: []
    human_judgment: true
    rationale: "Wording is an author preference (assumption A10); the local Docker server from plan 03-02 was not exercised by this plan, paging is covered by wiremock and the pty mock only"

duration: 6min
completed: 2026-11-17
status: complete
---

# Phase 3 Plan 04: Older history pages and D-15 message kinds Summary

**Scrolling to the oldest loaded message pages in the next older history page with the cursor kept on the same screen row, and history reads as text: dim system one-liners, [attachment: name] tags and [encrypted message] instead of ciphertext**

## Performance

- **Duration:** 6 min
- **Started:** 2026-11-17T10:58:22Z
- **Completed:** 2026-11-17T11:04:30Z
- **Tasks:** 2
- **Files modified:** 15 (2 created, 13 modified)

## Accomplishments

- Tracer: in the real binary, `gg` in general records one `channels.history` request with `latest` equal to the raw timestamp of the oldest loaded message (g-080), the page is prepended and `k` then shows `message 079`. `history::maybe_load_older` runs after every messages-pane motion in the `apply_motion` router; it needs a Loaded room with the cursor on index 0 and no page in flight, and an exhausted room shows `Beginning of history` instead of asking again.
- Older-page merge in `on_loaded`: de-duplicates by id (the page itself included), shifts the stored cursor by the number added and rebuilds `top_row` from the new `row_starts` so the cursor message keeps its screen row, including when the cursor moved down while the page was in flight. A page that adds nothing ends the history, so a misbehaving server cannot cause a request loop; a failed page only clears `loading_older` and moving onto the top message again retries.
- A server that caps the page below `history_size` is handled: `count_above_cap` sends 500, gets 100 messages back in full and the room stays not exhausted; only an empty page ends it.
- D-15 text rendering through a new `MessageKind { Normal, System }`: `api::map::kind_and_body` turns `uj`, `ul`, `au`, `ru`, `r`, `room_changed_topic` and any other type into `* ...` one-liners (author still the acting user), appends ` [attachment: <name>]` once per file, files[] and attachments[] name, and maps E2E leftovers (`t = e2e`, or a `content` object without a system type) to `[encrypted message]` without copying `msg` or `content`. `render_rows` draws System author and text with `messages.notice` in both layouts.
- Phase 1/2 snapshots are unchanged; one new reviewed snapshot (`message_kinds_52x20`).

## Task Commits

1. **Task 1 (tracer): gg at the oldest message -> LoadHistory(before = raw ts) -> older page prepended, cursor kept** - `d0f943c` (feat)
2. **Task 2: D-15 message kinds (TDD)**
   - RED - `80d21f3` (test): `MessageKind` and `Message.kind` scaffolding so the tests compile, fixture, 4 map tests, `system_row_style`, `message_kinds_52x20`
   - GREEN - `1741594` (feat): wire fields, `kind_and_body`, System styling, snapshot, pty mock and smoke checks

**Plan metadata:** recorded in the `docs(03-04)` commit that carries this SUMMARY.

## Tracer feedback gate

Interactive mode, `human_verify_mode` end-of-phase, tracer `<verify>` is `<automated>` only: the three verify commands were run after the tracer commit (17 history/api tests green, `SMOKE OK: connect_read`, `cargo test` 330 passed, clippy and fmt clean). Logged `Tracer verified end-to-end - expanding`; no checkpoint was synthesized.

## TDD Gate Compliance

Task 2 (`tdd="true"`) followed RED -> GREEN; the plan itself is `type: execute`.

- **RED** (`80d21f3`): `cargo test --lib -- api::map::tests ui::wrap::tests::system_row_style ui::messages::tests::message_kinds data::tests` reported 6 failed, 11 passed. Each target failed on its planned assertion: `history_kinds` (`left: (Normal, "carol", "carol")`, `right: (System, "carol", "* joined the room")`), `system_texts` (Normal vs System), `content_object_is_encrypted_even_without_type` and `attachment_alone_has_no_leading_space` (body `U0VOVElORUwtQ0lQSEVS` and `""` instead of the tags), `system_row_style` (`Style::new()` vs `Style::new().dim()`), `message_kinds_52x20` (author cell not dim). No compile error and no zero-test discovery once the `kind` field existed. Semantic assessment: valid RED for the intended reason. The `gsd_run check tdd-red-evidence` classifier has no adapter for `cargo test` output, so the machine check was not run (same as plan 03-03).
- **GREEN** (`1741594`): the same tests pass; snapshot written with `INSTA_UPDATE=always`, checked by eye (rows for normal, join, attachment and encrypted message; dim and bold are asserted on the cells), then the full suite re-run without it.
- **REFACTOR**: none needed.
- The RED commit also carries the `MessageKind` type, the `Message.kind` field and the `kind: MessageKind::Normal` literal edits (needed to compile the tests); the tracer commit `d0f943c` precedes RED by design (Task 1 is `type="tracer"`).

## Files Created/Modified

- `src/app/history.rs` - `maybe_load_older`, `on_older_page` (prepend, dedupe, cursor and row kept, exhausted), `BEGINNING` status text, 8 new tests
- `src/app/update.rs` - `apply_motion` router calls `history::maybe_load_older` after the messages-pane motion; local message literal gets `kind`
- `src/api/mod.rs` - `latest_param` and `count_above_cap` wiremock tests
- `src/api/wire.rs` - `WireMsg.file/files/attachments/content`, `WireFile`, `WireAttach`
- `src/api/map.rs` - `kind_and_body`, `ENCRYPTED_TEXT`, kind-aware `history`, 4 tests
- `src/api/fixtures/history_kinds.json` - seven messages: uj, au, topic, unknown type, attachment, file plus files, e2e
- `src/data/mod.rs`, `src/data/fake.rs`, `src/action.rs`, `src/app/messages.rs`, `src/ui/mod.rs` - `MessageKind`, `Message.kind`, literal updates
- `src/ui/wrap.rs` - System author and body use `messages.notice`; `system_row_style` test
- `src/ui/messages.rs` - `message_kinds_52x20` test; `src/ui/snapshots/...message_kinds_52x20.snap`
- `scripts/smoke_pty.py` - E2E/uj/attachment messages in the r-general mock, paging check (gg, latest, k to `message 079`), text and no-ciphertext checks

## Decisions Made

- An older page with nothing new (empty, or only held ids) ends the history; a shorter page never does (plan A-DUP, Pitfall 6).
- Precedence in `kind_and_body`: `t = e2e` is encrypted; any other `t` is a system message; a `content` object with no `t` is encrypted (A-E2E). The E2E branch ignores `msg`, `content`, files and attachments entirely.
- Attachment names are listed once across `file`, `files` and `attachments[].title` (the plan said once for file and files; a real uploaded file also appears as an attachment title, so the rule was extended to titles).
- `au` and `ru` with an empty target print `* added` / `* removed` without a trailing space.
- No new theme element: `messages.notice` covers system lines.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Attachment names de-duplicated across attachment titles too**
- **Found during:** Task 2 (kind_and_body)
- **Issue:** The plan de-duplicated file against files only. A real Rocket.Chat upload carries the same name in `file`, `files[]` and `attachments[].title`, so titles appended as-is would show every upload twice (`[attachment: a.png] [attachment: a.png]`).
- **Fix:** One ordered name list (file, files, titles) with a contains check; empty names are skipped. The plan's behavior cases are unchanged.
- **Files modified:** src/api/map.rs
- **Verification:** `history_kinds`, `attachment_alone_has_no_leading_space`
- **Committed in:** `1741594`

---

**Total deviations:** 1 auto-fixed (1 bug-avoidance in a rule the plan left open)
**Impact on plan:** none on behavior the plan lists; one extra de-dup rule. Extra tests beyond the plan list are additive: `older_prepend_keeps_screen_row_when_cursor_moved_down`, `no_trigger_outside_a_loaded_room`, `system_texts`, `content_object_is_encrypted_even_without_type`, `attachment_alone_has_no_leading_space`.

## Issues Encountered

- The `k` in the pty smoke can arrive before the older page does (the cursor sits on the oldest message, so it moves nothing). The scenario presses `k` repeatedly (up to 25 times, 0.2 s each) until `message 079` is drawn instead of assuming a timing.
- The RED commit leaves an unused-import warning for `MessageKind` in `src/ui/wrap.rs` (used by tests only until GREEN); it is gone in the GREEN commit and clippy is clean there.

## Authentication Gates

None.

## User Setup Required

None - no external service configuration required.

## Known Stubs

None.

## Threat Flags

None beyond the plan's threat model:
- T-03-16 (E2E disclosure): `kind_and_body` returns the fixed text before reading `msg`; `history_kinds`, `content_object_is_encrypted_even_without_type` and the pty check that the ciphertext sentinel never reaches the terminal cover it. `WireMsg` has no `Debug` and `content` is only tested for presence.
- T-03-17 (request loop): `loading_older` allows one request in flight, an empty or all-duplicate page sets `exhausted`, no automatic retry (`older_failure_allows_retry`, `exhausted_stops`).
- T-03-18 (terminal injection): attachment names and system texts are plain text in `Message.body` and still pass through `sanitize_line`/`sanitize` at draw time; the hostile-text sweep stays plan 03-06.

## Next Phase Readiness

- Ready for 03-05 (reconnect): `HistoryState` now carries a working older-page cycle; `:reconnect` still has to decide what happens to `loading_older` and cached rooms (E-ROOM-02/03 from 03-03 remain open).
- Not exercised: the local Docker server from 03-02 was not run against this plan; paging and kinds are covered by wiremock, unit tests and the pty mock. Wording of system lines (A10) is awaiting the author's confirmation.
- `h`/`l` in the messages pane still go through the router and call `maybe_load_older`, so pressing them on the top message of a room with older history starts the same fetch as `k`; harmless, left as the plan specifies.

---
*Phase: 03-connect-read*
*Completed: 2026-11-17*

## Self-Check: PASSED

- Created files present: `src/api/fixtures/history_kinds.json`, `src/ui/snapshots/rockettui__ui__messages__tests__message_kinds_52x20.snap`.
- Commits on HEAD: `d0f943c`, `80d21f3`, `1741594` (`git rev-list --count` from the recorded base `ef1ed7d` = 3).
- Plan verification re-run: `cargo test` 336 passed, `cargo clippy --all-targets -- -D warnings` clean, `cargo fmt --check` clean, no `*.snap.new`, `SMOKE OK` for connect_read, config_layout and insert.
- Acceptance greps: `pub fn maybe_load_older` 1, `maybe_load_older` in update.rs 1, `Beginning of history` 4, `message 079` in smoke_pty.py 4, `pub enum MessageKind` 1, `MessageKind::System` in wrap.rs 3, `joined the room` in map.rs 2, `[encrypted message]` in map.rs 3, kinds snapshot present.
