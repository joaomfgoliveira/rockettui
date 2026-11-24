# Design

## Context

See `proposal.md`. `ui/wrap.rs` draws one message as rows from a `RowCtx` built by `App::row_ctx`; the live pane and the results list both use it, and the reducer's row heights (`messages::row_starts_of`) use the same functions, so drawing and scrolling agree.

## Goals / Non-Goals

**Goals:**
- Same divider rule in both panes, decided in one place, with no change to how messages are selected and scrolled.

**Non-Goals:**
- No date on message rows, no relative dates, no new config key.

## Decisions

**The divider belongs to the message below it.** A message's block is its divider row (0 or 1) plus its message rows; `block_rows` gives the height and `row_starts_of` sums it. `starts[i]..starts[i+1]` still means "message i", so `scroll_into_view`, `step` and the cursor code need no change, and a message scrolled to the top brings its divider with it. Alternative: dividers as items of their own in the list. Rejected: every motion, the cursor index and every "message i" lookup would need to skip them.

**Pure function of two neighbours.** `divider_row(msg, prev, width, ctx)` is `Some` when `prev` is `None` or when the two messages' days in `ctx.zone` differ; it is `None` when a timestamp cannot be represented. No clock and no "today": the rule is the same for any list, including the newest-first results list.

**Drawing.** The renderers push the divider line, then the message rows; the cursor or selection range starts after the divider, so the highlight does not cover it. The divider is `── YYYY-MM-DD ` filled with `─` to the pane width, in `theme.messages_notice`, clipped on narrow panes.

**Revert the date-on-row work.** `RowCtx.now`, `format_msg_ts` and the clock dependency of `time_column` are removed; `timefmt::day_of` stays as the one place a day is computed.

## Risks / Trade-offs

- [One extra row per day] → Accepted: it is the point. A short window shows one more row less of history.
- [Pending messages carry the clock time as their `ts`] → They fall on today's day like any message; a test clock at 0 shows a 1970 divider in tests only.
- [Tiny panes] → The divider is clipped to the width; the body width rules are untouched.
