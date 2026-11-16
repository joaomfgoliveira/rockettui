---
phase: quick-261116-eog
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - src/ui/palette.rs
  - src/ui/mod.rs
  - src/ui/wrap.rs
  - src/ui/messages.rs
autonomous: true
requirements: [NAV-04]

estimate:
  tokens: 40000
  raw_tokens: 40000
  tasks: 2
  confidence: low

must_haves:
  truths:
    - "In the messages pane every author name is drawn in a foreground color picked from a fixed 6-entry named-ANSI palette by a 64-bit FNV-1a hash of the sanitized name bytes, so the same username gets the same color in every room, session and platform (golden values: alice LightBlue, bob Cyan, carol Yellow, dave Green)"
    - "Only the name text is colored: the HH:MM time stays dim with no foreground, and the alignment padding, the separator and the body keep the default foreground (buffer fg Color::Reset)"
    - "Names stay bold, bodies still start at the shared author column from quick task 261116-ed1, and the rendered text and row counts are unchanged: all 7 insta snapshots stay byte-identical and row_starts/view still match the drawn rows"
    - "The color is derived from the full sanitized name before truncation, so a hostile name with bidi/control characters gets the color of its sanitized form, and a name truncated with an ellipsis on a narrow pane keeps the color of its full name"
    - "The TwoLine header colors the author name the same way"
    - "The palette avoids Reset/Black/White/Gray/DarkGray (default fg, theme-dependent invisibility, and the unfocused cursor-row background), the red family (status-line error color), Blue and the washed-out bright variants. Both cursor-row highlights still cover every cell of the cursor rows, and the author cells keep their color under them"
  artifacts:
    - path: "src/ui/palette.rs"
      provides: "AUTHOR_PALETTE (6 named ANSI colors), private fnv1a_64 hash, pub fn author_color(sanitized: &str) -> Color"
      contains: "pub fn author_color"
    - path: "src/ui/mod.rs"
      provides: "palette module registration"
      contains: "pub mod palette;"
    - path: "src/ui/wrap.rs"
      provides: "render_rows styles the author span with bold plus author_color, in both layouts"
      contains: "author_color"
    - path: "src/ui/messages.rs"
      provides: "buffer-level render tests for author colors, uncolored padding/body and the cursor highlights"
      contains: "author_color"
  key_links:
    - from: "src/ui/wrap.rs"
      to: "src/ui/palette.rs"
      via: "render_rows calls author_color on the sanitize_line output, before fit_author shadows the name with the truncated form"
      pattern: "author_color\\(&author\\)"
    - from: "src/ui/messages.rs"
      to: "src/ui/wrap.rs"
      via: "render draws the render_rows lines unchanged. The cursor highlight set_style adds REVERSED or bg DarkGray but never sets fg, so author colors survive"
      pattern: "wrap::render_rows"
---

<objective>
Color each author's username in the messages pane with a color that is deterministic per username. The color is picked from a small palette of named ANSI colors by a stable hash of the sanitized author name, so the same user always has the same color across rooms and sessions. Bold styling stays, and so does the shared author column from quick task 261116-ed1.

Purpose: a distinct, stable color per author makes a busy room much faster to scan by speaker. Rocket.Chat web does the same with avatars.
Output: new `src/ui/palette.rs`, a one-line module registration in `src/ui/mod.rs`, the author span styling in `src/ui/wrap.rs`, and new unit and buffer-level render tests. No snapshot changes and no Cargo changes.

**Hash (how and why):** a hand-written 64-bit FNV-1a over the sanitized name's UTF-8 bytes, with offset basis `0xcbf2_9ce4_8422_2325` and prime `0x0000_0100_0000_01b3`. The palette index is `hash % 6`. FNV-1a is unseeded and fully specified, so the mapping is identical across runs, platforms and Rust versions. The std hasher is randomly seeded per process and its algorithm is unspecified, so it cannot give a per-user color that stays stable. Bytes are hashed exactly as sanitized, with no case folding, because the server sends the canonical username. Published FNV-1a 64 test vectors pin the implementation: "" gives 0xcbf29ce484222325, "a" gives 0xaf63dc4c8601ec8c and "foobar" gives 0x85944171f73967e8 (verified in Python while planning).

