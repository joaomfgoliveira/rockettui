# Phase 3: Connect & Read - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-11-16
**Phase:** 03-Connect & Read
**Areas discussed:** Credentials & server config, Connection & failure UX, Room list mapping, History loading & scroll

---

## Credentials & server config

| Question | Selected | Alternatives |
|----------|----------|--------------|
| Credential source / precedence | Env vars override config | Config overrides env; env only |
| Server URL | `server_url` in config.toml + env override | config only; env only |
| Missing credentials | Existing config error screen | Fake data + warning; stderr exit |
| chmod 600 check | Warn only, start anyway | Refuse to start; warn on any readable config |

## Connection & failure UX

| Question | Selected | Alternatives |
|----------|----------|--------------|
| Connect | TUI immediately, background load | Authenticate before TUI |
| Runtime failures | Status-bar error + `:reconnect` command | Status-bar only; full-screen error |
| Logging (AUTH-03) | No log file; redacting Debug + test | tracing file logging |
| TLS | reqwest + rustls defaults | ca_cert / insecure options |

## Room list mapping

| Question | Selected | Alternatives |
|----------|----------|--------------|
| Sections | User challenged "RC has no folders": newer RC has Categories. Resolved: research API, fall back to Favorites | Favorites only; categories required |
| Rooms shown / order | Joined rooms, by last activity | Alphabetical; include unjoined public |
| Unread | subscription `unread > 0` boolean | Mark read on open; ignore |
| E2EE rooms | Listed with marker, placeholder, no fetch | Hide |

**Notes:** Web search confirmed Categories exist (8.9.0+, Enterprise); API unconfirmed.

## History loading & scroll

| Question | Selected | Alternatives |
|----------|----------|--------------|
| Fetch | On open, history_size, cached | Refetch each open; prefetch all |
| Older messages | Load next page at top | Initial window only |
| Message types | Text; placeholders for system/attachments | Skip non-text |
| Pane states | Pane placeholder + status error | Status bar only |

## Claude's Discretion

- API module layout, endpoints, timeouts, wording, Docker setup.

## Deferred Ideas

- Directory browsing/joining, unread counts/mark-read, TLS options, file logging.
