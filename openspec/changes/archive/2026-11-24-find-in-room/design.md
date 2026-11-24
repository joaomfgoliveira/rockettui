# Design

## Context

The messages of a room live in `App.messages` (oldest first), the cursor in `MsgView.cursor` as an index, and older pages arrive through `Effect::LoadHistory { before }` and `Action::HistoryLoaded` / `HistoryFailed`, handled in `app/history.rs`. The help overlay (`app.help`), the context popup (`app.context`) and the delete prompt (`app.confirm`) are states checked at the top of the `Action::Key` path in `app/update.rs`, before the keymap; this change adds a fourth in the same place. The messages pane draws wrapped body rows in `ui/wrap.rs::body_rows`, one plain span per row, and `ui/status.rs::render_cmdline` draws the command row only in Command mode. `/`, `n` and `N` are unbound today, and `Esc` in Normal mode only clears the status line. See proposal.md for motivation and scope.

## Goals / Non-Goals

**Goals:**
- Keep the search state small and derived: no cached match list that live messages, edits, deletes or older pages could make stale.
- Reuse the existing older-page request, its one-in-flight rule and its end-of-history guarantee, so the search cannot start a request loop.
- Stay pure in the reducer: the search only pushes the same `Effect::LoadHistory` the cursor motion already pushes.

**Non-Goals:**
- No new `Mode`, no `keymap.toml` table, no new `Action` variant that carries text.
- No match index, no "match 3 of 12" counter: the total is unknown while history is unloaded.
- No match inside a deleted line, an encrypted placeholder or an attachment tag.

## Decisions

**1. State is the query plus a seeking flag, not a list of matches.** `App.search: Option<Search>` with `query: String` and `seeking: bool`, and `App.search_prompt: Option<String>` for the text being typed. Matches are found by running the pure matcher over `App.messages[room]` whenever they are needed (jumping, drawing). The cost is one pass over at most a few thousand bodies per jump, and a draw only matches the visible rows. *Alternative:* store `Vec<usize>` of matching indices. Rejected: every prepend, live insert, edit and tombstone would have to fix it up, and the bugs would be silent.

**2. The prompt is a state checked before the keymap, like `confirm`.** In `update`, after help, context and confirm and before the matcher, `if let Some(prompt) = app.search_prompt` consumes the key: a plain character appends, Backspace deletes (and closes on empty), `Ctrl-u` clears, `Enter` runs, `Esc` and `Ctrl-c` close. `Action::Paste` and `Action::KeyTimeout` are routed the same way (paste joins lines into one, like the command line; the timeout does nothing). The matcher is idle when `/` fires, so nothing is left pending. *Alternative:* add a fourth `Mode`. Rejected: the proposal keeps the badge on the real mode, and a mode would need a keymap table and lockout checks.

**3. Direction: `n` goes toward older messages, `N` toward newer.** Vim's `/` searches forward, but a chat reads upward in time and the proposal has `n` loading older history, so the useful direction for `n` is older. The first jump after `Enter` picks the nearest match at or older than the cursor for the same reason. This is the opposite of Vim's `/` and the same as Vim's `?`. It is stated in the spec and in the help text, not hidden. *Alternative:* follow Vim literally (`n` toward newer). Rejected: the common case, finding something said earlier, would need `N`, and loading history on `N` would read backwards.

**4. Matching: smart case over the sanitized body.** `app/search.rs` exposes a pure `matches(body, query) -> bool` and `find(body, query) -> Vec<Range<usize>>`. The query is lowercased unless it holds an uppercase character, then compared by `str::match_indices` on the body (lowercased in the insensitive case), after `sanitize` so it matches what is drawn. Lowercasing can change byte lengths for some Unicode, so ranges are computed on the sanitized text and its lowercase together and a mismatch in length falls back to "match, no highlight" instead of slicing. No regex crate. *Alternative:* `regex` with `(?i)`. Rejected: a new dependency and regex syntax the proposal excludes.

