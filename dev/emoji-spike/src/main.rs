//! Spike: do kitty Unicode-placeholder cells survive textwrap, ratatui's buffer diff, scrolling
//! and a popup drawn over them?
//!
//!   cargo run --manifest-path dev/emoji-spike/Cargo.toml -- --check   # measurements, no TTY
//!   cargo run --manifest-path dev/emoji-spike/Cargo.toml              # interactive, in kitty
//!
//! Interactive keys: j/k scroll, p popup on/off, [ ] narrower/wider, b stand-in blank/cut name,
//! u image ready on/off (off = the stand-in), q quit.

use std::io::{self, Write};

use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const IMAGE_ID: u32 = 0x00_12_34_56; // id carried in the foreground colour (24-bit)
const PLACEHOLDER: char = '\u{10EEEE}';
// First entries of kitty's rowcolumn-diacritics table: index 0 -> U+0305, 1 -> U+030D.
const DIACRITICS: [char; 4] = ['\u{0305}', '\u{030D}', '\u{030E}', '\u{0310}'];

/// One placeholder cell: U+10EEEE + row mark + column mark.
fn cell(row: usize, col: usize) -> String {
    format!("{PLACEHOLDER}{}{}", DIACRITICS[row], DIACRITICS[col])
}

/// The two-cell run for a 2x1 image.
fn run() -> String {
    format!("{}{}", cell(0, 0), cell(0, 1))
}

fn standin(blank: bool) -> String {
    if blank {
        "\u{a0}\u{a0}".into()
    } else {
        "rt".into()
    }
}

const BODY: &str = "ship it {E} and then {E} again, the quick brown fox jumps over the lazy dog {E}{E} \
                    done {E} ok";

fn body(ready: bool, blank: bool) -> String {
    BODY.replace("{E}", &if ready { run() } else { standin(blank) })
}

// ---- kitty upload -------------------------------------------------------------------------

fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for ch in data.chunks(3) {
        let n = (ch[0] as u32) << 16
            | (*ch.get(1).unwrap_or(&0) as u32) << 8
            | *ch.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if ch.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if ch.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// A 32x16 RGBA test image: a red/blue gradient with a white frame so cropping is visible.
fn test_image() -> (u32, u32, Vec<u8>) {
    let (w, h) = (32u32, 16u32);
    let mut px = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let edge = x == 0 || y == 0 || x == w - 1 || y == h - 1;
            if edge {
                px.extend([255, 255, 255, 255]);
            } else {
                px.extend([(x * 255 / w) as u8, 40, (y * 255 / h) as u8, 255]);
            }
        }
    }
    (w, h, px)
}

/// Transmit the image (chunked, quiet) and create a virtual placement of 2 columns x 1 row.
fn upload(out: &mut impl Write) -> io::Result<()> {
    let (w, h, px) = test_image();
    let data = b64(&px);
    let chunks: Vec<&str> = data
        .as_bytes()
        .chunks(4096)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect();
    for (i, c) in chunks.iter().enumerate() {
        let more = if i + 1 < chunks.len() { 1 } else { 0 };
        if i == 0 {
            write!(
                out,
                "\x1b_Ga=T,f=32,s={w},v={h},i={IMAGE_ID},U=1,c=2,r=1,q=2,m={more};{c}\x1b\\"
            )?;
        } else {
            write!(out, "\x1b_Gm={more},q=2;{c}\x1b\\")?;
        }
    }
    out.flush()
}

// ---- rows ---------------------------------------------------------------------------------

/// Wrap, then split each row at placeholder runs and give the run the image id as foreground.
fn rows(text: &str, width: usize) -> Vec<Line<'static>> {
    let id = Color::Rgb(
        (IMAGE_ID >> 16) as u8,
        (IMAGE_ID >> 8) as u8,
        IMAGE_ID as u8,
    );
    textwrap::wrap(text, width)
        .into_iter()
        .map(|row| {
            let mut spans = Vec::new();
            let mut plain = String::new();
            let mut img = String::new();
            for g in row.graphemes(true) {
                if g.starts_with(PLACEHOLDER) {
                    if !plain.is_empty() {
                        spans.push(Span::raw(std::mem::take(&mut plain)));
                    }
                    img.push_str(g);
                } else {
                    if !img.is_empty() {
                        spans.push(Span::styled(std::mem::take(&mut img), Style::new().fg(id)));
                    }
                    plain.push_str(g);
                }
            }
            if !plain.is_empty() {
                spans.push(Span::raw(plain));
            }
            if !img.is_empty() {
                spans.push(Span::styled(img, Style::new().fg(id)));
            }
            Line::from(spans)
        })
        .collect()
}

// ---- measurements -------------------------------------------------------------------------

