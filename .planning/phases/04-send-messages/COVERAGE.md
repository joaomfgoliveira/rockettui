# API Coverage — Rocket.Chat REST API (Phase 4: Send Messages)

> Full coverage by default. Opt-outs are explicit, reasoned decisions.
>
> Scope note: the deterministic detector (`api-coverage.cjs`) returned `detected: false` on the phase scope, but Phase 4 genuinely adds Rocket.Chat REST calls (the send path), so the matrix is written anyway, as in Phase 3. The capability surface is the message-sending part of the REST API a terminal chat client could use; server baseline 8.9.1 with a 9.0.0 pass (plan 04-08). Each INTEGRATE row names the plan that delivers it. Phase 3's rows (rooms, history, preferences) are unchanged. Plan numbers follow the current plan set, revision 2 (plans 04-01 to 04-08): plan 04-01 was split into 04-01 and 04-02 and plan 04-07 was added for D-20 in revision 1, and the D-18, D-20 and D-21 amendments are applied.

| capability | decision | reason |
|---|---|---|
| chat.sendMessage (client-supplied `_id`, body `_id`/`rid`/`msg`) | INTEGRATE | plan 04-01 (client and result path) and plan 04-02 (Enter sends) (MSG-02, D-15, default (f)) |
| send error mapping (Failed vs Unconfirmed, 4xx as Rejected) | INTEGRATE | plans 04-01, 04-02 and 04-03 (D-06, D-07, D-18): 401/403/404/429/3xx/other 4xx, unreachable and TLS are Failed; 5xx (D-18), timeout and connection lost are Unconfirmed |
| chat.getMessage (reconcile before resending an unconfirmed message) | INTEGRATE | plan 04-03 (D-15) |
| me (own username, once per connection) | INTEGRATE | plan 04-06 (D-17) |
| chat.postMessage | OPT-OUT | explicitly out of scope: it cannot carry a client `_id`, so retries could duplicate messages (D-15) |
| chat.sendMessage optional fields (tmid, alias, emoji, attachments, ...) | OPT-OUT | not needed: plain text to the open room only (avatar, blocks and previewUrls too); threads and rich messages are v2 (MSG-05) |
| chat.update / chat.delete / chat.react / pin / star | OPT-OUT | explicitly out of scope: edit, delete and reactions are v2 (MSG-05) |
| thread replies (tmid) / chat.getThreadMessages | OPT-OUT | explicitly out of scope: threads are not rendered (Phase 3 D-15, MSG-05) |
| rooms.upload / rooms.media (file attachments) | OPT-OUT | explicitly out of scope: file uploads are out of scope (REQUIREMENTS) |
| commands.run (slash commands) | OPT-OUT | not needed yet: text is sent as typed (D-14); slash commands are not a v1 requirement |
| users.autocomplete / mention completion | OPT-OUT | not needed yet: no completion UI in v1 |
| typing indicator (user-activity / stream-notify-room typing) | OPT-OUT | not needed yet: realtime streams are Phase 5 and typing state is not a v1 requirement |
| subscriptions.read (mark the room read after sending) | OPT-OUT | explicitly out of scope: no mark-as-read from the client (Phase 3 D-11, MSG-06 is v2) |
| server-side drafts / message drafts sync | OPT-OUT | explicitly out of scope: drafts stay in memory (D-10); persisting drafts is a Deferred Idea |
| permission pre-check (permissions.listAll, rooms.info readOnly / muted) | OPT-OUT | explicitly out of scope: no client-side pre-check; the server's rejection surfaces as a failed message (D-11, Deferred Ideas) |
| settings (Message_MaxAllowedSize) in the app | OPT-OUT | not needed: no client-side length cap beyond the 64 KiB input cap (D-12); read only by the dev probe |
| realtime DDP echo of sent messages (stream-room-messages) | OPT-OUT | not needed yet: Phase 5 (MSG-03); its echo lands on the client `_id` through the D-13 helper from plan 04-05, whose held-id rules are D-21's (Sent: ignore; pending: confirm in place) |
| rate-limit headers (X-RateLimit-*) | OPT-OUT | not needed: a 429 is a failed send with manual retry only (default (e)) |