**5. Jumping goes through the existing cursor model.** A jump computes the target index, then sets `MsgView.cursor` and `top_row` the way `messages::apply_motion` does for `gg` and `G`, so scrolling math stays in one place. `maybe_load_older` is not called after a jump: the search owns that request, so a jump onto the oldest message does not double-request.

**6. Seeking reuses `Effect::LoadHistory` and the in-flight flag.** When a scan toward older finds nothing, and the room is `Loaded` and not exhausted, the search sets `seeking = true` and, if `loading_older` is false, asks `history::request_older(app, room)` (the body of `maybe_load_older` after its cursor check, extracted so both callers share it and the one-in-flight rule). If a page is already in flight, only the flag is set. In `on_older_page`, after the page is merged, `search::on_page(app, room)` runs when `seeking` is set: scan the new messages; on a match, jump and clear `seeking`; with none, ask again, or when the room is now exhausted clear `seeking` and report. `on_failed` for an older page clears `seeking` and reports through the existing error status. Termination follows from the existing guarantee that a page adding nothing marks the room exhausted. Opening another room, `Esc`, or a new search clears `seeking`; `on_page` also checks that the room is still the open one and the query unchanged, so a late page cannot move the cursor.

**7. Highlighting is applied per wrapped row.** `body_rows` already receives one wrapped string per row; it splits each row's span at the match ranges of `find(row, query)` and styles them with the new `theme.search_match`. A match that straddles a wrap boundary is therefore not highlighted, although it still counts for jumping (which scans the whole body). This keeps `row_starts`, scroll math and wrapping untouched. The match style is patched on top of the cursor row's style with `Modifier::REVERSED` removed from the matched spans, so a match on the cursor row stays visible and distinct. *Alternative:* map match ranges through `textwrap`'s whitespace trimming to highlight across rows. Rejected: fragile for a rare case.

**8. Theme and keymap are vocabulary.** `search.match` is added to the theme element list, with a default of black on yellow, and parsed like the other `(pane, element)` pairs. `search_start`, `search_next`, `search_prev` are added to `ACTION_NAMES`, `action_from_name`, `name_of` (exhaustive match) and the default Normal table (`/`, `n`, `N`). `Action` gets three unit variants, so they stay keymap-bindable and `Debug` stays text-free. Typed text never becomes an `Action`: the prompt consumes `Key` events directly.

**9. Cancel and room change clear the search.** `Action::Cancel` in Normal mode clears `app.search` (and the status line, as today). `App::open_room` clears `app.search` and `app.search_prompt`, the same place that clears an edit. A search is about one room's messages, so carrying it over would highlight text in a room it was never run in.

**10. UI.** `render_cmdline` draws `/<query>` with `cmdline.text` when `search_prompt` is set and puts the hardware cursor after it, through the same sanitize and width path as the command line. The context popup gets the three actions on every message kind through `Kind::actions`, appended after the kind's own actions, and the existing "every name is a real action" test covers them. `Status` text is built-in only; the query is shown on the prompt row and nowhere else, and `Search` has no `Debug`.

## Risks / Trade-offs

- [`n` direction differs from Vim's `/`] → Documented in the spec, the help text and the context popup wording ("next match, older"); users can swap the keys in `keymap.toml`.
- [A match across a wrap boundary is not highlighted] → Jumping still finds it, so the cursor lands on a message with no visible highlight. Accepted; revisit if it shows up in daily use.
- [Seeking may fetch many pages for a rare string] → Each page is one request, one in flight, and `Esc` or a room change stops it. Bounded by the room's history.
- [Scanning on every draw] → Only the visible rows are matched at draw time; a jump scans bodies once. Both are plain substring searches.
- [Lowercase length changes for some Unicode] → Falls back to no highlight instead of slicing at a bad byte.
- [`App` grows two fields] → Both are `Option`s with no `Debug`, consistent with `help`, `context` and `confirm`.