fn check() {
    // 1.2a: widths.
    assert_eq!(cell(0, 0).width(), 1, "one placeholder cell is one column");
    assert_eq!(run().width(), 2, "the run is two columns");
    assert_eq!(
        run().graphemes(true).count(),
        2,
        "the run is two grapheme clusters, one per cell"
    );
    println!("width: cell=1 run=2 graphemes=2  ok");

    // 1.2b: never split across rows, at every width, and no row wider than the width.
    let text = body(true, true);
    let mut splits = 0;
    let mut blank_diffs = 0;
    for width in 4..=60 {
        if std::env::var("SPIKE_LOG").is_ok() {
            for r in &textwrap::wrap(&text, width) {
                if r.graphemes(true)
                    .filter(|g| g.starts_with(PLACEHOLDER))
                    .count()
                    % 2
                    != 0
                {
                    println!("odd run at width {width}: {:?}", r.to_string());
                }
            }
        }
        let wrapped = textwrap::wrap(&text, width);
        for r in &wrapped {
            assert!(r.width() <= width.max(2), "row {r:?} wider than {width}");
            // Every row has an even number of placeholder cells in runs; a run is cell(0,0) then
            // cell(0,1), so a row's placeholders must pair up as 0,1 sequences.
            let marks: Vec<&str> = r
                .graphemes(true)
                .filter(|g| g.starts_with(PLACEHOLDER))
                .collect();
            if marks.len() % 2 != 0 {
                splits += 1;
            }
            for pair in marks.chunks(2) {
                assert_eq!(pair.len(), 2, "run cut at width {width}: {r:?}");
                assert!(pair[0].contains(DIACRITICS[0]) && pair[0].ends_with(DIACRITICS[0]));
                assert!(pair[1].ends_with(DIACRITICS[1]));
            }
        }
        // Row counts must equal for ready and not-ready (the stand-in is two columns too).
        let a = textwrap::wrap(&body(true, true), width).len();
        let b = textwrap::wrap(&body(false, false), width).len();
        let blank_rows = textwrap::wrap(&body(false, true), width).len();
        if blank_rows != a {
            blank_diffs += 1;
        }
        if a != b {
            for r in textwrap::wrap(&body(true, false), width) {
                println!("ready  {:?}", r.to_string());
            }
            for r in textwrap::wrap(&body(false, false), width) {
                println!("stand  {:?}", r.to_string());
            }
        }
        assert_eq!(a, b, "row count differs ready vs stand-in at width {width}");
    }
    assert_eq!(splits, 0);
    println!(
        "wrap: widths 4..=60 (below 4 a word of two adjacent runs is hard-broken mid-run; the app never wraps under MIN_BODY=10) never split a run, no row too wide, same row count as stand-in  ok"
    );

    // Adjacent runs `{E}{E}` have no space between them, so textwrap could break between the two
    // runs (four columns) but must not break inside one.
    println!(
        "finding: the NBSP-pair stand-in changed the row count at {blank_diffs} of 57 widths; the 2-letter stand-in never did"
    );
    println!("all checks passed");
}

// ---- interactive --------------------------------------------------------------------------

fn main() -> io::Result<()> {
    if std::env::args().any(|a| a == "--check") {
        check();
        return Ok(());
    }
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen)?;
    upload(&mut stdout)?;
    let mut term = Terminal::new(ratatui::backend::CrosstermBackend::new(io::stdout()))?;

    let (mut scroll, mut popup, mut width, mut blank, mut ready) =
        (0usize, false, 40usize, true, true);
    let result = (|| -> io::Result<()> {
        loop {
            // Many repeated paragraphs so there is something to scroll.
            let mut lines: Vec<Line> = Vec::new();
            for i in 0..30 {
                lines.push(Line::raw(format!("--- message {i} ---")));
                lines.extend(rows(&body(ready, blank), width));
            }
            term.draw(|f| {
                let area = f.area();
                let list = Rect {
                    width: (width as u16 + 2).min(area.width),
                    ..area
                };
                let shown: Vec<Line> = lines
                    .iter()
                    .skip(scroll)
                    .take(list.height as usize - 2)
                    .cloned()
                    .collect();
                f.render_widget(
                    Paragraph::new(shown).block(Block::bordered().title(format!(
                        " w={width} scroll={scroll} popup={popup} ready={ready} blank={blank} "
                    ))),
                    list,
                );
                if popup {
                    let p = Rect {
                        x: 6,
                        y: 4,
                        width: 26,
                        height: 7,
                    };
                    f.render_widget(Clear, p);
                    f.render_widget(
                        Paragraph::new("popup over the list\n(close with p)")
                            .block(Block::bordered()),
                        p,
                    );
                }
            })?;
            if let Event::Key(k) = event::read()? {
                match k.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Char('j') => scroll += 1,
                    KeyCode::Char('k') => scroll = scroll.saturating_sub(1),
                    KeyCode::Char('p') => popup = !popup,
                    KeyCode::Char('[') => width = width.saturating_sub(1).max(6),
                    KeyCode::Char(']') => width += 1,
                    KeyCode::Char('b') => blank = !blank,
                    KeyCode::Char('u') => ready = !ready,
                    _ => {}
                }
            }
        }
    })();
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    result
}
