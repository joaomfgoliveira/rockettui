# Local Rocket.Chat for development

A throwaway Rocket.Chat server on your machine, with seeded data and a Personal Access Token
for the app user. It is the local Docker server that Phase 3 (DLVR-01) reads real rooms and
history from. Nothing here is real data and nothing here is meant to last: `down` deletes the
whole stack and its credentials.

## Prerequisites

- Docker with the compose plugin (`docker compose version`), daemon running.
- Python 3 (standard library only).
- Free host port 3100 on 127.0.0.1 (port 3000 is often taken; change it with `RT_HOST_PORT`).
- About 1 GB of image downloads on the first `up`.

## Commands

Run from the repository root, in this order:

```sh
python3 -I dev/seed.py up       # start Rocket.Chat + MongoDB replica set, wait for /api/info
python3 -I dev/seed.py seed     # create users, rooms, messages and the PAT
python3 -I dev/seed.py check    # prove the PAT works and the secret files are safe
python3 -I dev/seed.py capture  # write scrubbed REST fixtures to src/api/fixtures/docker/
python3 -I dev/seed.py down     # remove containers, volume and the state directory
```

Settings come from the environment: `RT_PROJECT` (default `rockettui-dev`), `RT_RELEASE`
(default `8.9.1`), `RT_HOST_PORT` (default `3100`).

`seed` refuses to run twice on the same stack; to start over run `down` then `up`.

## Seeded dataset

Users: `dev` (the app user, owner of the PAT), `alice`, `bob`, plus the `admin` account.

