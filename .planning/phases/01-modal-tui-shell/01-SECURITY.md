---
phase: "01"
slug: "modal-tui-shell"
status: verified
threats_open: 0
asvs_level: 1
created: "2026-11-15"
---

# Phase 01 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| data source -> App state / renderers | Fake data now, untrusted server data from Phase 3 | Room names, authors, message bodies |
| terminal input -> channel / matcher | Keystrokes, resizes, bracketed paste | Typed and pasted text |
| process -> terminal state | Raw mode, alternate screen, paste mode must be restored | Terminal state |
| clipboard -> input buffer | Pasted text of any size | Draft text |
| crate registry -> build | Third-party crates compiled in | Source code |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-01-01 | Tampering | src/ui/sanitize.rs, ui/*.rs | high | mitigate | sanitize/sanitize_line strip ESC/CSI/OSC/BEL/NUL/DEL/C1 before width math; all renderers call them | closed |
| T-01-02 | Spoofing | src/ui/sanitize.rs | medium | mitigate | Bidi, zero-width and tag-block chars stripped (is_spoofing) | closed |
| T-01-03 | Information Disclosure | action.rs, data/mod.rs, input/key.rs | medium | mitigate | Hand-written redacting Debug; no print macros or logging in src/ | closed |
| T-01-04 | Denial of Service | src/tui.rs, main.rs | medium | mitigate | Panic hook + tui::restore() on every exit; smoke quit/panic scenarios | closed |
| T-01-05 | Tampering | src/tui.rs, event.rs | medium | mitigate | Bracketed paste enabled; paste arrives as one Action::Paste | closed |
| T-01-06 | Denial of Service | ui/layout.rs, ui/mod.rs | low | mitigate | MIN_WIDTH/MIN_HEIGHT degrade path, saturating math, tiny_sizes test | closed |
| T-01-07 | Information Disclosure | Cargo.toml / Cargo.lock | low | mitigate | No HTTP/WebSocket crates; tokio has no net feature | closed |
| T-01-08 | Denial of Service | process signals | low | accept | SIGTERM/SIGHUP leave terminal raw; user can run reset (assumption A5) | closed |
| T-01-09 | Denial of Service | input/matcher.rs, app/update.rs | medium | mitigate | Counts saturate and clamp to MAX_COUNT 9999 | closed |
| T-01-10 | Information Disclosure | ui/status.rs, input/matcher.rs | low | mitigate | Only Normal-mode chords pend; redacting Debug for Outcome/Matcher | closed |
| T-01-11 | Denial of Service | src/lib.rs | low | mitigate | Bounded channel (256); batches drained and drawn once | closed |
| T-01-12 | Tampering | input/keymap.rs | low | mitigate | parse_keys returns typed ParseError; keymap parse tests | closed |
| T-01-13 | Tampering | ui/sidebar.rs | high | mitigate | Room/folder names pass through sanitize_line | closed |
| T-01-14 | Spoofing | ui/sidebar.rs | medium | mitigate | Bidi stripped; kind glyph drawn by app, not from name | closed |
| T-01-15 | Denial of Service | app/sidebar.rs | low | mitigate | Cursor re-clamped by on_rows_changed; saturating index math | closed |
| T-01-16 | Tampering | ui/wrap.rs, ui/messages.rs | high | mitigate | Sanitize before wrap; authors via sanitize_line | closed |
| T-01-17 | Denial of Service | ui/messages.rs, ui/wrap.rs | medium | mitigate | Only viewport rows drawn; width 0/1 handled | closed |
| T-01-18 | Denial of Service | app/messages.rs | medium | mitigate | view() clamps cursor and scrolls into view on every read | closed |
| T-01-19 | Denial of Service | app/input_buf.rs | medium | mitigate | MAX_INPUT_BYTES 64 KiB cap, char-boundary truncation | closed |
| T-01-20 | Tampering | app/input_buf.rs, ui/input.rs | high | mitigate | insert_char/insert_str sanitize; renderer uses sanitize_line | closed |
| T-01-21 | Information Disclosure | app/input_buf.rs | medium | mitigate | Debug prints only line count and byte length | closed |
| T-01-22 | Tampering | app/update.rs | medium | mitigate | Paste is one insert and never reaches submit_input | closed |
| T-01-23 | Spoofing | app/update.rs | low | accept | Local messages use fixed LOCAL_AUTHOR "me"; replaced by server authors in Phase 4 | closed |
| T-01-24 | Denial of Service | app/input_buf.rs, app/update.rs | low | mitigate | Clamped counted motions and line/column bounds | closed |
| T-01-25 | Denial of Service | app/update.rs | low | mitigate | MAX_CMDLINE 256 cap on typed and pasted text | closed |
| T-01-SC1 | Tampering | cargo installs | high | mitigate | Crates audited OK in RESEARCH and pinned by tracked Cargo.lock | closed |
| T-01-SC2 | Tampering | cargo installs | high | mitigate | No new crates; Cargo.toml and Cargo.lock untouched by this plan | closed |
| T-01-SC3 | Tampering | cargo installs | high | mitigate | No new crates; Cargo.toml and Cargo.lock untouched by this plan | closed |
| T-01-SC4 | Tampering | cargo installs | high | mitigate | No new crates; Cargo.toml and Cargo.lock untouched by this plan | closed |
| T-01-SC5 | Tampering | cargo installs | high | mitigate | No new crates; Cargo.toml and Cargo.lock untouched by this plan | closed |
| T-01-SC6 | Tampering | cargo installs | high | mitigate | No new crates; Cargo.toml and Cargo.lock untouched by this plan | closed |

*Status: open · closed. Verified by gsd-security-auditor with file/test evidence; `cargo test` 149/149 and pty smoke scenarios passed.*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-01-01 | T-01-08 | SIGTERM/SIGHUP do not restore the terminal; single local user can run `reset`; revisit when networking adds long-lived tasks (A5) | Accepted in plan 01-02 threat model | 2026-11-15 |
| AR-01-02 | T-01-23 | Local messages carry fixed author "me"; replaced by server-assigned authors in Phase 4 | Accepted in plan 01-06 threat model | 2026-11-15 |

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-11-15 | 31 | 31 | 0 | gsd-security-auditor |

Auditor observations (non-blocking): `log` is an indirect dependency via mio/crossterm (no logger installed; recheck if one is added); `App::with_fake_data` is a second, test-only reducer caller; message wrapping cost grows with room size (revisit in Phase 3).

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-11-15
