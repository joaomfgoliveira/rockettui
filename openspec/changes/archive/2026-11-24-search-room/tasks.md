## 1. API

- [x] 1.1 Run `dev/probe_search.py` against 8.9.1 and 9.0.0; record the maximum page size and any difference in `dev/README.md`
- [x] 1.2 Add `Client::search_room(room, query, offset, count)` calling `chat.search` (`roomId`, `searchText`, `count`, `offset`) and mapping the answer with the existing message mapper, with fixture-based tests (results, empty, error status)

## 2. Command and state

- [x] 2.1 Pass the raw text after the command name to command functions (`execute` and `CommandDef.run`), with a test that inner spacing is kept
- [x] 2.2 Add `ResultsState` and `App.results`, plus the search id counter
- [x] 2.3 Add `Effect::SearchRoom` with a redacting query wrapper, and `Action::SearchPage`; run the effect in `effect.rs`
- [x] 2.4 Add the `:search` command row (`cmd_search`) with the refusals: empty query, no open room, encrypted room; tests that no effect is pushed and an open list stays
- [x] 2.5 Handle `SearchPage`: open the list on the first page, append later pages, set `exhausted` on a short page, status lines for no results, failure and access refused, and drop late answers (closed list, newer search, other room)
- [x] 2.6 Route keys while the list is open: motions move the selection, `q`/`Esc`/`Ctrl-c` close, other actions do nothing; request the next page when the selection reaches the last result, with the in-flight guard; opening another room closes the list
- [x] 2.7 Tests that `Debug` of the effect and action show no query or message text

## 3. UI and theme

- [x] 3.1 Add `results.border`, `results.title`, `results.selected` to the theme (element list, struct, defaults, lookup) with a default-and-override test
- [x] 3.2 Draw the list over the messages pane in `ui/results.rs` (title with the query, newest first, selected row, scrolling) with snapshot tests

## 4. Docs and verification

- [x] 4.1 Add `:search` to the help overlay command list and the README/docs where `:help` is described
- [x] 4.2 Run `cargo fmt`, `cargo clippy` and `cargo test`; do a manual check against the local 8.9.1 and 9.0.0 servers (search, paging, no results, closing, encrypted-room refusal)
