---
phase: 03-connect-read
reviewed: 2026-11-17T00:00:00Z
depth: standard
files_reviewed: 29
files_reviewed_list:
  - Cargo.toml
  - dev/README.md
  - dev/seed.py
  - scripts/smoke_pty.py
  - src/action.rs
  - src/api/map.rs
  - src/api/mod.rs
  - src/api/wire.rs
  - src/app/command.rs
  - src/app/history.rs
  - src/app/messages.rs
  - src/app/mod.rs
  - src/app/sidebar.rs
  - src/app/update.rs
  - src/config/connection.rs
  - src/config/mod.rs
  - src/config/paths.rs
  - src/config/settings.rs
  - src/data/fake.rs
  - src/data/mod.rs
  - src/effect.rs
  - src/input/actions.rs
  - src/lib.rs
  - src/main.rs
  - src/redaction.rs
  - src/ui/messages.rs
  - src/ui/mod.rs
  - src/ui/status.rs
  - src/ui/wrap.rs
findings:
  critical: 0
  warning: 5
  info: 6
  total: 11
  necessity: 0
status: issues_found
---

# Phase 3: Code Review Report

**Reviewed:** 2026-11-17
**Depth:** standard
**Files Reviewed:** 29
**Status:** issues_found

## Summary

The Rust side keeps the security constraints it set out to keep. The token lives only in `api::Client`, with both header values marked sensitive. Redirects are disabled, no `reqwest::Error` is ever formatted, and bodies are capped. `Debug` is redacted on `Action`, `Message`, `HistoryPage`, `Client` and `Secret`. The reducer stays pure and effects are drained only by `run()`. I found no path that logs the PAT or message text, and no path that sends the PAT to another host. I found no crash in the reducer paths I traced (cursor and anchor indexing in `history::on_older_page`, `messages::view`).

The weaknesses are on the edges:
- The dev tooling trusts unvalidated environment variables.
- The wire layer is lenient enough to turn a wrong response into a silent, permanent empty state.
- Composing a message while connected shows it as sent when it is not.
- Room (re)loads have no in-flight guard.

No BLOCKER-level defect was found. The 5 warnings are real behavior or safety gaps that should be fixed; the 6 info items are minor.

External reviewer evidence: none provided. Structural findings (fallow): none provided.

## Warnings

### WR-01: `dev/seed.py` and `scripts/smoke_pty.py` trust unvalidated `RT_*` environment values (path traversal in a destructive command, and credentials sent to a non-loopback host)

**File:** `dev/seed.py:31-42` (also `dev/seed.py:365-375`, `scripts/smoke_pty.py:834`, `scripts/smoke_pty.py:915`)
**Issue:** `RT_PROJECT` and `RT_HOST_PORT` are used raw.
- `STATE_DIR = join(REPO, "dev", "state", PROJECT)`. `cmd_down` then `os.remove`s every entry of `STATE_DIR` and `rmdir`s it. `RT_PROJECT=../..` makes `STATE_DIR` the repository root, so `down` deletes the top-level files (Cargo.toml, README and the like). `RT_PROJECT=""` makes it `dev/state` itself and removes every project's credentials. `RT_PROJECT` is also passed to `docker compose -p`.
- `BASE = "http://127.0.0.1:" + HOST_PORT`. `RT_HOST_PORT=3100@other.example` gives the URL `http://127.0.0.1:3100@other.example`, whose host is `other.example`. The generated admin and user passwords (`login`) and the PAT-bearing requests would then go to that host. This contradicts the module docstring ("Everything here talks to 127.0.0.1") and the README Security section.
- `smoke_pty.py` reads `RT_PROJECT` into a path the same way. Its loopback check on the URL is good, but the traversal is unchecked.

The values come from the developer's own environment, so this is self-inflicted rather than remote. The guarantees are still stated and a destructive command depends on them.
**Fix:**
```python
import re
if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,40}", PROJECT):
    sys.exit("RT_PROJECT must match [A-Za-z0-9][A-Za-z0-9_.-]*")
if not (HOST_PORT.isdigit() and 1 <= int(HOST_PORT) <= 65535):
    sys.exit("RT_HOST_PORT must be a port number")
```
Apply the same `RT_PROJECT` check in `smoke_pty.py` before building the state path. In `cmd_down`, assert `os.path.dirname(os.path.realpath(STATE_DIR))` equals `<REPO>/dev/state` before deleting anything.

