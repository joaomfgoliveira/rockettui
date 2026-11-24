# Design

## Context

See `proposal.md` for the motivation and `specs/` for the behavior. Current state that shapes the approach:

- Commands run from a table in `app/command.rs`; `execute` splits the line on whitespace and hands each command a token slice, so inner spacing of the arguments is lost.
- IO is an `Effect` pushed by the reducer and run in `effect.rs`, which sends `Action`s back on the one channel. History paging already guards "one request in flight" with `loading_older` and addresses answers by room.
- The help overlay is app state (`App.help: Option<usize>`) that takes keys while open, not a mode. The results list follows the same shape.
- `chat.search` answers with the history message shape (spike `dev/probe_search.py`, 8.9.1), so `map::history`-style mapping is reusable.
- Theme elements are a fixed list of `group.name` keys in `config/theme.rs`.

## Goals / Non-Goals

**Goals:**
- Reuse the message type, mapper and message row drawing; add as little new code as the spike allows.
- Keep late answers harmless, the same way history answers are.

**Non-Goals:**
- No jump to a result in context, no cross-room search, no filter UI (see proposal).
- No client-side re-filtering or re-sorting of what the server returns.

## Decisions

**Results are `App.results: Option<ResultsState>`, not a mode.** It holds the room id, the query, the loaded messages, the selection, `top_row`, `exhausted` and `loading_more`. While `Some`, the key handler routes the motion actions and the close keys to it before the normal keymap, as the help overlay does. Alternative: a new `Mode::Results`. Rejected: the spec requires the mode badge to stay unchanged, and a mode would need keymap tables and help rows for it.

**Reuse the keymap's motion actions; close keys are fixed.** `move_down`, `move_up`, `goto_top`, `goto_bottom`, `half_page_*` are interpreted by the results state, so user remaps carry over. `q`, `Esc` and `Ctrl-c` close it, like help. No new action names, so no keymap vocabulary change. Every other action is swallowed while the list is open.

**One effect pair.** `Effect::SearchRoom { room, query: SendText-like wrapper, offset, count }` and `Action::SearchPage { room, query_id, offset, result }`. The query wrapper prints only its byte length in `Debug`, like `SendText`. The answer carries a search id (a counter on `App`) so an answer from a closed or replaced search is dropped, plus the room, so an answer for another room is dropped too. The results message list is `Vec<Message>` and is never put in an `Action` `Debug` print: the `Action` variant carries messages already (history does) and must keep the existing redaction behavior.

**Paging by offset.** `chat.search` takes `count` and `offset`. The first request is offset 0; the next is offset = number loaded. A page shorter than `count` sets `exhausted`. `loading_more` is the in-flight guard. The page size is a constant chosen while implementing from the probe's observed maximum, not above it.

**Raw argument for `:search`.** `execute` passes the text after the command name, trimmed at both ends but not re-joined, as a third argument to every command function. Alternative: join the tokens with one space. Rejected: `/regex/` queries and exact spacing would silently change.

**Refusals happen in the command, before any effect.** Empty query, no open room and encrypted room are checked in `cmd_search`; no state changes on refusal, so an open list stays.

**Drawing.** The list is drawn over the messages pane by a new `ui/results.rs`, using the existing message row drawing for author, time and text. It is not a copy of the messages pane: it has no unread marker, day separators or send state. Highlighting the query text inside results is not done (server syntax and regex make "the matched text" ill-defined).

**Theme.** Three elements `results.border`, `results.title`, `results.selected`, added to the element list, the `Theme` struct, its defaults and the key lookup, beside `help.*`.

## Risks / Trade-offs

- [Server behaviour verified on 8.9.1 only] → Run `dev/probe_search.py` against 9.0.0 while implementing and write down any difference before finishing.
- [A new state that takes keys can swallow input if its exit is missed] → One close path (`q`, `Esc`, `Ctrl-c`) and a test that every non-motion action is a no-op while open; opening another room also closes the list.
- [Results can contain deleted or system messages the server returns] → Drawn with the existing row logic; no extra filtering.
- [Message text could leak through `Debug`] → The query wrapper redacts; a test formats the effect and the action and checks no text appears.

## Migration Plan

No data or config migration. Existing `theme.toml` files keep working because the new elements have defaults. Rollback is reverting the change.
