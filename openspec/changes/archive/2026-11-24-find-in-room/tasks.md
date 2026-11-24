# Tasks

## 1. Matcher

- [x] 1.1 Add `src/app/search.rs` (declared in `app/mod.rs`) with the pure `matches(body, query)` and `find(body, query) -> Vec<Range<usize>>`: plain text, smart case, run on the sanitized body, with the length-mismatch fallback from design decision 4. Verify with unit tests: lowercase query matches any case, uppercase query is case-sensitive, `a.c` does not match `abc`, several occurrences, an empty query matches nothing, a Unicode body never panics or slices off a char boundary.
- [x] 1.2 In the same module add `Search { query, seeking }` with no `Debug` derive, and `App.search: Option<Search>` and `App.search_prompt: Option<String>` (`None` in `with_config`). Verify with `cargo build` and a test that `Search` and `App` stay free of a `Debug` impl, as for `help` and `confirm`.

## 2. Actions, keymap and theme vocabulary

- [x] 2.1 Add `Action::SearchStart`, `SearchNext`, `SearchPrev` (unit variants, `Debug` arms) and the names `search_start`, `search_next`, `search_prev` in `input/actions.rs` (`ACTION_NAMES`, `action_from_name`, `name_of`), with default Normal rows `/`, `n`, `N` in `input/keymap.rs`. Verify with keymap tests: the three defaults resolve, rebinding and `"n" = "none"` work, the names round-trip, and no existing preset or lockout test breaks.
- [x] 2.2 Add the three descriptions to `DESCRIPTIONS` in `app/help.rs` (say that `n` goes toward older messages). Verify with the existing help tests (every action has a description) plus a test that the Normal section lists `/`, `n` and `N`; update the help snapshots.
- [x] 2.3 Add the `search.match` element to `config/theme.rs` (element list, `(pane, element)` lookup, default black on yellow) and the `search_match` field on `Theme`. Verify with a theme test: the default is set, `[search.match] bg = "#ff8800"` overrides it, and an unknown `search.*` name is a config error.

## 3. Prompt and first match

- [x] 3.1 Handle `Action::SearchStart` in the reducer: open the prompt only with the messages pane focused and a room open, otherwise do nothing. Consume keys, `Paste` and `KeyTimeout` while `search_prompt` is set, before the keymap, as in design decision 2 (cap the query at `MAX_CMDLINE`; Backspace on empty, `Esc` and `Ctrl-c` close). Verify with reducer tests: opening, typing `j` does not move the cursor, paste joins lines, cancel leaves no search, no prompt from the sidebar or input bar or without a room, and the mode stays Normal throughout.
- [x] 3.2 On `Enter` run the search: an empty query only closes the prompt; otherwise set `app.search`, move the cursor to the nearest matching message at or older than it using the cursor and scroll model of `messages` (design decision 5), and show a status line when nothing is found. Verify with tests: nearest match, cursor already on a match, no match leaves the cursor and says so, a second search replaces the first.
- [x] 3.3 Draw the prompt: `render_cmdline` shows `/<query>` in `cmdline.text` with the hardware cursor after it, through the sanitize and width path of the command line. Verify with a render test at 80x24 and the smallest supported size (snapshot), and a check that a hostile query is sanitized.

## 4. Jumping and older history

- [x] 4.1 Implement `SearchNext` and `SearchPrev` (honouring a count, no wrap, status lines for no search and for no more matches in that direction). Verify with tests: `n` goes older, `N` newer, `3n` repeats, `N` on the newest match stays and reports, `n` with no search reports.
- [x] 4.2 Extract the request half of `history::maybe_load_older` into `history::request_older` and have both callers use it. Add seeking: when a scan toward older finds nothing in a Loaded, non-exhausted room, set `seeking` and request a page, or only set the flag when one is in flight. Verify with the existing `history` tests still passing and a test that a page already in flight produces no second effect.
- [x] 4.3 Call `search::on_page` from `history::on_older_page` and clear `seeking` from `on_failed`, per design decision 6: jump on a match, ask again when there is none, stop with a status line when the room is exhausted, and ignore the page if the room or query changed. Verify with tests: match in the first older page, match only in the second page, no match to the end of history (no further effect, cursor unmoved), a failed page stops with the error status, and switching room or `Esc` while loading stops further requests.

## 5. Highlight, clearing and context popup

- [x] 5.1 Style matches in `ui/wrap.rs::body_rows` per design decision 7, only when `app.search` is set and the message is not deleted, keeping the cursor row visible (no reversed modifier on matched spans). Verify with render tests: every occurrence styled, a themed `search.match` applies, a match on the cursor row is distinct, a deleted line and a hostile body are untouched, and `message_rows` heights are unchanged with and without a search (so scroll math still agrees with drawing).
- [x] 5.2 Clear the search with `Action::Cancel` in Normal mode and in `App::open_room`, and check live messages keep the highlight without moving the cursor. Verify with tests: `Esc` clears and `n` then reports no search, opening another room clears it, a live matching message is styled and the cursor stays.
- [x] 5.3 List `search_start`, `search_next` and `search_prev` after the kind's own actions for every message kind in `context::Kind::actions` (rows with no bound key are left out as today). Verify with the context tests: the own, unsent and other-message popups show the search rows with live keys, `"N" = "none"` removes its row, and the "every name is a real action" test passes; update the context snapshots.

- [x] 5.4 List `goto_top` ("jump to the oldest message") and `goto_bottom` ("jump to the newest message") for every message kind in `context::Kind::actions`, before the search rows. Verify with the context tests: the own, unsent and other-message popups show `gg` and `G`, rebinding or removing one follows the keymap, and the own-message snapshot is updated.

## 6. Docs and integration

- [x] 6.1 Document `/`, `n`, `N`, the three action names, the `search.match` element and the older-first direction of `n` in the README or user docs where the keymap and theme are described (find them with `grep -rn show_context README.md docs 2>/dev/null`). Verify that every key and name written there matches `cargo test` output for the defaults.
- [x] 6.2 Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test`, then `openspec validate find-in-room --strict`. Verify all four exit zero.
- [x] 6.3 Manual check against the local Docker Rocket.Chat 8.9.1: search a loaded word, press `n` past the oldest loaded message into older pages, search for a word that does not exist and watch it stop at the beginning of history, press `Esc` mid-search, and check that a live message containing the query is highlighted.

## Workflow follow-up

- Archive the change after the manual check, syncing the delta specs into `openspec/specs/` (`message-search` is a new capability).
