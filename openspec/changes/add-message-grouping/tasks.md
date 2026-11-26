# Tasks

## 1. Setting

- [ ] 1.1 Add `message_grouping_seconds` (0-3600, default 300) to `Settings`, `KNOWN_KEYS` and `parse`; verify with settings tests for the default, a valid value, `0`, an out-of-range value and a wrong type (each one error naming the key).

## 2. Grouping rule and row model

- [ ] 2.1 Add `RowCtx::grouping`, set by `pane_row_ctx` (two-line layout only) and `thread_row_ctx`, `None` everywhere else; verify with a test that the results and context contexts have none.
- [ ] 2.2 Add `wrap::continues` and `wrap::block_lines`, make `block_rows` its length; verify with unit tests for each condition: same author within the gap, a gap, a chained group, another author, a day change, the unread divider, system, deleted and unsent messages, different `tmid`, a thread marker row on the previous message, and `0` off.
- [ ] 2.3 Draw through `block_lines` in the room pane and the thread pane, so the cursor rows come from it; verify with snapshots of a room and a thread pane with a burst, a gap and a second author, and with a test that `row_starts` agrees with the rows drawn for every message.

## 3. Behaviour around it

- [ ] 3.1 Verify the cursor moves message by message over a group and the highlight covers only its rows, and that edit, delete and react act on the message under the cursor in a group (reducer tests).
- [ ] 3.2 Verify loading older history above a group keeps the cursor message where it was (history test), and that a live message continues the newest group.
- [ ] 3.3 Verify the compact layout, the search results list and the search context are unchanged (existing snapshots stay as they are).

## 4. Wrap-up

- [ ] 4.1 Ask the user to try it on their local server, with a burst of messages, and report the result.

## Workflow follow-up

- Archive after `add-thread-list`, then update the Obsidian Features list per CLAUDE.md.