**Palette (how and why):** `AUTHOR_PALETTE = [Cyan, Magenta, LightMagenta, Green, Yellow, LightBlue]`, in exactly that order. These are named ANSI colors 0-15, which the terminal theme remaps, so they adapt to dark and light themes. Fixed RGB and 256-cube indexed colors do not. Exclusions are grounded in the existing styles:
- Reset, Black, White and Gray are excluded. Reset is indistinguishable from the default fg and the uncolored focus border (`pane_block` sets only `BorderType`, no color). Black, White and Gray each disappear on one theme family.
- DarkGray is excluded because it is the unfocused cursor-row background (`Style::new().bg(Color::DarkGray)` in `src/ui/messages.rs`).
- Red and LightRed are excluded because status-line errors use `Color::Red` (`src/ui/status.rs`).
- Blue is excluded because it has the lowest contrast on dark backgrounds and on the DarkGray cursor row. LightBlue replaces it.
- LightGreen, LightCyan and LightYellow are excluded because they wash out on light backgrounds.

The focused cursor row adds REVERSED, which swaps the author color into the cell background, so it stays readable. With this order and hash, the fake authors map to 4 distinct colors: alice LightBlue, bob Cyan, carol Yellow, dave Green (computed while planning).

**Invariants:** only the author `Span`'s style changes. Text, padding and row structure are untouched, so `message_rows`, `row_starts`/`view` and every insta snapshot stay identical. The snapshots are plain text: ratatui's `TestBackend` Display writes cell symbols only, never styles. That is why colors are asserted on `Span.style` and on buffer cells (`cell.fg`, `cell.modifier`), and the snapshots must NOT change.

User-configurable author colors (theme.toml, CONF-06) belong to Phase 2 and are not part of this task.
</objective>

<execution_context>
@.claude/gsd-core/workflows/execute-plan.md
@.claude/gsd-core/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.claude/CLAUDE.md
@.planning/quick/261116-ed1-align-message-bodies-at-a-shared-x-colum/261116-ed1-SUMMARY.md
@src/ui/wrap.rs
@src/ui/messages.rs
@src/ui/mod.rs

<interfaces>
Unchanged signatures (src/ui/wrap.rs):
- `pub fn render_rows(msg: &Message, width: u16, layout: MessageLayout, author_col: usize) -> Vec<Line<'static>>`
- `pub fn message_rows(..) -> usize`, defined as `render_rows(..).len()` (keep it that way)
- private `fn fit_author(author: &str, col: usize) -> Cow<'_, str>`; `const LONG: &str = "firstname.lastname.contractor"` in wrap.rs tests