### WR-02: `seed.py down` deletes the credentials even when `docker compose down -v` failed

**File:** `dev/seed.py:365-375`
**Issue:** The return code of `compose(["down", "-v"], ...)` is ignored (`check=False`), and the state directory is removed unconditionally afterwards. If compose fails (daemon stopped, a transient error), the Mongo volume and the containers survive. The generated admin password is already gone, so a later `up` creates new passwords that do not match the surviving volume, and `seed` cannot log in as admin. The README promises that `down` "removes containers, volume and the state directory", and it can silently remove only the last.
**Fix:**
```python
result = compose(["down", "-v"], admin_pass)
if result.returncode != 0:
    raise Fail("docker compose down failed; state directory kept so you can retry")
```
Also catch `FileNotFoundError` from `subprocess.run` (docker not installed) and report it as `Fail` instead of a traceback.

### WR-03: A message composed while connected is appended locally as "me" and looks sent, but is never sent

**File:** `src/app/update.rs:202-224` (`submit_input`), `src/app/mod.rs:63-64`
**Issue:** With a real connection (the binary always has one now), Enter in Insert mode appends a `Message { author: "me", id: "local-N" }` to the open room and clears the draft. There is no status line, marker or style that says the message stayed on this machine. The smoke scenarios `insert` and `config_layout` assert exactly this "appears on screen" behavior. Against the company server, the author would see their own text in the room while no colleague ever receives it. Sending is Phase 4, but Phase 3 ships a client that talks to a real server and fakes a successful send.
**Fix:** Until Phase 4, make the local-only state visible. Either set a status message on submit, or render local messages with a visible marker:
```rust
app.status_msg = Some(StatusMsg::Info("not sent: sending arrives in Phase 4".into()));
```
Alternatively refuse to submit while `app.conn != ConnState::Idle` and show an error.

### WR-04: Lenient wire types never check `success` and default the payload to empty, so a wrong response becomes "Ready with no rooms" or a permanently empty history

**File:** `src/api/wire.rs:55-56`, `src/api/wire.rs:82-83`, `src/api/wire.rs:122-123`; consumed at `src/api/mod.rs:257-258`, `src/app/history.rs:150-172`
**Issue:** `SubscriptionsResp.update`, `RoomsResp.update` and `HistoryResp.messages` are `#[serde(default)]`, and `success` is not read. Any HTTP 200 JSON object without those keys is accepted: a `{"success":false,"error":"..."}` body returned with 200, a proxy's JSON status page, or another JSON API at the same URL. The result:
- `load_rooms` returns `Ok` with zero rooms, `ConnState::Ready` and no error (the `BadResponse` text "is server_url a Rocket.Chat server?" is never reached).
- A history call returns an empty first page. `on_loaded` then stores `Loaded { exhausted: true }` and the pane shows "No messages yet". `retry_failed` and reopening only retry `Failed`, so that room stays blank for the whole session.

**Fix:** Require the discriminating field and fail closed. For example, add `#[serde(default)] pub success: bool` to each response struct and map `!success` to `ApiError::BadResponse` in `get_json` callers. A simpler alternative is to drop the `default` on `update` and `messages`, which Rocket.Chat always sends on success, so a missing key fails with `BadResponse`.

### WR-05: `:reconnect` has no in-flight guard and responses are applied in arrival order, so a stale room load can overwrite a newer one

**File:** `src/app/command.rs:31-36`, `src/app/update.rs:43-48`
**Issue:** `cmd_reconnect` always sets `ConnState::Connecting` and pushes a new `Effect::LoadRooms`, even if a load is already running. `run()` spawns each effect as an independent task. Two loads can be in flight, and the reducer applies whichever answer arrives last. A slow, older failure (`Action::LoadFailed`) arriving after a newer success flips the status bar to `Failed` while the rooms shown are fine. A slow, older success can replace the newer room list. The reconnect's `Connecting` also clears the earlier error text before the outcome is known. The comment in `command.rs` says a broken token "cannot hammer the server", but repeated `:reconnect` (or a counted `cmd:reconnect` binding, up to `MAX_COUNT`=9999 in `update.rs:35`) is unbounded.
**Fix:** Guard on state:
```rust
pub fn cmd_reconnect(app: &mut App, _args: &[&str]) -> Result<(), String> {
    if app.conn == ConnState::Connecting {
        return Ok(()); // a load is already in flight
    }
    app.conn = ConnState::Connecting;
    app.effects.push(Effect::LoadRooms);
    history::retry_failed(app);
    Ok(())
}
```
This is sufficient because `LoadRooms` is the only room-list request. For counted actions, collapse repeats (do not run non-idempotent commands `n` times).

