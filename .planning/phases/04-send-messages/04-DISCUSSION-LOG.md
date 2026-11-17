# Phase 4: Send Messages - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-11-17
**Phase:** 4-Send Messages
**Areas discussed:** Send flow & optimistic echo, Send failure & retry, Draft & Enter semantics, Dedupe with Phase 5 realtime

---

## Send flow & optimistic echo
- Echo: **Optimistic, dim 'sending'** (alts: wait for server; optimistic with no marker)
- Draft clear: **Clear on Enter** (alt: clear on success)
- Concurrency: **Queued in order** (alts: fully parallel; block until done)

## Send failure & retry
- Failure UX: **Failed marker + retry key** (alts: restore draft; status error only)
- Idempotence: **Client _id, unconfirmed on timeout** (alt: plain retry)
- Recovery: **Independent of :reconnect** (alt: reconnect retries sends)

## Draft & Enter semantics
- Newline: **Keep existing binding** (alt: Alt-Enter/Ctrl-j)
- Draft scope: **Per-room drafts** (alt: single shared)
- Blocked rooms: **Block with status hint** (alt: block Insert mode)
- Size limit: **Server decides** (alt: client cap)

## Dedupe with Phase 5 realtime
- Reconcile: **Swap with server response** (alt: refetch history)
- Dedupe: **By message _id** (alt: leave to Phase 5)
- Scroll after send: user chose **both behaviors with a config option** (not one of the offered options)

## Claude's Discretion
- API/Effect/Action naming, queue mechanism, pending-state representation, marker glyphs, default retry/discard keys.

## Deferred Ideas
- Edit/delete/threads/reactions; disk-persisted drafts; client-side permission pre-check.
