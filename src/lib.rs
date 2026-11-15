//! rockettui: a modal terminal client for Rocket.Chat. This crate holds the headless core
//! (Elm/TEA: one `App`, one `Action` enum, one reducer) and the terminal layer above it.

pub mod action;
pub mod app;
pub mod data;
pub mod event;
pub mod input;
pub mod tui;
pub mod ui;

use crate::action::Action;
use crate::app::App;
use crate::app::update::update;

/// Capacity of the one bounded Action channel.
pub const CHANNEL_CAPACITY: usize = 256;

/// Whole seconds since the Unix epoch, 0 if the clock is before it. The only wall-clock read on
/// the app path: the reducer receives time as `Action::Clock`.
fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// The run loop: seed the channel, queue input, then repeatedly apply a batch of Actions through
/// the single reducer call site, draw once, and wait for the next Action.
pub async fn run(mut terminal: ratatui::DefaultTerminal) -> std::io::Result<()> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Action>(CHANNEL_CAPACITY);
    // The D-13 seam: Phase 3 sends the same Actions from REST tasks.
    for action in data::fake::seed() {
        if tx.send(action).await.is_err() {
            break;
        }
    }
    event::spawn_input_task(tx.clone());
    drop(tx);

    let mut app = App::new(terminal.size()?);
    // Drain the seed before the first draw so the first frame already lists the rooms.
    let mut batch: Vec<Action> = Vec::new();
    while let Ok(action) = rx.try_recv() {
        batch.push(action);
    }
    loop {
        for action in std::iter::once(Action::Clock(unix_now())).chain(batch.drain(..)) {
            update(&mut app, action);
            if app.should_quit {
                break;
            }
        }
        if app.should_quit {
            break;
        }
        terminal.draw(|f| ui::view(f, &app))?;
        // Debug-only hook so the pty smoke harness and the manual UAT can prove that a panic
        // restores the terminal. Compiled out of release builds (A17).
        #[cfg(debug_assertions)]
        if std::env::var_os("ROCKETTUI_DEBUG_PANIC").is_some() {
            panic!("ROCKETTUI_DEBUG_PANIC: deliberate panic to test terminal restore");
        }
        match rx.recv().await {
            Some(action) => batch.push(action),
            None => break,
        }
        while let Ok(action) = rx.try_recv() {
            batch.push(action);
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod test_util;
