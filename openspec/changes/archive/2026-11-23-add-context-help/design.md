# Design

## Context

See `proposal.md`. The help overlay (`App.help`, `app/help.rs`, `ui/help.rs`) already exists and is the precedent: transient UI that is a state of `App`, checked at the top of the `Action::Key` path before the keymap. `Keymap::bindings(mode)` returns every binding, `format_keys` renders keys, and the reducer already has the predicates the popup needs: `sidebar::selected_row` (room, folder or header), `edit::own_under_cursor` (editable own message), `send::unsent_under_cursor` (failed or unconfirmed local row), and `App.focus`.

## Goals / Non-Goals

**Goals:**
- One source of truth: keys come from the live keymap, and the object kinds use the same predicates the actions use, so the popup cannot promise an action that would be a silent no-op.
- A glanceable popup: a few lines, not the whole keymap.
- Reuse the descriptions table and theming approach of the help overlay.

**Non-Goals:**
- Acting from the popup, which-key for partial sequences, content-aware detail (no message text, author or room name is ever shown).

## Decisions

**State, not a mode, not a second overlay.** `App.context: bool` (the popup shows a snapshot-free view: it is recomputed from the app on every draw, so it follows live changes like a message arriving or the cursor object being deleted). Alternative: store the classified object at open time. Rejected: it can go stale (a failed message gets retried while open) and recomputing is cheap.

**Any key closes and is consumed.** Handled at the top of the key path, before the keymap, like the confirmation prompt. The one exception is a key that resolves, through the Normal keymap as a single chord, to `show_context` or `show_help`: it closes the popup and opens the full overlay. Alternative: let the key fall through to normal handling after closing. Rejected: it makes `?` followed by `dd` delete a message the user was only reading about, and "close then act" is easy to do by accident. Costs one extra key press when the user wants to act.

**Object classification is a pure function.** `app/context.rs` gets an enum of kinds (`Room`, `Folder`, `Header`, `OwnMessage`, `UnsentMessage`, `OtherMessage`, `Input`, `Nothing`) and `fn classify(&App) -> Kind`, using the predicates above. The order matters and is fixed: an unsent row is classified before an own server message, mirroring `delete`. `fn lines(&App) -> Vec<ContextLine>` maps each kind to a fixed list of action names (for example `OwnMessage` is `edit_message`, `delete_message`) and resolves each to its bound keys through `Keymap::bindings(Mode::Normal)`, dropping actions with no key and ignoring `cmd:` rows. The static kind-to-actions table is the only hand-written part, and a test asserts every name in it is a valid action name with a help description.

**Key notation.** Multiple keys for one action are joined with a space (`dd`, or `D dd`), as the overlay does, and sorted for stable output.

**Replacing the default of `?`.** The user asked for `?` to give the contextual view. Moving the full list behind a second `?` keeps one key for "tell me about this" and makes the popup the funnel into the overlay. The cost is a breaking change to a default binding, mitigated by `show_help` staying bindable (one `keymap.toml` line restores the old behaviour) and by `:help`. The overlay's own close keys include `show_context`, so `?` `?` `?` toggles out cleanly.

**Rendering.** `ui/context.rs` draws a bordered block at the bottom of the screen, above the status row, over the lower part of the messages and input area, cleared first. Height is content lines plus two borders, capped at a maximum and at the available height; width is the messages and input column. If it does not fit, the object name line is kept and action rows are cut from the bottom, with the "? all keys" row always last when there is room. Text goes through the sanitizer.

**Theming.** Additive element names `context.border`, `context.title`, `context.key`, `context.desc`, with defaults matching the overlay, added to `ELEMENT_PATHS`.

## Risks / Trade-offs

- [`?` default changes meaning for anyone used to the overlay] -> One-line rebind and `:help`; called out as breaking in the proposal. The user is the only current user.
- [The kind-to-actions table drifts from what the actions really accept] -> It reuses the reducer's predicates for classification; a test pins each kind's rows for a fixture app.
- [Sidebar vs messages focus misclassifies when a room is open but the sidebar is focused] -> Classification follows `App.focus` only, as the motions do.
- [Popup hides the input bar] -> Intended and temporary; any key closes it.

## Open Questions

- Whether `Select` on a room should say "open" or "open and focus messages". Wording only; decided at implementation from what `select` does.
