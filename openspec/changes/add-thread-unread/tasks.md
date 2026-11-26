# Tasks

## 1. Measure

- [x] 1.1 Ask the user, on the local 8.9.1 and 9.0.0 servers, to share the non-secret shape of a subscription with `tunread`, `tunreadUser` and `tunreadGroup`, of a subscription update after a thread reply, and whether a REST call (not DDP) marks a thread read; save redacted fixtures and verify they hold no token or real names.

## 2. Data and API

- [x] 2.1 Map the three subscription fields into room state as sets; verify with mapper tests (present, absent, empty).
- [x] 2.2 Carry the fields through live subscription updates with the keep-when-absent rule; verify with realtime tests for a full, a partial and an empty update.
- [x] 2.3 REST exists on both servers (measured in 1.1: `POST /api/v1/chat.readThread` with `{"tmid": id}`, 200; `rid` in the body is rejected as invalid-params): add `Client::mark_thread_read` and verify with a mock-server test that failure is an `ApiError` with no text.
- [x] 2.4 NOT NEEDED, skipped (REST exists on 8.9.1 and 9.0.0; DDP `readThreads` is gone on 9.0.0). Was: if only DDP exists: add a minimal method call to `ddp.rs` and use it for `readThreads`; verify with the fake-server test in `src/realtime/tests.rs` that login still works, the call is sent after login and a failure is ignored.

## 3. Reducer

- [x] 3.1 Add the unread predicate (messages, alert, threads) for the Unread section; verify the existing sidebar tests still pass, then add thread cases.
- [x] 3.2 Start, cancel and restart the thread mark-read timer from the pane events and apply the result; verify with reducer tests for delay, quick close, failure and a live reply while focused.

## 4. Drawing

- [x] 4.1 Draw the `T` badge (plain, mention, `99+`) after the existing badges; verify with sidebar snapshots including the unchanged old cases.
- [x] 4.2 Draw `new` in the marker row; verify with a snapshot.
- [x] 4.3 Add the three theme elements with defaults; verify override and unknown-element tests.
- [x] 4.4 Update the help text; verify the snapshot.

## 5. Wrap-up

- [x] 5.1 Ask the user to try it on their local server (a reply from a second account, reading in the web client) and report the result.

## Workflow follow-up

- Archive after `add-thread-view`, then update the Obsidian Features list per CLAUDE.md.