| Room | Type | What it exercises |
|------|------|-------------------|
| `rt-public` | public channel | 130 messages from dev and alice (paging beyond one page and beyond the server's 100 cap), alice's join system message, a `discussion-created` system message, one message with an attachment titled `report.pdf` |
| `rt-private` | private group | favorited by dev (Favorites folder) |
| DM `alice` | direct | unread messages for dev |
| DM `bob` | direct | closed by dev (`open=false`, must be skipped) |
| `secrets` | private group | `encrypted=true` set in MongoDB (shown as "encrypted, not supported") |
| `rt-discussion` | discussion | child of `rt-public`; its subscription `name` is the room id, the title is `fname` |
| `rt-team` | team main room | `teamMain`; best effort: held by the custom sidebar category `Work` |
| `general` | public channel | created by Rocket.Chat itself |

Notes: the category step needs no license (the preference is written through the API, the
subscription `category` through MongoDB); on this server channels count only mentions as unread,
so `rt-public` shows `unread: 0` with `alert: true`, while the alice DM shows `unread: 2`.

`capture` writes five fixtures under `src/api/fixtures/docker/` (`subscriptions`, `rooms`,
`preferences`, `history_rt_public`, `history_dm_alice`) with every `lastMessage` and `md` key
removed. It aborts without writing if any token or password appears in the text. Message text is
synthetic seed text.

## Run the app against it

```sh
set -a; . dev/state/rockettui-dev/.env.local; set +a; cargo run
```

## Probe the send contract

`dev/probe_send.py` is the Wave 0 probe of Phase 4 (sending messages). It asks the local server how
`chat.sendMessage` behaves, so the client is built on measured answers and not on documentation.
It reads the PAT and the admin password from the state directory in-process.

```sh
python3 -I dev/probe_send.py
RT_PROJECT=rockettui-dev9 python3 -I dev/probe_send.py   # the 9.0.0 stack
```

One line per probe:

| Probe | What it asks |
|-------|--------------|
| P0 | `GET /api/v1/me`: does the own-username call return a username (D-17) |
| P1 | `chat.sendMessage` with a client-chosen `_id`: status, message keys, `_id` echoed, `ts` shape (D-15) |
| P2 | the identical body again: how the server answers a repeated `_id` (A1) |
| P3a / P3b | `chat.getMessage` for a known and for an unknown id |
| P4 | a send into a read-only room |
| P5a / P5b | a send as a muted user (`SKIPPED` when the mute call is unavailable, A7), a send into an archived room |
| P6 | a text of the server's `Message_MaxAllowedSize` plus one character |
| P7 | the replaced endpoint `chat.postMessage` with an `_id` |

Output rule: only statuses, top-level key names, yes/no flags and closed error classes are
printed. No credential, message text, response body, header or server error text ever appears,
and every line is checked against the secret values before it is printed. The last line is
`PROBE OK` only when P1 returned the client's `_id` and every room the run created was deleted.

Cleanup guarantee: the probe creates three `rt-probe-*` rooms (plus one more when the mute probe
leaves the user muted), and deletes exactly those room ids in a `finally` block (`cleanup
deleted=n/m`). It never touches the seeded rooms or a container, and refuses a server that is not
on loopback.

## Probe the realtime contract

`dev/probe_ddp.py` is the spike behind `realtime-ddp`. It speaks DDP over a hand-rolled WebSocket
client (standard library only) to the local server, so the Rust client is built on measured
answers. It reads the PAT and the admin passwords from the state directory in-process, refuses a
server that is not on loopback, and never reads `~/.config/rockettui` or the environment, so it
cannot be pointed at a company server. It takes about two minutes: the last probe idles 90 s to
watch the server's heartbeat.

```sh
python3 -I dev/probe_ddp.py                                # 8.9.1
RT_PROJECT=rockettui-dev9 python3 -I dev/probe_ddp.py      # 9.0.0
```

One line per probe, ending in `PROBE OK`:

| Probe | What it asks |
|-------|--------------|
| D-info | `GET /api/info`: is a version reported |
| D0a / D0b / D0c | the WebSocket upgrade, the server's first frame, the DDP `connect` / `connected` handshake |
| D1a / D1b | `login {resume: <PAT>}`: does the PAT work as a DDP resume token, and is the id the own user id |
| D2a / D2b / D2c | `sub stream-room-messages ["__my_messages__", false]`; alice posts in a channel, a private group and a DM: do all arrive, with which keys, each with the right `rid` |
| D3a / D3b / D3c | `stream-notify-user` `<uid>/subscriptions-changed` and `<uid>/rooms-changed`: what fires per message and whether the subscription document carries `alert` / `unread` (D3c prints whether the keys are present: each message in a private group or DM is followed by a second update with neither key, which the client must not read as "read"; a public channel's updates carry neither, since channels count only mentions) |
| D4a to D4g | `stream-notify-room` `<rid>/deleteMessage` for the public channel, the private group and the DM; per room type, an edit (same `_id`, `editedAt`) and a delete (only `{_id}`, on the room stream), whether each reaches the app's own account, and whether the `subscriptions-changed` update that follows an edit carries `alert` or `unread` (D4f: it carries neither, on 8.9.1 and 9.0.0, so an edit never changes the unread flag). The inputs of `live-edits-deletes` |
| D5a / D5b / D5c | a client `ping` is answered by `pong`; WebSocket-level pings; a 90 s idle watch for the server's own `ping` and the gaps between them |

Output rule: only DDP message names, collection and event names, top-level key names, yes/no
flags and counts are printed; every line is checked against the secret values first. The probe
creates messages only in the seeded rooms as `alice`, and edits and deletes only its own.

Realtime scenarios of the pty harness (the real binary against the local server, 8.9.1 and
9.0.0, checked on 2026-11-23):

```sh
python3 -I scripts/smoke_pty.py target/debug/rockettui docker_realtime                       # 8.9.1
python3 -I scripts/smoke_pty.py target/debug/rockettui docker_reconnect                      # 8.9.1
RT_PROJECT=rockettui-dev9 python3 -I scripts/smoke_pty.py target/debug/rockettui docker_realtime    # 9.0.0
RT_PROJECT=rockettui-dev9 python3 -I scripts/smoke_pty.py target/debug/rockettui docker_reconnect   # 9.0.0
```

- `docker_realtime`: marks `rt-private` and the alice DM read through the REST API, opens
  `rt-private`, then `alice` posts there (the message appears with no command), `alice` posts in
  the DM (it gets its unread marker), and the app sends its own text (the echo shows once).
- `docker_reconnect`: the app talks to the server through a loopback TCP forwarder inside the
  scenario. With `rt-private` open and the DM loaded, the forwarder cuts every connection and
  refuses new ones (the status line shows `Reconnecting`); `alice` posts straight to the server
  (3 messages in `rt-private`, 55 in the DM, more than the default `history_size` of 50); the
  forwarder returns. The open room must catch up without duplicates, and the stale DM must refetch
  when opened and show its newest message once. A forwarder replaces stopping the container
  because posts cannot be made while the server is down.
- The pty harness's mock server (`start_mock`) has no WebSocket: it answers the realtime upgrade
  (`GET /websocket`) with 404 and counts it in `ws_attempts`, so the app shows `Reconnecting` there.
- Both end with a clean `:q` and check that no token or secret is in the terminal output and that
  no message text appears after the app leaves the alternate screen (on the TUI screen it is shown
  by design).

| Server | Result | Notes |
|--------|--------|-------|
| 8.9.1 | pass | `SMOKE OK` for both; the previous binary fails `docker_realtime` (control) |
| 9.0.0 | pass | `SMOKE OK` for both, and `docker_read` / `docker_send` still pass; no difference from 8.9.1, no client change needed |

Live edits and deletes (`live-edits-deletes`), checked on 2026-11-23 on both servers:

```sh
python3 -I scripts/smoke_pty.py target/debug/rockettui docker_edits                       # 8.9.1
RT_PROJECT=rockettui-dev9 python3 -I scripts/smoke_pty.py target/debug/rockettui docker_edits    # 9.0.0
```

- `docker_edits`: in `rt-private`, `alice` posts two messages; she edits the first (the new text and
  the `(edited)` marker show in the same screen row and the old text is gone) and deletes the second
  (its row becomes `alice  message deleted`, still in the same row, with no text). In the alice DM,
  which the app has not opened, she posts and deletes a message; opening the DM shows no trace and no
  deleted line, since deleted lines exist only for messages the app held.
- `docker_reconnect` now also covers edits and deletes during an outage: with two held messages in
  `rt-private`, `alice` edits one and deletes the other while the forwarder has cut the app off, then
  posts the three catch-up messages (a second later, so both held messages are older than the
  newest second of the refetched page, which is the only range the reconcile judges). After the
  reconnect the open room shows the edit with its marker and the deleted line. The reconnect can
  take up to 30 s after a longer outage (the backoff ceiling), so the scenario waits up to 60 s.
- `docker_deleted_status`: the server setting `Message_ShowDeletedStatus` (off by default; the
  scenario switches it on as `admin` and restores it in a `finally`) makes a delete leave a message of
  type `rm` (empty text, `editedAt` set, the original author in `u`) instead of removing it. A live
  delete in `rt-private` must become the deleted line in its row, and a deleted message in the alice DM
  loaded from history must show as one too: never `system message`, never an edited marker, never the
  text. Checked on 8.9.1 and 9.0.0; with the `rm` mapping disabled it fails (control).
- Both end with `finish_clean`: no token or secret in the terminal output, and none of the texts
  (including the deleted ones) after the app leaves the alternate screen.
- Not covered by a scenario: a message deleted while offline in the refetched page's oldest or
  newest second stays visible until a reload (see the change's design, Risks).

| Server | Result | Notes |
|--------|--------|-------|
| 8.9.1 | pass | `docker_edits` and `docker_reconnect` give `SMOKE OK`; the whole harness (21 scenarios) passes |
| 9.0.0 | pass | `docker_read`, `docker_send`, `docker_realtime`, `docker_reconnect` and `docker_edits` give `SMOKE OK`; no difference from 8.9.1, no client change needed |

## Probe the edit and delete contract

`dev/probe_edit.py` is the Wave 0 probe of `edit-delete-own-messages`. It asks the local server how
`chat.update` and `chat.delete` behave for the app user, so the client's error mapping is built on
measured answers. It reads the PAT and the admin password from the state directory in-process and
refuses a server that is not on loopback. It takes about 90 s: the time-limit probe waits until its
message is older than a minute.

```sh
python3 -I dev/probe_edit.py                                # 8.9.1
RT_PROJECT=rockettui-dev9 python3 -I dev/probe_edit.py      # 9.0.0
```

| Probe | What it asks |
|-------|--------------|
| E1 / E1s | an own edit: status, answer keys, `editedAt` present; and whether the edit comes back on the `stream-room-messages` stream |
| E2 / E2s | an own delete: status, answer keys; and whether it comes back on the `stream-notify-room` delete stream |
| E3a / E3b | an edit and a delete of an unknown message id |
| E4a to E4c | an edit and a delete of another user's message (the room owner's), and whether it is untouched |
| E5a to E5c | `Message_AllowEditing` off, then `Message_AllowDeleting` off, on a fresh own message |
| E6a / E6b | `Message_AllowEditing_BlockEditInMinutes` and `Message_AllowDeleting_BlockDeleteInMinutes` set to 1 on a message older than a minute |

Output rule as for the other probes: only statuses, key names, yes/no flags, the server's
`errorType` code and closed error classes. The four settings are read first and restored in a
`finally` (`settings restored=4/4`); the one room it creates is deleted (`cleanup deleted=1/1`).
The last line is `PROBE OK` only when the own edit and delete answered 200, the settings are back
and the room is gone. The measured answers are recorded in the change's `design.md` (Decision 7).

Editing and deleting your own messages (`edit-delete-own-messages`), checked on 2026-11-23 on both
servers:

```sh
python3 -I scripts/smoke_pty.py target/debug/rockettui docker_edit_own                       # 8.9.1
python3 -I scripts/smoke_pty.py target/debug/rockettui docker_delete_own                     # 8.9.1
RT_PROJECT=rockettui-dev9 python3 -I scripts/smoke_pty.py target/debug/rockettui docker_edit_own      # 9.0.0
RT_PROJECT=rockettui-dev9 python3 -I scripts/smoke_pty.py target/debug/rockettui docker_delete_own    # 9.0.0
```

- `docker_edit_own`: `dev` posts a message in `rt-private`. `e` puts its text in the input bar in
  Insert mode and the input title says that Enter saves and Esc cancels; `Esc` restores a half-typed
  draft; a changed text saves, and the row shows the new text and `(edited)` in the same screen row,
  once, with the server holding the new text and the draft back in the input. In a second room where
  `dev` is a plain member, `Message_AllowEditing` is switched off as `admin` (restored in a
  `finally`) and a save shows `edit not saved: ...`, keeps the typed text in the input and leaves the
  server text alone.
- `docker_delete_own`: `dd` on an own message opens the prompt in the status bar; `j` closes it and
  deletes nothing; `dd` then `y` turns the row into `dev  message deleted` in the same row and the
  server no longer holds the message. `e` and `dd` on a message written by `alice` change nothing
  (the mode, the screen and the server text are the same afterwards). In the plain-member room,
  `Message_AllowDeleting` is switched off, and a confirmed delete shows `message not deleted: ...`
  and the message stays.
- Version difference: none between 8.9.1 and 9.0.0. The refusals need a room where the app user is
  not an owner: `dev` owns `rt-private` and `rt-public`, and an owner's delete is accepted with
  `Message_AllowDeleting` off, so both scenarios create a throwaway public channel as `admin` with
  `dev` as a plain member (named `rt-eo-*` and `rt-do-*`) and delete it in a `finally`.
- Both end with `finish_clean`: no token or secret in the terminal output, and none of the texts
  after the app leaves the alternate screen.

## Second stack: Rocket.Chat 9.0.0

A fully independent stack (own project, state directory and volume) for the compatibility pass:

```sh
RT_PROJECT=rockettui-dev9 RT_RELEASE=9.0.0 RT_HOST_PORT=3101 python3 -I dev/seed.py up
RT_PROJECT=rockettui-dev9 RT_RELEASE=9.0.0 RT_HOST_PORT=3101 python3 -I dev/seed.py seed
```

## Compatibility

Checked on 2026-11-17 with the `docker_read` pty scenario, which starts the real binary with the
PAT from the stack's state file and walks the sidebar and history by key presses:

```sh
python3 -I scripts/smoke_pty.py target/debug/rockettui docker_read                       # 8.9.1
RT_PROJECT=rockettui-dev9 python3 -I scripts/smoke_pty.py target/debug/rockettui docker_read   # 9.0.0
```

| Server | Result | Notes |
|--------|--------|-------|
| 8.9.1 (`rockettui-dev`, port 3100) | pass | rooms, Favorites and Work folders, paging to `Beginning of history`, join and attachment lines, `encrypted, not supported`, clean `:q`, token never in the pty output |
| 9.0.0 (`rockettui-dev9`, port 3101) | pass | same scenario, no client change needed; the stack was seeded with the same script and removed afterwards |

Sending (Phase 4), checked on 2026-11-23 with the `docker_send` pty scenario and the send probe.
`docker_send` starts the real binary with the PAT from the stack's state file, opens `rt-private`
(never `rt-public`, so `docker_read` stays valid however often it runs), sends a fresh text from
Insert mode, waits for the confirmed row (server time and the `dev` author), quits, starts the
binary again and finds the same text in `rt-private`'s history, so the message persisted:

```sh
python3 -I dev/probe_send.py                                                              # 8.9.1
python3 -I scripts/smoke_pty.py target/debug/rockettui docker_send                        # 8.9.1
RT_PROJECT=rockettui-dev9 python3 -I dev/probe_send.py                                    # 9.0.0
RT_PROJECT=rockettui-dev9 python3 -I scripts/smoke_pty.py target/debug/rockettui docker_send   # 9.0.0
```

| Server | Result | Notes |
|--------|--------|-------|
| 8.9.1 | pass | `PROBE OK`, `SMOKE OK: docker_send`; the `_id`, `rid`, `msg` body is accepted and the text is in the history after a restart |
| 9.0.0 | pass | `PROBE OK`, `SMOKE OK: docker_send`; the same body is accepted, so no client change was needed. One difference: for the replaced `chat.postMessage` (P7) the error type is `error-invalid-params` here and `invalid-params` on 8.9.1; nothing in the client reads it |

Known differences and limits:

- `/api/info` reports `version` as major.minor (`8.9`, `9.0`), not the patch release.
- Channels count only mentions as unread on both servers; a DM counts every message.
- The seeded dataset and the captured fixtures come from 8.9.1 only; the 9.0.0 pass is an
  end-to-end read check, not a second fixture set.
- Both stacks use the Categories-era sidebar. Servers older than 8.x, and the company server
  (version and license unknown, RESEARCH Open Question 4), are untested until Phase 6.
- Long lines wrap in the 50-column message pane, so the attachment tag `[attachment: report.pdf]`
  can be split over two rows; the scenario checks both halves.

## Room search (`chat.search`)

`dev/probe_search.py` is the spike behind `search-room`. Read-only, same output rule as the other
probes. Checked on 2026-11-24 on 8.9.1 and 9.0.0 with identical results:

```sh
python3 -I dev/probe_search.py                                # 8.9.1
RT_PROJECT=rockettui-dev9 python3 -I dev/probe_search.py      # 9.0.0
```

- `GET chat.search?roomId&searchText&count&offset` answers `{messages, success}` with the history
  message shape, newest first. There is no total; the end is a page shorter than `count`.
- Default page 50, maximum 100 (`count=500` returns 100). The client asks for 50.
- Case-insensitive. An empty `searchText` and an unknown room id are both HTTP 400.
- `from:`, `has:`, `is:`, `on:` and `/regex/` queries are accepted (status 200).

End to end: `python3 -I scripts/smoke_pty.py target/debug/rockettui docker_search` searches the
seeded `rt-public` (130 messages, three pages), closes the list, and checks the no-results and
encrypted-room refusals. Passed on 8.9.1; on 9.0.0 only the probe above was run.

## Security

- Everything binds to 127.0.0.1 only; nothing is reachable from the network.
- The generated passwords and the PAT live in `dev/state/<project>/`, a 0700 directory holding
  0600 files, ignored by git through `/dev/state/`. They are never printed and never passed on a
  command line. `check` verifies the ignore rule and the modes.
- On this throwaway server two-factor authentication and the REST rate limiter are switched off.
- The token travels in clear over plain http, which is only acceptable on loopback.
- This setup must never be pointed at the company server: the company policy on third-party
  clients and PATs is unconfirmed (CLAUDE.md Policy, Phase 6).
