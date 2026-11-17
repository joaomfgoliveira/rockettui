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
python3 -I dev/seed.py down     # remove containers, volume and the state directory
```

Settings come from the environment: `RT_PROJECT` (default `rockettui-dev`), `RT_RELEASE`
(default `8.9.1`), `RT_HOST_PORT` (default `3100`).

`seed` refuses to run twice on the same stack; to start over run `down` then `up`.

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

## Security

- Everything binds to 127.0.0.1 only; nothing is reachable from the network.
- The generated passwords and the PAT live in `dev/state/<project>/`, a 0700 directory holding
  0600 files, ignored by git through `/dev/state/`. They are never printed and never passed on a
  command line. `check` verifies the ignore rule and the modes.
- On this throwaway server two-factor authentication and the REST rate limiter are switched off.
- The token travels in clear over plain http, which is only acceptable on loopback.
- This setup must never be pointed at the company server: the company policy on third-party
  clients and PATs is unconfirmed (CLAUDE.md Policy, Phase 6).
