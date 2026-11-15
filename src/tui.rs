//! Terminal lifecycle: raw mode, alternate screen, bracketed paste, and restore on every exit
//! path including panics.

use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use crossterm::execute;
use ratatui::DefaultTerminal;
use std::io::stdout;

/// Enter raw mode and the alternate screen, enable bracketed paste, and layer a panic hook that
/// turns bracketed paste off before the hook `ratatui::init` installed restores the screen.
pub fn init() -> DefaultTerminal {
    let terminal = ratatui::init();
    let _ = execute!(stdout(), EnableBracketedPaste);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(stdout(), DisableBracketedPaste);
        previous(info);
    }));
    terminal
}

/// Leave bracketed paste, the alternate screen and raw mode. Safe to call more than once.
pub fn restore() {
    let _ = execute!(stdout(), DisableBracketedPaste);
    ratatui::restore();
}
