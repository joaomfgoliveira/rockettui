---
phase: 03-connect-read
review: 03-REVIEW.md
---

# Review disposition

Written by the execute-phase orchestrator. Every finding defaults to `open`; nothing was fixed in this run.

| ID | Severity | Summary | Disposition |
|----|----------|---------|-------------|
| WR-01 | warning | dev/seed.py and smoke_pty.py trust unvalidated RT_PROJECT / RT_HOST_PORT (path traversal in `down`, credentials to a non-loopback host) | open |
| WR-02 | warning | `seed.py down` deletes credentials even when `docker compose down -v` failed | open |
| WR-03 | warning | message composed while connected is appended locally as "me" and looks sent but is never sent (until Phase 4) | open |
| WR-04 | warning | lenient wire types never check `success`; a wrong 200 response becomes Ready with no rooms / permanently empty history | open |
| WR-05 | warning | `:reconnect` has no in-flight guard; a stale room load can overwrite a newer one | open |
| IN-01 | info | older-page paging can skip messages sharing the boundary timestamp | open |
| IN-02 | info | best-effort preferences call still blocks the room load up to 15 s | open |
| IN-03 | info | `alert: true` with `unread: 0` rooms not shown as unread | open |
| IN-04 | info | http-to-remote security warning can be clipped on the status bar | open |
| IN-05 | info | test-only fake data compiled into non-test builds | open |
| IN-06 | info | housekeeping: unused reqwest `json` feature, `scripts/__pycache__/` not ignored, seed.py KeyError/ValueError handling | open |
