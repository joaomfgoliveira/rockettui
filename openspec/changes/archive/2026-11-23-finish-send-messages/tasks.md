# Tasks

No design.md: verification only, no new behavior.

## 1. Live-server proof

- [x] 1.1 Confirm both stacks are up and seeded (`python3 -I dev/seed.py check`, and the 9.0.0 stack with `RT_PROJECT=rockettui-dev9 RT_RELEASE=9.0.0 RT_HOST_PORT=3101`)
- [x] 1.2 Run `dev/probe_send.py` on 8.9.1 and on 9.0.0
- [x] 1.3 Run `smoke_pty.py ... docker_send` and `docker_read` on both releases (SMOKE OK expected; scenario sends only to `rt-private`, never prints the token)
- [x] 1.4 Add the docker_send and probe commands, with the date checked, to `dev/README.md` Compatibility section

## 2. Regression gate

- [x] 2.1 Run the mock-based pty scenarios: quit, panic, insert, send, send_retry, send_unconfirmed, send_author, quit_undelivered, config_layout, config_keymap, config_error, connect_read, missing_creds, config_creds, perm_warning, unauthorized
- [x] 2.2 `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check` passes

## 3. Human UAT (official client)

- [x] 3.1 Send `uat hello from rockettui` to `rt-private` from rockettui; row shows at once, then time and author, input empty, still in Insert mode (done by user against a real server and colleagues, not the local `rt-private` room)
- [x] 3.2 In the web client at http://localhost:3100 as `dev`, the message appears in `rt-private` exactly once
- [x] 3.3 Optional: send a text over the server size limit; row shows failed, `r` fails the same way, `:q` warns and a second `:q` quits (confirmed by user)
- [x] 3.4 Record any difference found, with the step number, then archive this change