Current span structure, which must stay the same (only the author span's style changes):
- Compact, first row: `[time (dim), " " (raw), author (bold), pad (raw), body (raw)]`, so `spans[2]` is the author and `spans[3]` the padding. Continuation rows: `[indent (raw), row (raw)]`.
- TwoLine header: `[author (bold), "  " (raw), time (dim)]`. Body rows: `["  " (raw), row (raw)]`.
- In `render_rows`, `let author = sanitize_line(&msg.author).into_owned();` is line 97. The Compact arm then shadows `author` with `fit_author(&author, col)` at line 104. The color must be computed from the line-97 value, before that shadowing.

New API (src/ui/palette.rs): `pub fn author_color(sanitized: &str) -> ratatui::style::Color`.

Buffer facts (ratatui 0.30): `buffer[(x, y)]` exposes `fg: Color`, `bg: Color` and `modifier: Modifier`. An unstyled cell has `fg == Color::Reset`. `Span.style.fg` is `Option<Color>`, and `Span::raw` has `Style::default()`. In the 52x20 newest pane (`open_general_80x25`), inner x starts at 1: `HH:MM` occupies x=1..=5 with ':' at x=3, the author starts at x=7, the 5-wide author column ends at x=11, padding is x=12..=13, and the body starts at x=14. Continuation rows start with 13 spaces, so x=3 is ':' only on a message's first row. The seeded general-room authors are alice, bob, carol and dave, and all four are visible in that window.

Highlight code (src/ui/messages.rs `render`): focused `Style::new().add_modifier(Modifier::REVERSED)`, unfocused `Style::new().bg(Color::DarkGray)`, applied with `buffer_mut().set_style` per cursor row. Neither sets fg.

Existing test helpers in src/ui/messages.rs tests: `general()`, `pane_rect`, `render_pane`, `row_text`, `all_text`, `open_general_80x25`, `app_80x24`, `press`, `view`, `row_starts`. In src/ui/wrap.rs tests: `msg(author, body)`, `text`, `texts`, `solo`.
</interfaces>
</context>

<tasks>

<task type="tracer" tdd="true">
  <name>Task 1: Deterministic author color wired end-to-end in the Compact layout (palette, wrap, pane)</name>
  <files>src/ui/palette.rs, src/ui/mod.rs, src/ui/wrap.rs, src/ui/messages.rs</files>
  <behavior>
    - palette tests (src/ui/palette.rs): fnv1a_64(b"") == 0xcbf2_9ce4_8422_2325, fnv1a_64(b"a") == 0xaf63_dc4c_8601_ec8c, fnv1a_64(b"foobar") == 0x8594_4171_f739_67e8. Golden mapping: author_color("alice") == Color::LightBlue, ("bob") == Color::Cyan, ("carol") == Color::Yellow, ("dave") == Color::Green. author_color("") does not panic and returns a palette entry.
    - compact_author_colored (src/ui/wrap.rs): render_rows(&msg("bob", "hi"), 60, Compact, 5) still has the text "00:00 bob    hi". spans[2] has content "bob", style.fg == Some(author_color("bob")), and its add_modifier contains BOLD. spans[3] (padding) and spans[4] (body) have style == Style::default(). spans[0] (time) has style.fg == None and its add_modifier contains DIM.
    - authors_colored_in_buffer (src/ui/messages.rs, open_general_80x25, 52x20 pane): for every inner row y in 1..=18 whose cell (3, y) is ":", the name is the trimmed symbols of x=7..=11. Assert buffer[(7, y)].fg == author_color(name) and its modifier contains BOLD, and buffer[(12, y)].fg, buffer[(13, y)].fg and buffer[(14, y)].fg are all Color::Reset. On every other inner row (continuations), buffer[(1, y)].fg and buffer[(14, y)].fg are Color::Reset. Collect the colors of the names seen: exactly 4 distinct colors for alice, bob, carol and dave. At HEAD this fails, because every author cell has fg Reset.
  </behavior>
  <action>
Write the behavior tests first (RED), then implement (GREEN). Commit the task as one commit, tests and implementation together, like quick task 261116-ed1.

src/ui/palette.rs (new):
- Add a module doc: deterministic per-author colors for the messages pane. It must say that callers pass `sanitize_line` output (threat T-01-01/T-01-16), that the hash is FNV-1a 64 and why it is used (unseeded, fully specified, stable across runs and platforms), and that untrusted input only selects an index into a fixed table.
- Add a private `const AUTHOR_PALETTE: [Color; 6]` holding exactly `Color::Cyan, Color::Magenta, Color::LightMagenta, Color::Green, Color::Yellow, Color::LightBlue`, in that order. Give it a doc comment listing the exclusions and the reason for each, as in the objective's Palette paragraph. The order is part of the stable mapping, so reordering changes users' colors. Say so in the comment.
- Add a private `fn fnv1a_64(bytes: &[u8]) -> u64`. It starts from offset basis `0xcbf2_9ce4_8422_2325`, and for each byte it XORs `u64::from(byte)` into the hash, then calls `wrapping_mul` with the prime `0x0000_0100_0000_01b3`. Use named private consts for the basis and the prime. Do not use any standard-library or seeded hasher type, and add no crates.
- Add `pub fn author_color(sanitized: &str) -> Color`. It returns `AUTHOR_PALETTE[index]`, where index is `fnv1a_64(sanitized.as_bytes()) % AUTHOR_PALETTE.len()`, computed in u64 and converted back to usize. The value is always below 6, so indexing cannot panic. Its doc says the argument must already be sanitized, and that the bytes are hashed exactly as given, with no case folding.
- Add a `#[cfg(test)] mod tests` with the palette behavior tests.

src/ui/mod.rs: add `pub mod palette;` to the module list in alphabetical order, between `messages` and `sanitize`. Change nothing else.

src/ui/wrap.rs, `render_rows`: right after the existing `let author = sanitize_line(&msg.author).into_owned();`, compute the author color with `author_color(&author)` (import `crate::ui::palette::author_color`). Build `let name = bold.fg(color);`, after `bold` is defined. The color comes from the full sanitized name BEFORE the Compact arm shadows `author` with `fit_author`, so a truncated name keeps its full name's color at every width. In the Compact arm, the first-row author span uses `name` instead of `bold`. The padding span stays `Span::raw(pad.clone())`, the time stays `dim`, and the body and continuation rows stay raw, so only the name text is colored (D-10's compact shape and the 261116-ed1 alignment are unchanged). Do not change any text, padding width, row count or the cap math. Extend the `MessageLayout::Compact` doc comment to say the author name is bold and colored by `palette::author_color`. Add the `compact_author_colored` test. Every existing wrap.rs test must pass unchanged.

