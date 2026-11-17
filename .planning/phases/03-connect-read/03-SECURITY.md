---
phase: "03"
slug: "connect-read"
status: verified
threats_open: 0
asvs_level: 1
created: "2026-11-17"
---

# Phase 03 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| client → Rocket.Chat server (REST) | HTTPS GETs authenticated with the PAT headers; redirects never followed | PAT, user id (secret); room and message data (private) |
| server text → terminal | Room, folder, author, body, attachment and system text drawn in ratatui | Untrusted text; sanitized at draw time |
| config.toml / environment → process | Server URL, user id and token read at startup | Credentials (secret) |
| dev tooling → local Docker stack | seed.py / smoke_pty.py talk to 127.0.0.1 only | Throwaway PAT and passwords (git-ignored, 0600) |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-03-01 | Information Disclosure | connection.rs, api/mod.rs | high | mitigate | `Secret` redacts Debug, no Display; sensitive headers | closed |
| T-03-02 | Information Disclosure | api/mod.rs | high | mitigate | `redirect::Policy::none()`; 3xx → `Redirected` | closed |
| T-03-03 | Information Disclosure | connection.rs | medium | mitigate | URL userinfo/query/fragment rejected, fixed error text | closed |
| T-03-04 | Spoofing | api/mod.rs | high | mitigate | rustls default TLS; no cert-verification weakening | closed |
| T-03-05 | Denial of Service | api/mod.rs get_json | medium | mitigate | 5 s/15 s timeouts, 8 MiB body cap, lenient wire types | closed |
| T-03-06 | Tampering | ui/sidebar.rs | high | mitigate | `sanitize_line` on room and folder names | closed |
| T-03-07 | Information Disclosure | cleartext http:// server_url | low | accept | In-app warning for non-loopback http | closed (accepted, AR-01) |
| T-03-08 | Information Disclosure | dev/seed.py state files | high | mitigate | 0600 files in 0700 dir, git-ignored, env not argv | closed |
| T-03-09 | Elevation of Privilege | dev/compose.yml | medium | mitigate | Bound to 127.0.0.1; per-stack random passwords | closed |
| T-03-10 | Information Disclosure | api/fixtures/docker | high | mitigate | `scrub` + `assert_no_secrets` before write | closed |
| T-03-11 | Tampering | docker.io images | low | accept | Image tags pinned in compose.yml | closed (accepted, AR-02) |
| T-03-12 | Information Disclosure | app/history.rs on_open | high | mitigate | Encrypted rooms never load | closed |
| T-03-13 | Tampering | ui/messages.rs, ui/wrap.rs | high | mitigate | `sanitize` / `sanitize_line` on body, author, title | closed |
| T-03-14 | Tampering | history query values | low | mitigate | `.query()` percent-encoding | closed |
| T-03-15 | Denial of Service | first-open fetches | low | mitigate | One request per first open, no prefetch | closed |
| T-03-16 | Information Disclosure | api/map.rs e2e | medium | mitigate | Fixed encrypted text before reading `msg` | closed |
| T-03-17 | Denial of Service | history.rs maybe_load_older | medium | mitigate | One in flight, `exhausted`, no auto retry | closed |
| T-03-18 | Tampering | attachment/system texts | high | mitigate | Plain text, sanitized at draw time | closed |
| T-03-19 | Information Disclosure | parse_file, Sources Debug | high | mitigate | Errors name type only; values wrapped in `Secret` | closed |
| T-03-20 | Information Disclosure | config.toml permissions | medium | mitigate | chmod 600 warning (status bar + stderr), never chmods | closed |
| T-03-21 | Information Disclosure | http:// to remote | medium | mitigate | Persistent warning | closed |
| T-03-22 | Tampering | warning text | low | mitigate | Fixed text, sanitized | closed |
| T-03-23 | Information Disclosure | redirect handling | high | mitigate | Two-server no-follow test | closed |
| T-03-24 | Information Disclosure | ApiError / reqwest::Error | high | mitigate | Error text never formatted from reqwest | closed |
| T-03-25 | Information Disclosure | Debug of Phase 3 types | high | mitigate | `sentinel_sweep` test | closed |
| T-03-26 | Tampering | hostile server text | high | mitigate | `hostile_server_text` test at two sizes | closed |
| T-03-27 | Denial of Service | retry vs rate limiter | medium | mitigate | No automatic retry; only `:reconnect` | closed |
| T-03-28 | Repudiation | absence of logs | low | accept | No log files or logger by design | closed (accepted, AR-03) |
| T-03-29 | Information Disclosure | smoke_pty.py docker_env | medium | mitigate | Env to child only; pass/fail output | closed |
| T-03-30 | Spoofing | non-local server in smoke | medium | mitigate | Refuses any host except 127.0.0.1/localhost | closed |
| T-03-31 | Tampering | fixture-driven mapping | low | mitigate | Fixtures written once, covered by mapping tests | closed |
| T-03-SC | Tampering | cargo installs (plan 03-01) | high | mitigate | Only reqwest, serde, serde_json, wiremock added; audited | closed |
| T-03-SC | Tampering | installs (plans 03-02..03-07) | low | accept | No later dependency changes; stdlib-only scripts | closed (accepted, AR-04) |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above workflow.security_block_on count toward threats_open*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-01 | T-03-07 | http:// to a remote host is the user's choice; rockettui shows a persistent warning | project author | 2026-11-17 |
| AR-02 | T-03-11 | Images pinned by tag, not digest; dev-only local stack | project author | 2026-11-17 |
| AR-03 | T-03-28 | Not logging avoids leaking message contents or tokens; no audit trail needed for a personal client | project author | 2026-11-17 |
| AR-04 | T-03-SC (03-02..03-07) | No dependency changes after the audited first commit | project author | 2026-11-17 |

*Accepted risks do not resurface in future audit runs.*

---

## Unregistered Flags and Notes

- **WR-01 (low, from 03-REVIEW.md, not accepted here):** dev/seed.py reads `RT_HOST_PORT` and `RT_PROJECT` unchecked. Only the developer's own environment can set them; smoke_pty.py keeps its own loopback check (T-03-30).
- **T-03-12 gap (informational):** `maybe_load_older` does not re-check `room.encrypted`, so a room that becomes encrypted after history loaded can still get an older-page request after `:reconnect`. Impact is limited by T-03-16.
- **T-03-04 (informational):** the "no TLS weakening" gate is a one-time grep, not a persistent test.

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-11-17 | 33 | 33 | 0 | gsd-security-auditor (4 low accept-rows closed by author acceptance) |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-11-17
