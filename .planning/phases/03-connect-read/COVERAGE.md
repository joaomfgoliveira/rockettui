# API Coverage — Rocket.Chat REST API (Phase 3: Connect & Read)

> Full coverage by default. Opt-outs are explicit, reasoned decisions.
>
> Scope note: the deterministic detector (`api-coverage.cjs`) returned `detected: false` on the phase scope, but Phase 3 genuinely integrates the Rocket.Chat REST API, so the matrix is written anyway (planner quality gate). Capabilities are the REST surface a terminal chat client could use; the server version baseline is 8.9.1 with a 9.0.0 compatibility pass (plan 03-07). Each INTEGRATE row names the plan that delivers it.

| capability | decision | reason |
|---|---|---|
| pat-header-auth (`X-User-Id` + `X-Auth-Token` on every request) | INTEGRATE | plan 03-01 (AUTH-01) |
| subscriptions.get (joined rooms, unread, favorite, category, open) | INTEGRATE | plan 03-01 (ROOM-01, D-09, D-10, D-11) |
| rooms.get (encrypted, lm, teamMain, prid) | INTEGRATE | plan 03-01 (ROOM-01, ROOM-03, D-12) |
| users.getPreferences (sidebarCategories) | INTEGRATE | plan 03-01 Task 2, best effort (D-10) |
| channels.history | INTEGRATE | plan 03-03 (MSG-01) |
| groups.history | INTEGRATE | plan 03-03 (MSG-01) |
| im.history | INTEGRATE | plan 03-03 (MSG-01) |
| history-paging (`latest` cursor, `count`) | INTEGRATE | plan 03-04 (D-14) |
| http-status-error-mapping (401/403/404/429/3xx/5xx) | INTEGRATE | plans 03-01 and 03-06 (D-06) |
| rate-limit-response (429 + X-RateLimit-* headers) | INTEGRATE | mapped to RateLimited with a manual-retry message; headers are not parsed (no automatic retry by D-06) |
| login (username + password, optional OTP) | OPT-OUT | explicitly out of scope: AUTH-04 is v2; Phase 3 authenticates with a PAT only |
| logout / token revocation | OPT-OUT | not needed: PATs are revoked by the user in the official client; the app holds no session |
| users.generatePersonalAccessToken | OPT-OUT | not needed in the app: the user creates the PAT in the official client; used only by the dev seed script (plan 03-02) |
| me (current user profile) | OPT-OUT | not needed yet: the user id comes from config/env and no profile data is shown |
| info (server version) | OPT-OUT | not needed yet: compatibility is handled by status-first error mapping; dev/seed.py uses it only for readiness |
| subscriptions.getOne | OPT-OUT | not needed: subscriptions.get returns the full list in one call |
| subscriptions.read (mark as read) | OPT-OUT | explicitly out of scope: D-11 forbids marking read on open; mark-as-read is v2 (MSG-06) |
| subscriptions.unread (mark unread) | OPT-OUT | explicitly out of scope: unread management is v2 (MSG-06) |
| rooms.info | OPT-OUT | not needed: rooms.get already carries every field Phase 3 uses |
| rooms.history (9.0.0 cursor endpoint) | OPT-OUT | not needed yet: exists only from 9.0.0 and the company server version is unknown (RESEARCH Alternatives); the per-type history routes work on 8.9 and 9.0 |
| chat.syncMessages | OPT-OUT | not needed yet: incremental sync fits Phase 5 realtime gap-filling, not "latest N" |
| chat.getMessage | OPT-OUT | not needed: messages arrive in history pages |
| chat.postMessage / chat.sendMessage | OPT-OUT | explicitly out of scope: sending is Phase 4 (MSG-02) |
| chat.update / chat.delete / chat.react | OPT-OUT | explicitly out of scope: edit, delete and reactions are v2 (MSG-05) |
| chat.getThreadMessages / thread replies in history | OPT-OUT | explicitly out of scope: threads are not rendered (D-15, MSG-05); history is fetched without thread replies |
| chat.search | OPT-OUT | explicitly out of scope: search is v2 (MISC-01) |
| rooms.favorite / users.setPreferences / experimental rooms.setCategory | OPT-OUT | explicitly out of scope: Phase 3 is read-only (only GET requests); used only by the dev seed script |
| channels.list / directory / channels.join / im.create | OPT-OUT | explicitly out of scope: browsing and joining rooms is a deferred idea (CONTEXT Deferred Ideas) |
| rooms.createDiscussion / teams.* management | OPT-OUT | explicitly out of scope: admin and creation features are not a goal; discussions and teams are only listed (D-09) |
| rooms.upload / file download | OPT-OUT | explicitly out of scope: file uploads are out of scope (REQUIREMENTS); attachments render as `[attachment: name]` text (D-15) |
| users.info / avatars / presence | OPT-OUT | not needed: the TUI is text-only and shows usernames from message and subscription data |
| e2e key endpoints (e2e.fetchMyKeys, e2e.getUsersOfRoomWithoutKey, ...) | OPT-OUT | explicitly out of scope: E2EE rooms show "encrypted, not supported" (ROOM-03, REQUIREMENTS Out of Scope) |
| livechat / omnichannel | OPT-OUT | explicitly out of scope: livechat rooms (t=l) are skipped; not a personal-chat feature |
| admin endpoints (settings, permissions, users.create) | OPT-OUT | explicitly out of scope: admin features are not a goal (REQUIREMENTS Out of Scope); used only by the dev seed script |
| realtime DDP / WebSocket (stream-room-messages) | OPT-OUT | not needed yet: realtime is Phase 5 (MSG-03, MSG-04) |
| custom CA / insecure TLS options | OPT-OUT | explicitly deferred: D-08 and CONTEXT Deferred Ideas (revisit in Phase 6) |