## Info

### IN-01: Older-page paging can skip messages that share the boundary timestamp

**File:** `src/api/mod.rs:277-281`, `src/api/map.rs:204-231`
**Issue:** The next page is requested with `latest = <oldest message's ts>`, which Rocket.Chat treats as exclusive. If several messages share the exact same millisecond (bulk import, bot bursts) and the page limit splits them, the ones left out of the first page are never fetched. The id de-duplication in `on_older_page` would handle an inclusive boundary.
**Fix:** Optionally send `inclusive=true` (the id de-dup already handles the repeat), or document the limitation.

### IN-02: The best-effort preferences call still blocks the whole room load for up to the request timeout

**File:** `src/api/mod.rs:251-263`
**Issue:** `tokio::join!` waits for all three calls. If `users.getPreferences` hangs, the room list is delayed by `REQUEST_TIMEOUT` (15 s) even though its failure is ignored by design. This costs UX, not correctness.
**Fix:** Give the preferences request a short dedicated timeout, or apply the rooms first and the folders when preferences arrive. The two-action `FoldersLoaded` and `RoomsLoaded` shape already supports that.

### IN-03: Rooms the server marks `alert: true` but `unread: 0` are not shown as unread

**File:** `src/api/wire.rs:62-78`, `src/api/map.rs:81`; `dev/README.md:47-49`
**Issue:** `Room.unread` is `sub.unread > 0`. `alert` is not deserialized. The README documents that on the tested servers channels count only mentions, so a channel with new non-mention messages has `unread: 0, alert: true`. It never appears under the Unread section, unlike in the official client. If "find rooms with new activity" is part of the core value, this under-reports.
**Fix:** Decide the intended semantics. If alert-only rooms should count, add `#[serde(default)] pub alert: bool` to `WireSub` and use `sub.unread > 0 || sub.alert`.

### IN-04: The security warning on the status bar can be clipped when a failure text is also shown

**File:** `src/ui/status.rs:21-37`
**Issue:** Connection failure text, config warnings, pending keys and the status message share one unwrapped row. The longest warning ("server_url uses http:// to a non-local host: the token is sent unencrypted", about 75 columns) plus the mode badge and room name already exceeds 80 columns. The `Failed` error text is pushed before the warnings, so on a failure the warning is cut off first, at the moment the user is most likely to be misconfigured. The same text is printed to stderr on exit, so it is not lost.
**Fix:** Render warnings before the connection state, or give the warning its own row and truncate the less important spans.

### IN-05: Test-only fake data is compiled into the production library and `Action` surface

**File:** `src/app/mod.rs:153-160`, `src/data/fake.rs`, `src/action.rs:60-61`, `src/app/update.rs:53-64`
**Issue:** `lib.rs` states "There is no fake data", yet `App::with_fake_data`, `data::fake::seed` and `Action::MessagesLoaded` (with its reducer arm and `Debug`/keymap arms) are compiled in non-test builds. Their only non-test caller is `with_fake_data`, which is itself used only from `test_util` and tests. This is dead code in the shipped binary, not a trust-boundary guard.
**Fix:** Gate them with `#[cfg(test)]`, or move them under `test_util`, if that does not disturb the snapshot tests. Otherwise leave a comment naming them as test fixtures.

### IN-06: Housekeeping nits

**File:** `Cargo.toml:12`; `.gitignore`; `dev/seed.py:123-126,148`
**Issue:**
- The `reqwest` `json` feature is enabled but no `.json()` call exists; the code decodes with `serde_json::from_slice`.
- `scripts/__pycache__/` is untracked and not ignored (`git status` shows `?? scripts/__pycache__/`).
- `seed.py` can raise `KeyError` (`login`: `data["userId"]`) or `ValueError` (`json.loads` in `api`) outside the `Fail` handlers, so `up` and `seed` print a traceback instead of `FAIL:`. `up` only catches `Fail` while waiting for `/api/info`.

**Fix:** Drop the unused feature, add `__pycache__/` to `.gitignore`, and wrap those two spots to raise `Fail`.

---

_Reviewed: 2026-11-17_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
