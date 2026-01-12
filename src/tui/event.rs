use std::time::Duration;

use crossterm::event::{self, Event, KeyEvent};
use tokio::sync::mpsc;

use crate::agent::AgentEvent;

/// Events that drive the TUI application
#[derive(Debug)]
pub enum TuiEvent {
    /// Terminal input event (key press)
    Input(KeyEvent),

    /// Periodic tick for animations (spinners)
    Tick,

    /// Event from agent execution
    Agent(AgentEvent),

    /// Terminal resize
    Resize { width: u16, height: u16 },
}

/// Handles event polling and aggregation from multiple sources
pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<TuiEvent>,
    tx: mpsc::UnboundedSender<TuiEvent>,
}

impl EventHandler {
    /// Create a new event handler that polls terminal input and generates ticks
    ///
    /// # Errors
    /// Returns error if event polling thread fails to spawn
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let event_tx = tx.clone();

        // Spawn terminal event polling task
        std::thread::spawn(move || {
            loop {
                // Poll for terminal events with timeout
                if event::poll(tick_rate).unwrap_or(false) {
                    if let Ok(evt) = event::read() {
                        let tui_event = match evt {
                            Event::Key(key) => TuiEvent::Input(key),
                            Event::Resize(w, h) => TuiEvent::Resize {
                                width: w,
                                height: h,
                            },
                            _ => continue,
                        };
                        if event_tx.send(tui_event).is_err() {
                            break;
                        }
                    }
                } else {
                    // Timeout - send tick for animations
                    if event_tx.send(TuiEvent::Tick).is_err() {
                        break;
                    }
                }
            }
        });

        Self { rx, tx }
    }

    /// Get the sender for forwarding agent events
    pub fn agent_event_sender(&self) -> mpsc::UnboundedSender<TuiEvent> {
        self.tx.clone()
    }

    /// Wait for the next event
    pub async fn next(&mut self) -> Option<TuiEvent> {
        self.rx.recv().await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_tui_event_debug() {
        let event = TuiEvent::Tick;
        assert!(format!("{event:?}").contains("Tick"));
    }
}
