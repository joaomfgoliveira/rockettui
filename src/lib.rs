//! rockettui: a modal terminal client for Rocket.Chat. This crate holds the headless core
//! (Elm/TEA: one `App`, one `Action` enum, one reducer) and the terminal layer above it.

pub mod action;
pub mod api;
pub mod app;
pub mod config;
pub mod data;
pub mod effect;
pub mod event;
pub mod input;
pub mod tui;
pub mod ui;

use crate::action::Action;
use crate::app::update::update;
use crate::app::{App, ConnState};
use crate::effect::Effect;

/// Capacity of the one bounded Action channel.
pub const CHANNEL_CAPACITY: usize = 256;

/// Show the config errors and wait for a key press (D-02). Reads crossterm events directly.
///
/// ARCHITECTURE EXCEPTION, accepted by the author on 2026-11-16: the CLAUDE.md constraint says
/// one `App`, one `Action` enum, one channel, one reducer. This screen runs only when
/// `config::load()` fails, before any `App` exists, so there is no reducer to route input
/// through. It holds only the `Report`, redraws on resize, returns on a key press, and is never
/// reached once the config loads: from then on all input goes through the channel and the
/// reducer as before. It adds no Action variant, channel or App state.
pub async fn show_config_errors(
    mut terminal: ratatui::DefaultTerminal,
    report: &config::Report,
) -> std::io::Result<()> {
    use crossterm::event::{Event, EventStream};
    use futures::StreamExt;
    terminal.draw(|f| ui::error_screen::render(f, report))?;
    let mut events = EventStream::new();
    while let Some(next) = events.next().await {
        match next {
            Ok(event) if ui::error_screen::quits_on(&event) => return Ok(()),
            Ok(Event::Resize(..)) => {
                terminal.draw(|f| ui::error_screen::render(f, report))?;
            }
            Ok(_) => {}
            Err(_) => return Ok(()),
        }
    }
    Ok(())
}

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

/// The per-process prefix of every client message id: `rt` followed by 32 lowercase hex digits.
/// The only place the program reads OS randomness (through `RandomState`'s per-process keys). Ids
/// need to be unique across runs, not secret, and the server accepts any string as an `_id`
/// (RESEARCH Pattern 2, probe P1). Drawn once in `run`, never in the reducer.
fn session_id_prefix() -> String {
    use std::hash::BuildHasher;
    // Every `RandomState::new()` carries its own random keys, so each hash is an independent draw.
    let high = std::hash::RandomState::new().hash_one(0u8);
    let low = std::hash::RandomState::new().hash_one(1u8);
    format!("rt{high:016x}{low:016x}")
}

/// The run loop: queue input, then repeatedly apply a batch of Actions through the single reducer
/// call site, spawn the fetches the reducer asked for, draw once, and wait for the next Action.
///
/// There is no fake data: the first frame shows an empty sidebar and `Connecting…` (D-05), and
/// the rooms arrive from the server as `Action`s sent by the effect tasks (the D-13 seam).
pub async fn run(
    mut terminal: ratatui::DefaultTerminal,
    config: config::Config,
) -> std::io::Result<()> {
    // The one channel. `tx` stays alive for the whole run: effect tasks need a sender, and the
    // input task ends the loop with `Action::Quit` when its event stream ends.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Action>(CHANNEL_CAPACITY);
    event::spawn_input_task(tx.clone());

    // The client is built before `App::with_config` consumes the config, so the token reaches
    // `api::Client` and never `App`. A build failure is reported by the first effect.
    let client: Result<api::Client, api::ApiError> = match config.connection.as_ref() {
        Some(connection) => api::Client::new(connection),
        None => Err(api::ApiError::Client),
    };
    let mut app = App::with_config(terminal.size()?, config);
    // The binary shows local time (RESEARCH A2, confirmed by the author on 2026-11-16). `App::new`
    // and the tests stay on UTC, and the reducer still never reads a clock or a zone.
    app.zone = ui::timefmt::Zone::Local;
    // One prefix per process, so a restarted client never reuses a message id (D-07).
    app.id_prefix = session_id_prefix();
    app.conn = ConnState::Connecting;
    app.effects.push(Effect::LoadRooms);
    // The own username, once per connection (D-17); `:reconnect` asks again, a send never does.
    app.effects.push(Effect::LoadMe);
    let mut batch: Vec<Action> = Vec::new();
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
        for effect in app.effects.drain(..) {
            effect::spawn(client.clone(), tx.clone(), effect);
        }
        if saw_key {
            key_deadline = app
                .keys_pending()
                .then(|| tokio::time::Instant::now() + app.key_timeout());
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
mod redaction;
#[cfg(test)]
pub(crate) mod test_util;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_prefix_shape() {
        let prefix = session_id_prefix();
        assert_eq!(prefix.len(), 34, "{prefix}");
        assert!(prefix.starts_with("rt"), "{prefix}");
        assert!(
            prefix[2..]
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "{prefix}"
        );
        assert_ne!(
            prefix,
            session_id_prefix(),
            "two runs must not share a prefix"
        );
    }
}
