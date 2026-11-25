//! Terminal lifecycle: raw mode, alternate screen, bracketed paste, and restore on every exit
//! path including panics.

use crate::app::emoji::Graphics;
use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use crossterm::execute;
use ratatui::DefaultTerminal;
use std::io::{IsTerminal, stdout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Set once any image was transmitted to the terminal, so that `restore` knows to delete them.
static IMAGES_SENT: AtomicBool = AtomicBool::new(false);

/// Enter raw mode and the alternate screen, enable bracketed paste, and layer a panic hook that
/// turns bracketed paste off before the hook `ratatui::init` installed restores the screen.
pub fn init() -> DefaultTerminal {
    let terminal = ratatui::init();
    let _ = execute!(stdout(), EnableBracketedPaste);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        delete_images();
        let _ = execute!(stdout(), DisableBracketedPaste);
        previous(info);
    }));
    terminal
}

/// Leave bracketed paste, the alternate screen and raw mode. Safe to call more than once.
pub fn restore() {
    delete_images();
    let _ = execute!(stdout(), DisableBracketedPaste);
    ratatui::restore();
}

/// Delete every image this client transmitted, if it transmitted any.
fn delete_images() {
    use std::io::Write;
    if IMAGES_SENT.swap(false, Ordering::SeqCst) {
        let mut out = stdout();
        let _ = out
            .write_all(crate::kitty::DELETE_ALL.as_bytes())
            .and_then(|()| out.flush());
    }
}

/// Transmit to the terminal the emoji images that arrived since the last call, once each, before
/// the next frame draws the cells that show them. Nothing is written unless the terminal answered
/// the startup query with yes.
pub fn upload_images(app: &mut crate::app::App) {
    use std::io::Write;
    if app.graphics != Graphics::Supported {
        return;
    }
    let uploads = app.images.take_uploads();
    if uploads.is_empty() {
        return;
    }
    let mut out = stdout().lock();
    for frame in uploads {
        IMAGES_SENT.store(true, Ordering::SeqCst);
        let command = crate::kitty::upload(frame.id, frame.width, frame.height, &frame.pixels);
        let _ = out.write_all(command.as_bytes());
    }
    let _ = out.flush();
}

/// How long to wait for the terminal's answer to the graphics query. A terminal that understands
/// it answers in a few milliseconds; one that does not is bounded by the device-attributes request
/// sent right after it, and a silent one by this limit.
const QUERY_TIMEOUT: Duration = Duration::from_millis(250);

