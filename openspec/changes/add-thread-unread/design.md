# Design

## Context

See `proposal.md`. The server keeps per-thread unread state on the user's subscription (`tunread`, a list of thread ids; `tunreadUser` and `tunreadGroup`, the subset with mentions). Room unread counts come from the subscription and are shown as the server reports them (`rooms` and `realtime-messages` specs). Room mark-read is REST `subscriptions.read` after a delay. The DDP client (`src/realtime/ddp.rs`) only sends `login` and receives subscription events; it cannot call other methods.

## Goals / Non-Goals

**Goals:**
- Show thread unread on the parent and the room, keep it in step, mark read when read.

**Non-Goals:**
- Desktop notifications.
- Counting unread replies per thread (the server gives the set of unread threads, not the numbers).
- Changing the room's own unread count.

## Decisions

**1. Store the sets, not counts.** `Room` keeps `unread_threads: HashSet<String>` and `unread_thread_mentions: HashSet<String>`. The badge number is the set size; a parent's `new` is set membership by its id. The server's lists are replaced whole when present in an update and kept when absent (same rule as the existing live unread counts). *Alternative:* count replies locally. Rejected: the app does not see every reply, and the project rule is that counts are the server's.

**2. Where the data comes from.** The room list load (`subscriptions.get`) and live subscription updates already carry the subscription document; extend `wire` and `map` for the three fields, and the realtime event mapper for partial updates. Verify on the local 8.9.1 and 9.0.0 that the fields exist and that a reply in a thread produces a subscription update.

**3. Marking a thread read: REST first, DDP if needed.** The official client marks a thread read with the DDP method `readThreads` (thread id). First measure whether the local servers expose a REST equivalent usable with a PAT. If one exists, use it, like room mark-read. If not, extend `ddp.rs` with a small generic method call (`call(method, params)` with an id and a result) after the login. That extension is the biggest piece of this change and the only new realtime capability; keep it narrow (fire and forget, result ignored beyond logging the failure class). *Alternative:* skip marking read and let the user read in another client. Rejected: the badge would never clear.

**4. Reuse the room mark-read timer.** The pane open event starts a cancelable timer like the room's; closing or replacing cancels it; a live reply to a focused open thread restarts the read. Best effort, silent on failure.

**5. Sidebar.** The badge is drawn after the mention badge. Unread section membership: a room is unread when it has unread messages, an alert flag, or a non-empty `unread_threads`. Style elements are named in the spec and are user-facing vocabulary, so they are fixed here.

## Risks / Trade-offs

- [The server may not send thread fields on subscription updates for every reply] -> Measure first; if absent, refresh the fields on the next room-list load and narrow the live scenario.
- [A DDP method call is new surface in a client that is deliberately minimal] -> Keep it to one method, covered by the existing fake-server tests in `src/realtime/tests.rs`.
- [Unread-section rule widens what counts as unread] -> One predicate, tested for the existing cases first.
- [Thread ids in the set may refer to deleted threads] -> The set is replaced from the server on every update that carries it, so stale ids drop out.

## Open Questions

- Whether `readThreads` has a REST twin on both server versions. Answered by task 1.1 before the realtime work starts; it changes only which of two tasks (2.3 or 2.4) is done, not the specs.
