# Proposal

## Why

The help overlay (`?`) shows every binding at once, which is the right tool for learning the app but a poor one for the daily question "what can I do with this thing right now?". `e`, `dd` and `r` only act on certain messages, `za` only on folders, `<CR>` means different things per pane. Today you have to remember which keys apply where, or scroll the full list.

## What Changes

- Add a context popup: a small panel at the bottom of the screen, opened by `?` in Normal mode, that names the selected object (a room, a folder, a message of yours, someone else's message, a failed or unconfirmed message, the input bar, a section header) and lists only the actions that apply to it, with their keys from the live keymap.
- The popup closes on any key. The key that closes it is consumed and does nothing else.
- Pressing `?` (or the key bound to `show_help`) while the popup is open opens the full help overlay, so the popup is also the way into the complete list. `:help` still opens the overlay directly.
- Add a bindable action `show_context`, bound to `?` in Normal mode by default. **BREAKING (default binding only)**: `?` no longer opens the full overlay directly. `show_help` stays bindable and keeps its name, but has no default key. A user who wants the old behaviour binds `"?" = "show_help"` in `keymap.toml`.
- Like the overlay and the confirmation prompt, the popup is a state of the app, not a mode: the badge keeps showing the real mode and `keymap.toml` gets no new table.

Out of scope: acting on the object from inside the popup (it is a read-only hint), which-key hints for half-typed sequences, per-room or per-message-type detail beyond the object kinds above.

## Capabilities

### New Capabilities
- `context-help`: the context popup: how it opens and closes, how the selected object is classified, which actions each kind lists, and how it is drawn.

### Modified Capabilities
- `keymap`: new bindable action `show_context`, bound to `?` by default; `show_help` no longer has a default key.
- `help`: the full overlay opens from the popup, `:help`, or a user binding of `show_help`, instead of from `?` directly.

## Impact

- `input/`: new `Action::ShowContext`, its name, the default row (`?` moves from `show_help` to `show_context`).
- `app/`: popup state on `App`, the object classification, its handling at the top of the key path (next to the overlay).
- `ui/`: a new popup widget at the bottom of the screen, themed through `theme.toml` (additive `context.*` elements).
- The help descriptions table gains one entry; the action-to-description test keeps it complete.
- No new dependencies, no network, no change to credentials or message handling. The popup shows only built-in text and key names, never message content or room names.
