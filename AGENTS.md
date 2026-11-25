# Agent rules

The full project instructions are in `.claude/CLAUDE.md`. The ones that must never be missed:

Hard rules, no exceptions, even when a task seems to need them:

- Never read, source, print or pass on the user's token or any credential: not `.env*` files, not `dev/state/`, not `config.toml`'s `token`, not environment variables that hold them. If a task needs a live call, give the user the command to run themselves and ask for the non-secret result.
- Never contact the company Rocket.Chat server or anything but the local Docker server and mocks. Live checks happen only against the user's own local server, run by the user.
- Never look for a way around a guard or hook that blocks this. A block is the answer.