src/ui/messages.rs: no change to the rendering logic. Above the existing `highlight` definition in `render`, add a one-line comment: neither highlight sets a foreground, so author colors survive on the cursor rows (REVERSED swaps them into the background). Add the `authors_colored_in_buffer` test, using `crate::ui::palette::author_color` and `ratatui::style::Color` (already imported in the parent module).

Snapshots: do NOT set INSTA_UPDATE. The snapshots capture symbols only, so `cargo test` must pass with every `.snap` file byte-identical. A snapshot failure means text changed, which is a bug to fix, not a snapshot to regenerate.
  </action>
  <verify>
    <automated>cargo test --lib -- ui::palette::tests ui::wrap::tests ui::messages::tests ui::tests app::messages::tests && cargo clippy --all-targets -- -D warnings && cargo fmt --check</automated>
  </verify>
  <acceptance_criteria>
    - `grep -c 'pub mod palette;' src/ui/mod.rs` [HEAD=0 EXPECT==1 @8bbf6b1]
    - `grep -c 'fn fnv1a_64' src/ui/palette.rs` [HEAD=error:2 EXPECT>=1 @8bbf6b1]
    - `grep -c 'pub fn author_color' src/ui/palette.rs` [HEAD=error:2 EXPECT==1 @8bbf6b1]
    - `grep -v '^\s*//' src/ui/palette.rs | grep -c 'std::hash\|RandomState\|hash_map'` [HEAD=0 EXPECT==0 @8bbf6b1 GUARD MUTATION: adding a `std::hash::Hasher`-based import to src/ui/palette.rs makes the count 1]
    - `grep -v '^\s*//' src/ui/wrap.rs | grep -c 'author_color'` [HEAD=0 EXPECT>=2 @8bbf6b1] (import or call plus the new test)
    - `grep -v '^\s*//' src/ui/messages.rs | grep -c 'author_color'` [HEAD=0 EXPECT>=1 @8bbf6b1]
    - Invariant: `git diff --quiet -- src/ui/snapshots` exits 0, i.e. every snapshot is byte-identical
    - `find src/ui/snapshots -name '*.snap.new' | wc -l` [HEAD=0 EXPECT==0 @8bbf6b1 GUARD MUTATION: changing any rendered text makes the failing insta assertion write a .snap.new file]
  </acceptance_criteria>
  <done>In the 52x20 pane, alice, bob, carol and dave draw in 4 distinct palette colors, bold, while time, padding and body cells keep the default foreground. The FNV-1a vectors and golden mapping tests pass. Every pre-existing test in ui::wrap, ui::messages, ui::tests and app::messages passes with unchanged snapshots. Clippy and fmt are clean.</done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: TwoLine header color, full-name and sanitized-name keying, palette guard, highlight coexistence, full gate</name>
  <files>src/ui/palette.rs, src/ui/wrap.rs, src/ui/messages.rs</files>
  <behavior>
    - two_line_author_colored (src/ui/wrap.rs): render_rows(&msg("alice", long body), 30, TwoLine, 0) still has header text "alice  00:00". Header spans[0] has style.fg == Some(author_color("alice")) and BOLD. Header spans[1] has style == Style::default(), and header spans[2] (time) has fg None. Every body-row span has style.fg == None.
    - truncated_author_keeps_full_name_color (src/ui/wrap.rs): render_rows(&msg(LONG, "x"), 30, Compact, 29). spans[2] content is "firstname.l\u{2026}" and its style.fg == Some(author_color(LONG)). Also assert author_color(LONG) != author_color("firstname.l\u{2026}"), which is Cyan vs LightMagenta and proves the test discriminates.
    - hostile_author_colored_by_sanitized_name (src/ui/wrap.rs): for msg("\u{202e}mallory\u{1b}", "x") at width 60, Compact, solo, spans[2] content is "mallory" and its style.fg == Some(author_color("mallory")). That equals the fg rendered for msg("mallory", "x"). Assert author_color("mallory") != author_color("\u{202e}mallory\u{1b}") (Green vs LightMagenta) so the test proves the hash sees sanitized bytes.
    - palette_avoids_clashes (src/ui/palette.rs): AUTHOR_PALETTE has 6 pairwise-distinct entries. None is Reset, Black, White, Gray, DarkGray, Red, LightRed, Blue, LightGreen, LightCyan or LightYellow, and none matches Color::Rgb(..) or Color::Indexed(_). For 1000 generated names, author_color returns a member of AUTHOR_PALETTE.
    - author_color_survives_cursor_highlight (src/ui/messages.rs): app_80x24, open general, press "gg". The cursor message's first row is at y=1 and its name is the trimmed symbols of x=7..=11. buffer[(7, 1)].fg == author_color(name) and its modifier contains REVERSED. After pressing "<C-w>h" and re-rendering, buffer[(7, 1)].fg == author_color(name) and buffer[(7, 1)].bg == Color::DarkGray.
  </behavior>
  <action>
