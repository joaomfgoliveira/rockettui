# Tasks

## 1. Focus cycle

- [x] 1.1 In `src/app/update.rs`, replace the hard-coded `FocusNext` / `FocusPrev` matches with a step through an ordered list of cyclable panes (today `[Sidebar, Messages]`), with `Input` mapping to Sidebar on next and Messages on prev; verify `cargo test` passes
- [x] 1.2 Update the `focus_cycle` test to assert Sidebar <-> Messages in both directions and the two escapes from `Input`; verify `cargo test focus_cycle` passes
- [x] 1.3 Update the `FocusNext` / `FocusPrev` doc comments in `src/action.rs`; verify they no longer mention the input bar and `cargo clippy` is clean

## Workflow follow-up

- Archive the change with `/openspec-archive-change` once implemented.
