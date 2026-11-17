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

Known differences and limits:

- `/api/info` reports `version` as major.minor (`8.9`, `9.0`), not the patch release.
- Channels count only mentions as unread on both servers; a DM counts every message.
- The seeded dataset and the captured fixtures come from 8.9.1 only; the 9.0.0 pass is an
  end-to-end read check, not a second fixture set.
- Both stacks use the Categories-era sidebar. Servers older than 8.x, and the company server
  (version and license unknown, RESEARCH Open Question 4), are untested until Phase 6.
- Long lines wrap in the 50-column message pane, so the attachment tag `[attachment: report.pdf]`
  can be split over two rows; the scenario checks both halves.

## Security

- Everything binds to 127.0.0.1 only; nothing is reachable from the network.
- The generated passwords and the PAT live in `dev/state/<project>/`, a 0700 directory holding
  0600 files, ignored by git through `/dev/state/`. They are never printed and never passed on a
  command line. `check` verifies the ignore rule and the modes.
- On this throwaway server two-factor authentication and the REST rate limiter are switched off.
- The token travels in clear over plain http, which is only acceptable on loopback.
- This setup must never be pointed at the company server: the company policy on third-party
  clients and PATs is unconfirmed (CLAUDE.md Policy, Phase 6).
