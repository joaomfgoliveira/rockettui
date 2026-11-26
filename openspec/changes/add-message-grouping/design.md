# Design

## Context

See `proposal.md`. A message is drawn by `wrap::render_rows(msg, width, ctx)` (two-line layout: a header row `author ... time`, then the body rows indented by 2, then reactions and the thread marker). The drawing and the row model that scrolling uses share one function, `wrap::block_rows(msg, prev, width, ctx)`, which already takes the previous message (for the day divider), and `app::messages::row_starts_of` sums it for the room pane, the thread pane and the results list. The thread pane always draws with the two-line layout. Rocket.Chat's own client decides "sequential" messages with `Message_GroupingPeriod`; its default is believed to be 300 s, which is only a belief until read from a server.

## Goals / Non-Goals

**Goals:**
- One place decides whether a message continues the one before it, used by the drawing and the row model alike, so scrolling cannot disagree with what is drawn.
- Messages stay individually addressable (cursor, edit, delete, react, follow).

**Non-Goals:**
- Reading the server's `Message_GroupingPeriod`: the gap is a local setting, as a normal client has no admin access to it and this needs no extra request.
- Grouping in the compact layout, where every row carries its own time, or in lists whose neighbours are not neighbours in time (search results, search context).
- Collapsing or folding a group.

## Decisions

**1. A pure `continues(msg, prev, ctx)` in `ui/wrap.rs`.** It is true when `ctx.grouping` is set and: both messages are `Normal` and `Sent`; the authors are equal; `tmid` is equal; `0 <= msg.ts - prev.ts <= grouping`; no day divider and no unread divider is drawn above `msg` (reusing `divider_row` and `unread_row`, so "same day" is the divider's own rule in the local zone); and `prev` has no thread marker row (`tmid` none and `tcount` 0). Time is measured from the message directly before, like Rocket.Chat, so a steady conversation stays one group. *Alternative:* measure from the group's first message. Rejected: a long burst would split at an arbitrary point and it is not what the Rocket.Chat UI does.

**2. A grouped message is its normal rows minus the header row.** In the two-line layout the first row is always the header (a deleted message is one row, but never grouped), so the grouped rows are `render_rows(..)[1..]`. A new `wrap::block_lines(msg, prev, width, ctx)` returns the dividers plus those rows, and `block_rows` is its length, so the drawing (`ui/messages.rs`, `ui/thread.rs`) and the row model call one function. *Alternative:* a `grouped` flag threaded through `render_rows` and all its callers and tests. Rejected: about twenty call sites for what one wrapper does.

**3. The mode is in `RowCtx`, not at the call sites.** `RowCtx` gets `grouping: Option<u32>` (seconds). `pane_row_ctx` (the room pane) sets it from the setting when the layout is two-line, `thread_row_ctx` sets it always, and every other context (results, search context) leaves it `None`. Because `row_starts_of` takes the same context, each list keeps its own answer with no extra parameter.

**4. Setting.** `message_grouping_seconds` in `config.toml`, an integer 0 to 3600, default 300; 0 is `None`. It is read like `mark_read_delay_ms` (`int_in`, with the existing error messages for range and type) and listed in `KNOWN_KEYS`. *Alternative:* the Rocket.Chat name `Message_GroupingPeriod`. Rejected: the config keys are lower snake case with units in the name.

**5. Highlight and cursor are untouched.** The cursor highlight covers the rows of the cursor message, taken from the same `block_lines`, so a grouped message highlights only its own body rows and the header stays with the first message of the group. A group's header is highlighted with that first message only.

## Risks / Trade-offs

- [A grouped message shows no time of its own, so a long-lived group hides when each message was sent] -> The setting bounds a pause, and 0 turns grouping off; a later change could show the time on hover or in the context popup.
- [Loading older history can make the first held message continue a newly loaded one and lose its header row] -> History anchors the view on the cursor message's offset from the top, not on row counts, so the view does not jump; a test covers a page loaded above a group.
- [Two messages by one author in different threads shown in the channel] -> `tmid` must match, so they are not grouped.
- [The failure marker lives on the header row] -> Unsent and failed messages never continue a group and nothing continues them.
- [Wrong gap for a given server] -> It is the user's setting, default 300 (believed to match Rocket.Chat's default; check an admin's `Message_GroupingPeriod` when unsure).

## Open Questions

- The thread pane's two-line layout, hour at the right and day dividers only (from `add-thread-list`) are not in the main specs yet; this change assumes them and does not restate them.
