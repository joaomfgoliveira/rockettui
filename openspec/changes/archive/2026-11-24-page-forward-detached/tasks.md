## 1. API

- [x] 1.1 Extend `dev/probe_search.py` with the `oldest`+`offset` probes (nearest N after a time, `offset=M` empty, exclusive `oldest`) and run it on 8.9.1 and 9.0.0; record the result in `dev/README.md`
- [x] 1.2 Add `Client::load_newer` (the 100-message first request, the offset probing bounded at 24 requests, the final page) returning the page, its newest raw timestamp and `reached_newest`; wiremock tests: short answer, exactly 100, many newer, none newer, an error from any request, the probing bound
- [x] 1.3 Use it for the newer side of `Client::load_context`; drop `newer_missing` and `CONTEXT_NEWER_CAP`; return the window's newest raw timestamp and whether it reaches the newest; update the context tests

## 2. State

- [x] 2.1 Turn `App.detached` into a map `RoomId -> Detached { newest_raw, loading_newer }`; the jump sets it from the window and does not detach when the window already reaches the newest; remove the "newer not shown" and "ends before the newest" statuses
- [x] 2.2 Add `Effect::LoadNewer`, `Action::NewerLoaded` and `Action::NewerFailed` with redacting `Debug`; run the effect in `effect.rs`
- [x] 2.3 Trigger a page after a messages-pane motion onto the last loaded message of a detached room (one in flight), apply the page (skip held ids, append, keep cursor and top row, update `newest_raw`), drop stale answers
- [x] 2.4 On `reached_newest` end the detached state, set the status, and record one `refetch_newest`; on failure clear the in-flight flag and show the error
- [x] 2.5 Tests for the reducer behavior above, including `G`, `:reload` and room switch still returning to the newest, and that a non-detached room is unchanged

## 3. Verification

- [x] 3.1 Extend the `docker_search` smoke scenario: jump to an old result, move down onto the end, check newer messages appear and the status line says when the newest is reached
- [x] 3.2 Run `cargo fmt`, `cargo clippy`, `cargo test`, the smoke scenario on 8.9.1 and 9.0.0, and the probe on 9.0.0