/// The kitty graphics query (a one-pixel image, `a=q`: only ask whether it could be shown, nothing
/// is stored or drawn) followed by a device-attributes request. Every terminal answers the latter,
/// so its reply marks the end of the wait; a graphics reply, if there is one, comes first.
const QUERY: &[u8] = b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[c";

/// True when the environment says the terminal may be kitty or one that implements its graphics
/// protocol with Unicode placeholders, and no multiplexer sits in between (tmux and screen need
/// passthrough that this client does not ask for). Only then is the query sent at all, so every
/// other terminal receives nothing it could misread.
fn may_support_graphics(env: impl Fn(&str) -> Option<String>) -> bool {
    if env("TMUX").is_some() || env("STY").is_some() {
        return false;
    }
    let term = env("TERM").unwrap_or_default();
    env("KITTY_WINDOW_ID").is_some()
        || term.contains("kitty")
        || env("TERM_PROGRAM").as_deref() == Some("ghostty")
}

/// What the terminal's answer to `QUERY` says: `Supported` only when a graphics reply that says
/// `OK` for our query id arrived. An error reply, no graphics reply (only the device attributes),
/// or nothing at all is `Unsupported`.
pub fn graphics_from_reply(reply: &[u8]) -> Graphics {
    let text = String::from_utf8_lossy(reply);
    let ok = text
        .split("\x1b_G")
        .skip(1)
        .filter_map(|apc| apc.split("\x1b\\").next())
        .any(|body| body.starts_with("i=31;OK"));
    if ok {
        Graphics::Supported
    } else {
        Graphics::Unsupported
    }
}

/// True once `reply` holds the device-attributes answer (`ESC [ ? ... c`), the end of the wait.
fn reply_complete(reply: &[u8]) -> bool {
    reply
        .windows(3)
        .position(|w| w == b"\x1b[?")
        .is_some_and(|at| reply[at..].contains(&b'c'))
}

/// Ask the terminal whether it can show images. Call after raw mode is on and before anything else
/// reads the input. Never fails and never blocks longer than `QUERY_TIMEOUT`: not a terminal, an
/// unlikely one, no answer or a refusal all give `Unsupported`.
pub fn detect_graphics() -> Graphics {
    use std::io::Write;
    if !stdout().is_terminal() || !std::io::stdin().is_terminal() {
        return Graphics::Unsupported;
    }
    if !may_support_graphics(|key| std::env::var(key).ok()) {
        return Graphics::Unsupported;
    }
    let mut out = stdout();
    if out.write_all(QUERY).and_then(|()| out.flush()).is_err() {
        return Graphics::Unsupported;
    }
    let deadline = Instant::now() + QUERY_TIMEOUT;
    let mut reply: Vec<u8> = Vec::new();
    while !reply_complete(&reply) {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || reply.len() > 4096 {
            break;
        }
        match read_stdin(left) {
            Some(bytes) => reply.extend_from_slice(&bytes),
            None => break,
        }
    }
    graphics_from_reply(&reply)
}

/// Wait up to `wait` for input on stdin and read what is there. `None` on timeout or error.
fn read_stdin(wait: Duration) -> Option<Vec<u8>> {
    let mut fd = libc::pollfd {
        fd: 0,
        events: libc::POLLIN,
        revents: 0,
    };
    let millis = i32::try_from(wait.as_millis()).unwrap_or(i32::MAX);
    // SAFETY: `fd` is one valid `pollfd`, and the count passed is 1.
    let ready = unsafe { libc::poll(&mut fd, 1, millis) };
    if ready <= 0 {
        return None;
    }
    let mut buf = [0u8; 256];
    // SAFETY: `buf` is valid for `buf.len()` bytes for the whole call.
    let n = unsafe { libc::read(0, buf.as_mut_ptr().cast(), buf.len()) };
    usize::try_from(n)
        .ok()
        .filter(|n| *n > 0)
        .map(|n| buf[..n].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn a_graphics_ok_reply_means_supported() {
        let reply = b"\x1b_Gi=31;OK\x1b\\\x1b[?62;c";
        assert_eq!(graphics_from_reply(reply), Graphics::Supported);
        // The device-attributes answer may arrive in several reads.
        assert!(!reply_complete(b"\x1b_Gi=31;OK\x1b\\"));
        assert!(reply_complete(reply));
    }

    #[test]
    fn a_refusal_no_graphics_reply_or_silence_means_unsupported() {
        assert_eq!(
            graphics_from_reply(b"\x1b_Gi=31;ENOTSUPPORTED:no\x1b\\\x1b[?62;c"),
            Graphics::Unsupported
        );
        assert_eq!(graphics_from_reply(b"\x1b[?62;c"), Graphics::Unsupported);
        assert_eq!(graphics_from_reply(b""), Graphics::Unsupported);
        // Another query id is not our answer.
        assert_eq!(
            graphics_from_reply(b"\x1b_Gi=7;OK\x1b\\"),
            Graphics::Unsupported
        );
        assert_eq!(
            graphics_from_reply(&[0xff, 0xfe, 0x1b]),
            Graphics::Unsupported
        );
    }

    #[test]
    fn only_a_kitty_like_terminal_outside_a_multiplexer_is_asked() {
        assert!(may_support_graphics(env_of(&[("TERM", "xterm-kitty")])));
        assert!(may_support_graphics(env_of(&[("KITTY_WINDOW_ID", "1")])));
        assert!(may_support_graphics(env_of(&[("TERM_PROGRAM", "ghostty")])));
        assert!(!may_support_graphics(env_of(&[("TERM", "xterm-256color")])));
        assert!(!may_support_graphics(env_of(&[])));
        assert!(!may_support_graphics(env_of(&[
            ("TERM", "xterm-kitty"),
            ("TMUX", "/tmp/tmux-1000/default,1,0")
        ])));
        assert!(!may_support_graphics(env_of(&[
            ("TERM", "screen"),
            ("STY", "1.pts")
        ])));
    }

    #[test]
    fn the_query_asks_and_stores_nothing() {
        let text = String::from_utf8_lossy(QUERY);
        assert!(text.contains("a=q"), "a query action only: {text:?}");
        assert!(!text.contains("a=T") && !text.contains("a=t,") && !text.contains("U=1"));
    }

    #[test]
    fn without_a_terminal_nothing_is_asked() {
        // Under `cargo test` stdout is not a terminal, so this must return at once without
        // writing the query anywhere.
        let started = Instant::now();
        assert_eq!(detect_graphics(), Graphics::Unsupported);
        assert!(started.elapsed() < QUERY_TIMEOUT);
    }
}
