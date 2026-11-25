# Design

## Context

See `proposal.md`. The input bar is one `InputBuffer` for the open room plus a `drafts: HashMap<RoomId, InputBuffer>` for the rooms that are not open. Sending is optimistic: a pending message sits in the room's list, a per-room queue sends in order, and the incoming copy reconciles it. `add-thread-view` adds `ThreadPane` and `Focus::Thread`.

## Goals / Non-Goals

**Goals:**
- Reply and start threads with the existing send machinery, not a second one.
- Keep drafts of room and thread separate.

**Non-Goals:**
- Replying from the thread list (open it, then reply).
- Quoting or attachments.

## Decisions

**1. A reply target on the input state.** `InputTarget::Room | Thread { parent_id }`, set when Insert mode starts from the pane or by `reply_in_thread`, and cleared when the pane closes or another room opens. The send path reads the target and puts `tmid` (and `tshow`) on the pending message and on the request. The live text is the target's draft; when the thread is beside the messages it is drawn in a bar of its own under the pane, and the other bar shows its stored draft. On a narrow terminal there is one bar.

**2. Drafts keyed by `DraftKey { room, thread: Option<String> }`.** The map key widens from `RoomId`; `open_room` still moves the room draft in and out of `input`, and switching target swaps the buffer the same way. In memory only, as today. The quit warning counts unsent messages, not drafts, so it counts pending thread replies (kept in the pane or, once it closes, an outbox) and not text drafts.

**3. One queue per room.** A thread reply joins the room's send queue so order with room messages is kept. The pending message is stored in the pane's list (and not in the main flow, unless `tshow`), reconciled by the incoming copy by id as today.

**4. REST send.** `chat.sendMessage` already takes a message object; add `tmid` and `tshow`. Starting a thread is just a reply whose `tmid` is a message that has no replies: the server creates the thread. The pending reply needs the parent's `tcount` bumped locally and corrected by the server value.

**5. Pane actions reuse lookups.** Edit, delete and react find their message by id; extend that lookup to search the pane's list too (already extended for live events in `add-thread-view`). The confirmation prompt and the picker are unchanged.

**6. Keys.** `reply_in_thread` on `gR` (Normal) and `toggle_reply_to_channel` on `<C-b>` (Insert). Insert-mode `<C-b>` is unused today (verify in the default keymap before implementing; pick another key if taken).

## Risks / Trade-offs

- [Widening the draft key touches every draft path] -> Do it first, with the existing draft tests passing before thread drafts are added.
- [A pending reply can be mistaken for a room message in reconciliation] -> Reconcile by the pending id and the thread id together; test the unknown-outcome and retry paths.
- [`tshow` semantics differ across server versions] -> Measure on 8.9.1 and 9.0.0 and record a fixture.
- [Deleting the parent while replying] -> The send fails with a server error and shows as a failed row.

## Open Questions

- None that change the specs.