Write the behavior tests first (RED), then implement (GREEN). Commit the task as one commit.

src/ui/wrap.rs, the TwoLine arm of `render_rows`: the header author span uses the same `name` style (bold plus `author_color` of the full sanitized author) that Task 1 introduced. The separator stays `Span::raw("  ")`, the time stays `dim`, and body rows stay raw. TwoLine keeps the full-length author header and ignores `author_col`, as before (D-10). Update the `render_rows` doc comment: in both layouts the author name, and only the name, is bold and colored by `palette::author_color`, keyed on the full sanitized name, so truncation never changes a user's color. Add the three wrap.rs behavior tests. The existing `two_line_layout`, `two_line_ignores_author_col`, `sanitize_before_wrap` and `tiny_width_no_panic` tests must pass unchanged.

src/ui/palette.rs: add the `palette_avoids_clashes` test. Generate the 1000 names deterministically (for example `format!("user{i}")`), with no randomness. This test is the guard that keeps the exclusions in the objective's Palette paragraph from regressing if the palette is edited.

src/ui/messages.rs: add the `author_color_survives_cursor_highlight` test, mirroring the setup of the existing `cursor_rows_reversed_when_focused` and `cursor_row_not_reversed_when_unfocused` tests. No production change. The existing `cursor_rows_reversed_when_focused`, `cursor_row_not_reversed_when_unfocused`, `window_matches_view`, `bodies_share_one_column`, `long_author_truncated_and_aligned` and `hostile_message_sanitized` tests must stay green unchanged.

