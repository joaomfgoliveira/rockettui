---
status: testing
phase: 03-connect-read
source: [03-VERIFICATION.md]
started: 2026-11-17T11:35:16Z
updated: 2026-11-17T11:35:16Z
---

## Current Test

number: 1
name: A6: judge the sidebar folder layout on the real servers (Unread, Folders with Favorites and Work, Recent)
expected: |
  The author finds the grouping and ordering usable for daily use
awaiting: user response

## Tests

### 1. A6: judge the sidebar folder layout on the real servers (Unread, Folders with Favorites and Work, Recent)
expected: The author finds the grouping and ordering usable for daily use
result: [pending]

### 2. A10 / D-15: judge the wording of system lines ('* joined the room'), attachment tags ('[attachment: name]') and '[encrypted message]'
expected: Wording is acceptable and the dim system lines are readable
result: [pending]

### 3. D6: with the real binary in a real 80-column terminal, check the status bar when a failure text and a config warning are shown together (and the http-to-remote warning)
expected: The chmod 600 warning and the failure reason are not clipped to the point of being unreadable
result: [pending]

### 4. End-of-phase UAT (plan 03-07 Task 2 human-check): run the real binary against the live 8.9.1 stack and walk the five ROADMAP criteria
expected: All five criteria hold when driven by a human in a real terminal: env-var and config.toml (chmod 600) auth; sidebar lists channels, private groups and DMs and Enter opens one; k/gg pages back to 'Beginning of history' in rt-public; 'secrets' shows 'encrypted, not supported'; chmod 644 on config.toml warns in the status bar and on exit; the token never shows in scrollback after :q
result: [pending]

## Summary

total: 4
passed: 0
issues: 0
pending: 4
skipped: 0
blocked: 0

## Gaps
