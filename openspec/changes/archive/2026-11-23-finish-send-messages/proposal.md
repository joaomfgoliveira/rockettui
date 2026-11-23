# Proposal

## Why

Phase 4 (send messages) is code-complete except its last plan: proof against the real local Rocket.Chat on 8.9.1 and 9.0.0 and a human check in the official client. Until then MSG-02 is not confirmed end to end. This change replaces the GSD plan 04-08.

## What Changes

- Prove sending against both local Docker servers with the `docker_send` pty scenario (its first commit, 663cb29, already landed).
- Document the new commands in `dev/README.md` (Compatibility section).
- Run the full regression gate.
- Human UAT: the message sent from rockettui appears once in the official web client.
- No behavior change: the `message-sending` spec already describes what is built.

## Capabilities

### New Capabilities

### Modified Capabilities

## Impact

`scripts/smoke_pty.py`, `dev/README.md`. Sends test messages only to the seeded throwaway `rt-private` room on loopback servers. The company server stays out of scope.
