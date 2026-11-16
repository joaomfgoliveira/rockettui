---
phase: "02"
slug: configuration-system
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: "2026-11-16"
---

# Phase 02 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| config files -> config loader | User-edited TOML enters the process; untrusted until validated | config.toml, keymap.toml, theme.toml |
| environment -> config_dir | $XDG_CONFIG_HOME and $HOME choose which directory is read | directory path |
| problem text -> terminal and stderr | Keys and short values from config files are drawn on the error screen and printed after restore | short config strings |
| validated Settings -> layout and renderer | User-chosen widths and strftime output reach geometry math and the terminal | widths, timestamp format |
| message timestamps -> formatter | Server-controlled timestamps (Phase 3) are formatted and drawn | timestamps |
| keymap.toml -> Keymap | User-written key notation and action names decide what every key press does | key bindings, `cmd:` lines |
| cmd: bindings -> command table | A bound command line is executed by the reducer on a key press | command text |
| theme.toml -> Theme | User-chosen colors, modifiers, border types and marker strings reach every renderer | styles, marker strings |
| theme.toml author_palette -> author_color | A user-supplied list decides the color of every author name | color list |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-02-01 | Information Disclosure | src/config/diag.rs | medium | mitigate | message()+span() only, own line_col, toml Display unused, echoed values capped; `no_source_echo` | closed |
| T-02-02 | Tampering | src/ui/error_screen.rs, Report Display | high | mitigate | `sanitize_line` on key/message/dir before draw and stderr dump; `hostile_text_sanitized` | closed |
| T-02-03 | Denial of Service | src/config/paths.rs | medium | mitigate | metadata first, regular files only, 1 MiB cap; `read_source_hardening` | closed |
| T-02-04 | Tampering | src/config/paths.rs | low | mitigate | relative or empty $XDG_CONFIG_HOME ignored; `config_dir_resolution` | closed |
| T-02-05 | Denial of Service | src/config/settings.rs | medium | mitigate | StrftimeItems validation + sample render via `write!`; no unwrap on user data | closed |
| T-02-06 | Information Disclosure | config file permissions | low | accept | See AR-02-01 | closed |
| T-02-07 | Denial of Service | src/ui/layout.rs, src/app/mod.rs | low | mitigate | saturating layout math; `sidebar_wider_than_terminal` | closed |
| T-02-08 | Tampering | src/ui/timefmt.rs | medium | mitigate | load-time format checks + `sanitize_line` on output | closed |
| T-02-09 | Denial of Service | src/ui/timefmt.rs | low | mitigate | `timestamp_opt().single()` None / formatter error -> "?"; `out_of_range_is_placeholder` | closed |
| T-02-10 | Denial of Service | src/config/keymap.rs | medium | mitigate | `check_lockout` per mode; `lockout_guard` | closed |
| T-02-11 | Elevation of Privilege | src/config/keymap.rs, src/input/actions.rs | medium | mitigate | curated action names only; `cmd:` checked against COMMANDS; no process spawn; `internal_actions_unnamed` | closed |
| T-02-12 | Information Disclosure | src/action.rs | low | mitigate | `RunCommand(..)` Debug redacted; `debug_redacts` | closed |
| T-02-13 | Denial of Service | src/config/keymap.rs | low | mitigate | `reject_text_eaters`; `insert_sequence_rejected` | closed |
| T-02-14 | Tampering | src/config/theme.rs markers | high | mitigate | marker rejected unless `sanitize_line` leaves it unchanged; `markers_validation` | closed |
| T-02-15 | Spoofing | room-kind glyphs | low | accept | See AR-02-02 | closed |
| T-02-16 | Denial of Service | src/ui/sidebar.rs width math | low | mitigate | marker width caps (4, 12) + derived placeholder; `themed_markers` | closed |
| T-02-17 | Denial of Service | src/ui/palette.rs, src/config/theme.rs | high | mitigate | empty or >64 palette is a load error; `author_color` returns Reset on empty; `palette_validation`, `empty_palette_is_reset` | closed |
| T-02-18 | Tampering | author colors -> terminal | low | accept | See AR-02-03 | closed |
| T-02-SC1 | Tampering | cargo installs (toml, chrono) | high | mitigate | default features off, Cargo.lock committed, networking-crate guard reads 0, RESEARCH audit OK | closed |
| T-02-SC2 | Tampering | cargo installs | high | mitigate | Cargo.toml/Cargo.lock untouched by plan 02-02 | closed |
| T-02-SC3 | Tampering | cargo installs | high | mitigate | Cargo.toml/Cargo.lock untouched by plan 02-03 | closed |
| T-02-SC4 | Tampering | cargo installs | high | mitigate | Cargo.toml/Cargo.lock untouched by plan 02-04 | closed |
| T-02-SC5 | Tampering | cargo installs | high | mitigate | Cargo.toml/Cargo.lock untouched by plan 02-05 | closed |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above workflow.security_block_on count toward threats_open*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-02-01 | T-02-06 | Phase 2 config files hold no credentials; the chmod 600 warning is AUTH-02 in Phase 3 and will use `Report.warnings` | Accepted in plan 02-01 threat model | 2026-11-16 |
| AR-02-02 | T-02-15 | A user may theme two room kinds with the same glyph; glyphs come from the user's own theme, never from server-controlled room names | Accepted in plan 02-04 threat model | 2026-11-16 |
| AR-02-03 | T-02-18 | Palette entries are parsed into the typed ratatui Color at load; no user string reaches an escape sequence; author names still pass through `sanitize_line` | Accepted in plan 02-05 threat model | 2026-11-16 |

*Accepted risks do not resurface in future audit runs.*

---

## Auditor Notes (informational, non-blocking)

1. keymap.toml key-notation parse errors may echo up to about 78 characters of user text (`cap_message`, 96-char limit), above the 32-character wording in plan 02-01. It affects keymap.toml only, is sanitized on output, and was specified by plan 02-03. If config.toml gains a similar path (e.g. Phase 3 credentials), use the 32-character `cap()`.
2. `paths.rs` has a check-then-read gap between metadata and read; exploiting it needs write access to the user's own config directory.

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-11-16 | 23 | 23 | 0 | gsd-security-auditor (opus) |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-11-16
