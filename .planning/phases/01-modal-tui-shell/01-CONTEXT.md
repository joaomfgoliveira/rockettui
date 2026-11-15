# Phase 1: Modal TUI Shell - Context

**Gathered:** 2026-11-14
**Status:** Ready for planning

<domain>
## Phase Boundary

A runnable ratatui app with a sidebar (rooms) + messages + input bar layout, driven entirely by fake in-memory data, with Normal / Insert / Command modes and vim-style navigation. Covers NAV-01..NAV-04. No config files (Phase 2), no networking (Phases 3-5).

</domain>

<decisions>
## Implementation Decisions

### Focus & pane model
- **D-01:** Focus moves between panes in Normal mode with `Ctrl-w` + `h`/`j`/`l` (vim window-style). Tab / Ctrl-k belong to the `vim-extras` preset (Phase 2), not Phase 1.
- **D-02:** In the sidebar, `j`/`k` only move the cursor; `Enter` opens the room and moves focus to the messages pane.
- **D-03:** `i` (and `a`/`o`) enters Insert mode on the input bar from any pane; `Esc` returns to Normal mode and to the previously focused pane.
- **D-04:** Messages pane has a line-based cursor on a message: `j`/`k` move it and scroll at edges, `gg`/`G` jump to oldest/newest, `Ctrl-d`/`Ctrl-u` half-page. A room opens positioned at the newest message.

### Key matcher
- **D-05:** Matcher supports count prefixes (`5j`, `3<C-d>`) from the start; counts are generic over actions.
- **D-06:** Multi-key sequences use a trie/prefix matcher with a timeout (~1s, to become configurable later). Pending keys are shown in the status line. Ambiguous prefix waits for the timeout, then fires or cancels.
- **D-07:** The default keymap is built-in data shaped `mode -> key sequence -> Action`, the same structure Phase 2 will load from `keymap.toml`. No hardcoded per-mode `match` blocks.
- **D-08:** Key syntax is Vim notation: `<C-d>`, `<Esc>`, `<CR>`, `gg`. This becomes the `keymap.toml` syntax.

### Layout & chrome
- **D-09:** Bottom status bar shows mode, current room and pending keys; a vim-style `:` command-line row sits below it.
- **D-10:** Two message render layouts are implemented as an enum: compact (`HH:MM author  text`, word wrap with hanging indent) and two-line (author/timestamp header, indented body). Phase 1 uses one built-in default (compact); Phase 2 adds the `config.toml` selector.
- **D-11:** Sidebar has three sections: Unread, Folders, and everything else ordered by most recent activity. Phase 1 is layout only: render the sections from fake data, `j`/`k` across them, fold/unfold of folders, fake unread flags. Real unread tracking and folder management/persistence are deferred.
- **D-12:** Room model carries a kind enum covering channel, private group, discussion, team and DM; all kinds can be placed in folders (membership by room id). Include one encrypted room in fake data for ROOM-03 later.
- **D-13:** Fake data: ~8 rooms, 50+ messages in some rooms, one very long wrapped message, one empty room. It sits behind a replaceable data-source boundary so Phase 3 swaps it for real data. Sidebar width fixed (~28 cols) until config exists.

### Insert & Command behavior
- **D-14:** Input bar is multiline from the start (grows with content, with a sensible max height). Editing: typing, Backspace, arrows/Home/End, `Ctrl-w`, `Ctrl-u`.
- **D-15:** `Enter` in Insert mode appends a local fake message to the current room, clears input and stays in Insert mode. Phase 4 swaps this for an API send.
- **D-16:** Command mode supports only `:q` / `:quit`; unknown commands show an error in the status bar. Dispatch is table-driven so later phases can add commands.
- **D-17:** `:` from Normal opens Command mode; `Esc` cancels any mode back to Normal; `Ctrl-c` acts as `Esc` and does not quit. Only `:q` quits.

### Added during plan-phase (2026-11-14, author decision)
- **D-18:** With focus on the input bar in Normal mode, the NAV-02 motions act on the input buffer, not as no-ops: `h`/`l` move the cursor left/right, `j`/`k` move between lines of the multiline input, `gg`/`G` jump to the first/last line. (Originally planned as no-ops; the author chose input-bar motions as above.) `Ctrl-d`/`Ctrl-u` scroll half a page of the input if it scrolls, otherwise are harmless no-ops.
- **D-19:** Author-accepted narrowing of NAV-02's `hjkl` wording (2026-11-14): in the messages pane `h`/`l` are intentional no-ops (D-04 lists only j/k/gg/G/Ctrl-d/Ctrl-u there), and on sidebar room rows `h`/`l` do nothing; `h`/`l` only fold/unfold folder rows. D-18 applies to the input bar only, not to every pane.

### Claude's Discretion
- Key used to insert a newline in the multiline input (Shift-Enter is unreliable in terminals; likely `Alt-Enter` or `Ctrl-j`).
- Max height of the input bar, exact fold keys for sidebar folders, fake data content, border/glyph styling, event-loop/tick design, terminal setup/teardown and panic-restore handling, module skeleton (following TEA: one `App`, one `Action` enum, one channel, one reducer).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Project definition
- `.planning/PROJECT.md` — core value, constraints (Rust, ratatui+crossterm, tokio, TEA architecture)
- `.planning/REQUIREMENTS.md` — NAV-01..NAV-04 for this phase
- `.planning/ROADMAP.md` — Phase 1 goal and success criteria
- `docs/objective.md` — design notes: layout, keymap presets (`vim` strict vs `vim-extras`), planned modules (`app/`, `ui/`, `input/`)

</canonical_refs>

<code_context>
## Existing Code Insights

Greenfield: no `Cargo.toml` or `src/` exists yet.

### Reusable Assets
- None.

### Established Patterns
- Elm/TEA architecture (one `App` state, one `Action` enum, one channel, one reducer) from PROJECT.md constraints.

### Integration Points
- The keymap data structure (D-07), key notation (D-08) and the fake data-source boundary (D-13) are the seams Phases 2 and 3 plug into.

</code_context>

<specifics>
## Specific Ideas

- Sidebar organization the user wants: Unread, then user-defined Folders, then remaining rooms ordered by most recent. Folders can contain channels, discussions, teams and DMs.
- Both message layouts (compact and two-line) should eventually be user-selectable in settings.

</specifics>

<deferred>
## Deferred Ideas

- Real unread tracking (overlaps deferred MSG-06) and in-app folder management with persistence — future phase/backlog.
- `config.toml` setting to choose message layout — Phase 2.
- Tab pane switch and Ctrl-k room switch — `vim-extras` preset, Phase 2.

</deferred>

---

*Phase: 1-Modal TUI Shell*
*Context gathered: 2026-11-14*
