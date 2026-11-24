## 1. Dividers

- [x] 1.1 Remove the date-on-row work: `RowCtx.now`, `format_msg_ts`, and the clock parameter of `time_column`; keep `timefmt::day_of`
- [x] 1.2 Add `wrap::divider_row` and `wrap::block_rows`; make `messages::row_starts_of` sum block heights
- [x] 1.3 Draw the divider above each message in `ui/messages.rs` and `ui/results.rs`, outside the cursor and selection highlight

## 2. Tests and verification

- [x] 2.1 Unit tests: first message, day change, same day, clipping and unrepresentable times for `divider_row`; results list shows dividers and rows with only the time; the divider is not highlighted; the row model matches the renderer
- [x] 2.2 Update the snapshots and the position-dependent tests for the extra divider row
- [x] 2.3 Run `cargo fmt`, `cargo clippy` and `cargo test`
- [x] 2.4 Run the `docker_search` smoke scenario against the local Rocket.Chat
