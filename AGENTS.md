# Agent rules

The full project instructions are in `.claude/CLAUDE.md`. The ones that must never be missed:

Hard rules, no exceptions, even when a task seems to need them:

- Never read, source, print or pass on the user's token or any credential: not `.env*` files, not `dev/state/`, not `config.toml`'s `token`, not environment variables that hold them. If a task needs a live call, give the user the command to run themselves and ask for the non-secret result.
- Never contact the company Rocket.Chat server or anything but the local Docker server and mocks. Live checks happen only against the user's own local server, run by the user.
- Never look for a way around a guard or hook that blocks this. A block is the answer.


## Features list

The features list lives in the user's Obsidian vault: `/home/joliveira/Documents/Obsidian Vault/Personal/Projects/Rockettui/Features.md`. On every new feature (when its OpenSpec change is archived, or a direct edit adds user-visible behavior), update that file: tick the feature's checkbox or add a line under the right section, and refresh the `Progress` count and the `Last updated` date. Keep the same style (plain language, no jargon).
