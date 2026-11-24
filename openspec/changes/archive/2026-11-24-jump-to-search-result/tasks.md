## 1. API

- [x] 1.1 Extend `dev/probe_search.py` with the context probes (contiguity of `latest`+`inclusive`, `oldest` returning the newest after T, the 100 cap) and run it on 8.9.1 and 9.0.0; record the result in `dev/README.md`
- [x] 1.2 Keep each search result's raw `ts` (search page returns it beside the messages; `ResultsState` holds it), with tests
- [x] 1.3 Add `Client::load_context` (the two history requests, `NotFound` when the result is absent from the older answer, the 100 rule for the newer side) with wiremock tests for: both sides, few newer, exactly 100 newer, result missing, an error from either request

## 2. State and keys

- [x] 2.1 Add `ContextView`, `App.context_view`, `App.context_seq` and `App.pending_context`
- [x] 2.2 Add `Effect::LoadContext`, `Action::ContextLoaded` and `Action::ContextFailed` with redacting `Debug`; run the effect in `effect.rs`
- [x] 2.3 Open a result from the list with the select action: request the window and set the pending id; replace a pending id when another result is opened
- [x] 2.4 Handle the answers: open the view with the result selected, set the newer-not-shown status, error statuses, and drop late answers (list closed, another room, another result)
- [x] 2.5 Route keys while the view is open: motions move the selection with no request, `q`/`Esc`/`Ctrl-c` return to the list, other actions do nothing; closing the list or opening another room drops the view and any pending id
- [x] 2.6 Tests for the reducer behavior above, including that live messages are not added to the view and are in the room afterwards, and that `Debug` prints no text

## 3. Jump to the real chat

- [x] 3.1 Return the raw timestamp of the window's oldest message with `ContextLoaded` (`oldest_raw`), with tests in the API layer
- [x] 3.2 Add `App.detached`; implement the jump: replace the room's messages, set `HistoryState::Loaded` from `oldest_raw`, close view and list, focus the pane, cursor on the result, status when newer messages were not shown
- [x] 3.3 While detached: `on_message` does not append, send/edit/delete/retry are refused with a status message; `G` on the last loaded message and opening another room return (drop messages, history state and the flag; `G` reloads the newest)
- [x] 3.4 Tests: the jump, live message ignored, send refused, older page merges, `G` returns, room switch returns, a non-detached room unchanged

## 4. UI

- [x] 4.1 Draw the view over the messages pane (`render_context`), with the selected row highlighted and the title; snapshot tests including 40x8

- [x] 4.2 Make `?` open the context popup over the results list and the context view (`Kind::SearchResults`, `Kind::SearchContext`, fixed `q Esc` row) and add a key hint to both titles; tests and snapshots

## 5. Verification

- [x] 5.1 Extend the `docker_search` smoke scenario: search, open a result with `Enter`, check the window shows the neighbours, `q` returns to the list, `q` closes it, and `Enter` twice lands in the room on the result
- [x] 5.2 Run `cargo fmt`, `cargo clippy`, `cargo test`, the smoke scenario on 8.9.1, and the probe on 9.0.0
