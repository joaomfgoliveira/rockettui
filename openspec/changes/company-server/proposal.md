# Proposal

## Why

The point of rockettui is daily use against the author's company self-hosted Rocket.Chat, but it has only run against local Docker. Whether company policy allows third-party clients and PATs is still unconfirmed. Replaces GSD Phase 6 (DLVR-02).

## What Changes

- Confirm company policy allows a third-party client and Personal Access Tokens (blocker; do this before pointing at the server).
- Create a PAT for the author's own account and configure it via env var or `chmod 600` config.
- Check compatibility with the company server's Rocket.Chat version against what was tested (8.9.1 and 9.0.0).
- Handle company-specific needs found on the way, for example a private CA (no `ca_cert` option exists today) or enterprise sidebar categories.
- Use it day to day: read, send and receive live messages.

## Capabilities

### New Capabilities

### Modified Capabilities

## Impact

Likely configuration and TLS handling if a private CA is needed. Depends on `realtime-ddp`. Capabilities will be filled in once real server differences are known.

## Open Questions

- Is a PAT allowed, and is third-party client use acceptable? (author to confirm)
- Which Rocket.Chat version does the company run?
