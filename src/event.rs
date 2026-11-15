//! The input task: the only producer of `Action::Key`, `Action::Resize` and `Action::Paste`.

use crate::action::Action;
use crossterm::event::{Event, EventStream, KeyEventKind};
use futures::StreamExt;
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;

/// Read terminal events and forward them as Actions until the channel closes. When the event
/// stream ends or fails, ask the app to quit.
pub fn spawn_input_task(tx: Sender<Action>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut events = EventStream::new();
        while let Some(next) = events.next().await {
            let action = match next {
                Ok(Event::Key(k)) if k.kind != KeyEventKind::Release => Action::Key(k),
                Ok(Event::Resize(w, h)) => Action::Resize(w, h),
                Ok(Event::Paste(s)) => Action::Paste(s),
                Ok(_) => continue,
                Err(_) => break,
            };
            if tx.send(action).await.is_err() {
                return;
            }
        }
        let _ = tx.send(Action::Quit).await;
    })
}
