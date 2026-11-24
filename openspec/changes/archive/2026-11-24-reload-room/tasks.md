## 1. Command

- [x] 1.1 Add `history::reload` choosing the path by state (refuse for no room, encrypted and Loading; detached: `jump::reset`, clear the find state, `on_open`; Failed or none: drop the state, `on_open`; Loaded: `refetch_newest`) and set the status line
- [x] 1.2 Add the `:reload` row to `COMMANDS` in `app/command.rs` (it appears in the help overlay)

## 2. Tests and verification

- [x] 2.1 Unit tests: no room, encrypted room, Loading refused, Failed retried, Loaded refetches and keeps cursor and unsent messages, detached returns to newest and clears the find state, status text, the command shows in help, an unknown command is unaffected
- [x] 2.2 Run `cargo fmt`, `cargo clippy`, `cargo test`, and the `docker_search` smoke scenario extended with `:reload` after the jump
