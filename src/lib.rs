//! rockettui: a modal terminal client for Rocket.Chat. This crate holds the headless core
//! (Elm/TEA: one `App`, one `Action` enum, one reducer) and the terminal layer above it.

pub mod action;
pub mod app;
pub mod config;
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

/// How long a count or key sequence may wait for its next key (~1 s, D-06; configurable in Phase 2).
pub const KEY_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1000);

/// Sleep until `deadline`, or never when there is none.
async fn sleep_until_opt(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending::<()>().await,
    }
}

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
pub async fn run(
    mut terminal: ratatui::DefaultTerminal,
    config: config::Config,
) -> std::io::Result<()> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Action>(CHANNEL_CAPACITY);
    // The D-13 seam: Phase 3 sends the same Actions from REST tasks.
    for action in data::fake::seed() {
        if tx.send(action).await.is_err() {
            break;
        }
    }
    event::spawn_input_task(tx.clone());
    drop(tx);

    let mut app = App::with_config(terminal.size()?, config);
    // Drain the seed before the first draw so the first frame already lists the rooms.
    let mut batch: Vec<Action> = Vec::new();
    while let Ok(action) = rx.try_recv() {
        batch.push(action);
    }
    // Armed only after a key leaves a count or sequence pending; the reducer never sees a clock.
    let mut key_deadline: Option<tokio::time::Instant> = None;
    loop {
        let mut saw_key = false;
        for action in std::iter::once(Action::Clock(unix_now())).chain(batch.drain(..)) {
            saw_key |= matches!(action, Action::Key(_) | Action::KeyTimeout);
            update(&mut app, action);
            if app.should_quit {
                break;
            }
        }
        if app.should_quit {
            break;
        }
        if saw_key {
            key_deadline = app
                .keys_pending()
                .then(|| tokio::time::Instant::now() + KEY_TIMEOUT);
        }
        terminal.draw(|f| ui::view(f, &app))?;
        // Debug-only hook so the pty smoke harness and the manual UAT can prove that a panic
        // restores the terminal. Compiled out of release builds (A17).
        #[cfg(debug_assertions)]
        if std::env::var_os("ROCKETTUI_DEBUG_PANIC").is_some() {
            panic!("ROCKETTUI_DEBUG_PANIC: deliberate panic to test terminal restore");
        }
        tokio::select! {
            received = rx.recv() => match received {
                Some(action) => batch.push(action),
                None => break,
            },
            () = sleep_until_opt(key_deadline) => batch.push(Action::KeyTimeout),
        }
        while let Ok(action) = rx.try_recv() {
            batch.push(action);
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod test_util;
