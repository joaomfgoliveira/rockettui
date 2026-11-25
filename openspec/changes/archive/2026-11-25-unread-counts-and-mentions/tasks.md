## 1. Data model

- [x] 1.1 Add `Unread { flagged, count, mentions }` with `is_unread()`, `merge(UnreadUpdate)` and `Default`, in `src/data/mod.rs`; replace `Room.unread: bool`
- [x] 1.2 Adapt every user of `room.unread` (`app::sidebar`, `app::read`, `app::realtime`, `data/fake.rs`, `redaction.rs`, `app/command.rs`, tests) to `is_unread()`

## 2. Wire and mapping

- [x] 2.1 Declare `userMentions`, `groupMentions`, `alert` on `WireSub` and `userMentions`, `groupMentions` on `WireSubDoc` (`src/api/wire.rs`)
- [x] 2.2 Map them in `src/api/map.rs`; flagged from `alert`, mentions summed
- [x] 2.3 Update `src/realtime/events.rs` to produce an `UnreadUpdate` with optional fields; keep dropping an update with none
- [x] 2.4 Change `Action::RtSubscription` and its `Display` (no message content, no token) to carry the update

## 3. Reducer

- [x] 3.1 `app::realtime::on_subscription` merges the update; keep the cursor-follows-room behavior when the room moves sections
- [x] 3.2 A confirmed mark-read resets the room's `Unread`
- [x] 3.3 Tests: partial update keeps the count; `unread: 0` clears mentions; alert with zero count stays unread; read elsewhere leaves Unread

## 4. Rendering and theme

- [x] 4.1 Add `sidebar.mention` to the theme vocabulary (`src/config/theme.rs`), default red bold
- [x] 4.2 Render `(count)` and `@N` in `src/ui/sidebar.rs`, with `99+` and name-first truncation
- [x] 4.3 Snapshot and unit tests: count, count plus mention, flagged without count, large count, themed badge

## 5. Wrap-up

- [x] 5.1 Add REST fixture cases (docker fixture already has `userMentions`) and a map test for the summed mentions
- [x] 5.2 Update the help/context text if it describes the unread marker as a flag
- [x] 5.3 Update the Features file as the project instructions require, once archived
