# Tasks

## 1. Measure the server (run by the user, local Docker only)

- [x] 1.1 Ask the user to post a parent with 3 replies (one with "also send to channel") on the local 8.9.1 and 9.0.0 servers and share the non-secret JSON shape of `chat.getThreadMessages` and of a stream message with `tmid`; save it as redacted fixtures under `src/api/fixtures` and verify the fixtures contain no token or real names.

## 2. Data and mapping

- [x] 2.1 Add `tmid`, `tshow`, `tcount` and `tlm` to `Message` and map them in `api/map.rs` for every `Message` constructor; verify with mapper unit tests over the fixtures (parent, reply, channel-visible reply, plain message).
- [x] 2.2 Add `in_main_flow` and make the main flow draw and count only those messages; verify with a test that hidden replies do not move the cursor, the unread divider or the follow rules.
- [x] 2.3 Add `Client::load_thread` for `chat.getThreadMessages` with offset paging and verify against a mock server test, including a rejected request mapped to `ApiError` with no message text in its Debug/Display.

## 3. Reducer and realtime

- [x] 3.1 Add `ThreadPane` to `App`, the `OpenThread`, `CloseThread`, `ThreadLoaded` and `ThreadLoadFailed` actions and `Effect::LoadThread`; verify with reducer tests for open, replace, close, room change and failure.
- [x] 3.2 Route live replies per the modified `realtime-messages` requirement (store, append to the open pane, bump the parent); verify with tests for open thread, closed thread, duplicate and channel-visible reply.
- [x] 3.3 Apply live edits, deletes and reactions to messages held in the pane; verify with tests.

## 4. Input and focus

- [x] 4.1 Add `open_thread` (`gt`) and `close_thread` to the action vocabulary, the default keymap and `Esc` handling in the pane; verify that `gg`, `gr` still work and an unknown spelling is a config error.
- [x] 4.2 Add `Focus::Thread`, make `CYCLE` depend on whether the pane is open, and wire `Ctrl-w l/h`; verify with the existing focus tests extended.
- [x] 4.3 Add the thread action to the context popup; verify with a popup snapshot test for a parent, a plain message and the pane.

## 5. Drawing

- [x] 5.1 Draw the marker row and the `[thread]` tag at the start of a channel-visible reply in both layouts; verify with snapshot tests (singular and plural, wrapped narrow pane, cursor highlight).
- [x] 5.2 Draw the thread pane, the split layout and the narrow takeover; verify with snapshots at 160 and 80 columns and a test that no size panics.
- [x] 5.3 Add `messages.thread`, `messages.thread_reply_hint` and `thread_pane.border` to the theme with defaults; verify with theme tests for the override and the unknown element.

## 6. Docs and wrap-up

- [x] 6.1 Update the help overlay text and the config docs for the new keys and theme elements; verify the help snapshot.
- [ ] 6.2 Ask the user to try the feature against their local server and report (not the company server); record the outcome.

## Workflow follow-up

- Archive the change, then tick the thread view line in the Obsidian Features list per CLAUDE.md.