Do not touch Cargo.toml or Cargo.lock, src/app/messages.rs or any snapshot file.
  </action>
  <verify>
    <automated>cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && cargo build && python3 -I scripts/smoke_pty.py target/debug/rockettui quit && python3 -I scripts/smoke_pty.py target/debug/rockettui insert</automated>
    <human-check>Run `cargo run` in your usual dark theme (and a light theme if one is handy). Open #general. Confirm that alice, bob, carol and dave show 4 different readable colors, that padding and bodies stay in the default color, that the focused cursor row (reversed) and the unfocused cursor row (dark gray background) are still readable, and that bodies still line up at one column.</human-check>
  </verify>
  <acceptance_criteria>
    - `grep -c 'fn palette_avoids_clashes' src/ui/palette.rs` [HEAD=error:2 EXPECT==1 @8bbf6b1]
    - `grep -c 'fn truncated_author_keeps_full_name_color\|fn hostile_author_colored_by_sanitized_name\|fn two_line_author_colored' src/ui/wrap.rs` [HEAD=0 EXPECT==3 @8bbf6b1]
    - `grep -c 'fn author_color_survives_cursor_highlight' src/ui/messages.rs` [HEAD=0 EXPECT==1 @8bbf6b1]
    - Invariant: `git diff --quiet -- Cargo.toml Cargo.lock src/app/messages.rs src/ui/snapshots` exits 0, i.e. none of those paths changed
    - `find src/ui/snapshots -name '*.snap.new' | wc -l` [HEAD=0 EXPECT==0 @8bbf6b1 GUARD MUTATION: changing any rendered text makes the failing insta assertion write a .snap.new file]
  </acceptance_criteria>
  <done>In both layouts the author name, and only the name, is bold and colored by a stable FNV-1a hash of the full sanitized name. The palette guard, truncation, hostile-name and highlight-coexistence tests pass. The full `cargo test` suite passes with every snapshot unchanged. Clippy and fmt are clean, both pty smoke scenarios print SMOKE OK, and Cargo.toml, Cargo.lock and src/app/messages.rs are unchanged.</done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| Rocket.Chat data to renderer | Author names are untrusted text. They now also select an author color, in addition to feeding the drawn text, column width and truncation |

## STRIDE Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation Plan |
|-----------|----------|-----------|----------|-------------|-----------------|
| T-261116-eog-01 | Tampering | src/ui/wrap.rs render_rows to src/ui/palette.rs author_color | medium | mitigate | author_color only ever receives `sanitize_line` output, the same string that is drawn and measured (T-01-01/T-01-16 preserved). Untrusted bytes only pick an index `hash % 6` into a fixed const table of named ANSI colors, so no user data reaches an escape sequence and indexing cannot go out of bounds. Tests: hostile_author_colored_by_sanitized_name, palette_avoids_clashes (1000 names stay in the palette), hostile_message_sanitized stays green |
| T-261116-eog-02 | Spoofing | Author color as an identity cue | low | accept | With 6 colors, different users share colors by design, and anyone can pick a name that lands on a target's color. The color is a scanning aid, not an identity signal: the sanitized name text is always drawn next to it, and is ellipsis-marked when truncated (261116-ed1) |
| T-261116-eog-03 | Information Disclosure | FNV-1a hash of usernames | low | accept | The hash is non-cryptographic and only picks a color. It is never logged, stored or sent, and usernames are already displayed in clear. No message contents are hashed or logged |
| T-261116-eog-04 | Denial of Service | Per-frame hashing | low | accept | FNV-1a is O(name bytes) per rendered message, the same order as the sanitize_line call already made per message per frame. The pane still draws only the viewport slice |
| T-261116-eog-SC | Tampering | cargo dependencies | low | accept | No package installs. The hash is hand-written, and Cargo.toml and Cargo.lock are untouched (gate in Task 2) |
</threat_model>

<verification>
- `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check` is green.
- `git diff --stat -- src/ui/snapshots Cargo.toml Cargo.lock src/app/messages.rs` is empty, and no `.snap.new` file exists.
- `git diff --stat` lists only src/ui/palette.rs (new), src/ui/mod.rs, src/ui/wrap.rs and src/ui/messages.rs as source changes.
- The pty smoke scenarios `quit` and `insert` print SMOKE OK.
</verification>

<success_criteria>
- The same username always gets the same color, verified by FNV-1a published vectors and the golden alice/bob/carol/dave mapping.
- Only the name text is colored, in both layouts, and it stays bold.
- Alignment, wrapped text, row counts and all 7 snapshots are unchanged.
- Hostile and truncated names are colored by their full sanitized name.
- The palette excludes colors that clash with the cursor-row highlights, the default fg and border, and the status-line error color.
</success_criteria>

<output>
Create `.planning/quick/261116-eog-color-each-author-username-deterministic/261116-eog-SUMMARY.md` when done
</output>
