# Design

## Context

The app is Elm/TEA: one `App`, one `Action` enum, one reducer. Overlays (help, context popup, reaction picker) are state on `App`, checked in `Action::Key` before the keymap, and they are not modes. `App::open_room` is the single entry point for opening a room; it handles drafts, the mark-read timer and focus. The reaction picker (`src/app/react.rs`) already has the shape this needs: a query, a ranked list, a cursor, key and paste handlers, and a stale check after live updates. The keymap comment in `src/input/keymap.rs` reserved `Ctrl-k` for this. See proposal.md for motivation and the `room-switcher` spec for behavior.

## Goals / Non-Goals

**Goals:**
- Follow the reaction picker's structure so the two overlays read alike.
- Keep ranking a pure function of the room list, the query and the recent list, so it is unit-testable without a `Frame`.

**Non-Goals:**
- Fuzzy matching on anything but the room name (no topics, no members).
- Insert-mode opening, persisting the recent list, joining unjoined rooms.

## Decisions

**Overlay state, not a mode.** `App.switcher: Option<Switcher>` holds the query, the selected index and the built list. The reducer routes keys to it first when it is `Some`, like `react_picker`. Alternative: a `Mode::Switch`. Rejected: the badge would lie about where focus returns, and every mode match in the keymap, status bar and help would grow a branch. The picker already established the overlay pattern.

**Opens already typing.** Unlike the reaction picker, which opens browsing, the switcher takes characters as the query at once. Navigation uses `Up`/`Down`, `Ctrl-j`/`Ctrl-k` (the fzf and telescope habit, and `Ctrl-k` already opens it) and `Ctrl-n`/`Ctrl-p` (as in the reaction picker), none of which are characters, so no browsing/typing split is needed. The switcher's reason to exist is speed, so no second key.

**Recent list as `Vec<RoomId>` on `App`.** `open_room` moves the opened id to the front (removing any earlier occurrence) when the room actually changes. Alternative: a `visited_at` clock stamp per room. Rejected: the reducer reads no clock, and a list gives order for free. Capped at the number of rooms by construction, so there is no growth. Rooms that disappear are skipped when the list is read, not purged.

**Order is a sort key, not a score, for the empty query.** The key is `(!unread, recent_rank or MAX, Reverse(last_activity), name)`: unread first, then most recently visited, then never-visited rooms by activity, then name. Non-empty query: key `(match tier, empty-query key)`.

**Hand-rolled subsequence matcher, four tiers.** Walk the lowercased name once; tier 0 prefix, 1 first matched char at a word start (after `-`, `_`, `.`, space, `/`), 2 contiguous run, 3 scattered. A small function returning `Option<Tier>`. Alternative: the `nucleo` crate. Rejected for now: room lists are small, a dependency is more to carry for a learning project, and the tiers are easy to read and test. The matcher sits behind one function, so swapping it later touches one place.

**Rebuild, don't patch, on stale.** After each reduced action, if the switcher is open, rebuild the list from `app.rooms` and re-find the selected room by id. This matches `react::close_if_stale` being called at the top of the reducer, and avoids holding indices into a list that changed.

**Reuse the sidebar's row text.** The unread count and mention badge formatting already exists for the sidebar (`rooms` spec). The switcher calls the same helper instead of copying it, so the two cannot drift. The popup is drawn centered over the messages pane, with the search line on top and a scrolling list, in the style of the reaction picker popup.

**Choosing goes through `open_room`.** `Enter` calls `open_room(id)` and closes the overlay. `open_room` is also where the recent list is updated, so sidebar `Enter` and the switcher both feed it. Choosing the already-open room only closes the overlay, since `open_room` already ignores a same-room open apart from focus; the switcher skips even that so the focus rule in the spec holds (the room stays as it was).

## Risks / Trade-offs

- [`Ctrl-j` is the same byte as `Enter` in a legacy terminal, so it could open a room instead of moving down] → kitty reports them apart (checked by hand in task 4.2); `Ctrl-n`/`Ctrl-p` and the arrows stay as keys that never clash.
- [Spec says focus goes to messages on choose, but open-room-chosen "keeps the room as it is"] → Both hold: opening another room focuses messages; choosing the current room only closes the overlay and leaves focus alone.
- [`Ctrl-k` may be taken by the terminal or tmux] → It is rebindable through `switch_room`.
- [Hand-rolled ranking may feel off for odd names] → Tiers are one small function with tests; replaceable by `nucleo` without touching the rest.
- [The recent list resets at restart, so the first `Ctrl-k` of a session is ordered by activity] → Accepted; persisting is a non-goal and the activity order is a sensible fallback.

## Open Questions

- Whether to add a `:switch` command-line command as well as the key. It changes no spec requirement here and can be added later.
