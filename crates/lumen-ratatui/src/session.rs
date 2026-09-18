//! The app-side session: a ratatui `Terminal` on [`LumenBackend`] plus the
//! input stream coming back from the service.
//!
//! The service owns the grid's dimensions and passes them to the app before the
//! first draw. Input arrives on a reader thread and is exposed through
//! [`Session::poll`], so a normal ratatui event loop stays single-threaded.

use crate::backend::LumenBackend;
use crate::protocol::{self, AppMessage, KeyEvent, MouseEvent, ServerMessage};
use ratatui::layout::Size;
use ratatui::{Frame, Terminal};
use std::io::{self, BufReader, Stdout};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Input the service delivered to the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Paste(String),
    Focus(bool),
    /// The service is ending the session; the app should return from its loop.
    Quit,
}

/// A live connection to a Lumen terminal session.
pub struct Session {
    terminal: Terminal<LumenBackend<Stdout>>,
    events: Receiver<Event>,
    size: Arc<Mutex<Size>>,
    _reader: JoinHandle<()>,
}

impl Session {
    /// Adopt the service's grid and start the input reader.
    ///
    /// The service always sends [`ServerMessage::Size`] first, so this blocks
    /// until that arrives; the environment variables are only a fallback if the
    /// service is silent.
    pub fn connect() -> io::Result<Self> {
        let cols = env_dimension("LUMEN_COLS").unwrap_or(80);
        let rows = env_dimension("LUMEN_ROWS").unwrap_or(24);
        let size = Arc::new(Mutex::new(Size::new(cols, rows)));

        {
            let stdin = io::stdin();
            let mut locked = stdin.lock();
            if let Some(ServerMessage::Size { cols, rows }) =
                protocol::read_message::<_, ServerMessage>(&mut locked)?
            {
                set_size(&size, cols, rows);
            }
        }

        let backend = LumenBackend::new(io::stdout(), size.clone());
        let mut terminal = Terminal::new(backend)?;
        {
            let adopted = self::size(&size);
            protocol::write_message(
                &mut io::stdout(),
                &AppMessage::Ready {
                    cols: adopted.width,
                    rows: adopted.height,
                },
            )?;
        }
        terminal.hide_cursor()?;

        let (tx, events) = mpsc::channel();
        let reader_size = size.clone();
        let reader = thread::Builder::new()
            .name("lumen-ratatui-input".into())
            .spawn(move || {
                let mut input = BufReader::new(io::stdin());
                let send = |event: Event| tx.send(event).is_ok();
                loop {
                    match protocol::read_message::<_, ServerMessage>(&mut input) {
                        Ok(Some(ServerMessage::Size { cols, rows })) => {
                            set_size(&reader_size, cols, rows);
                        }
                        Ok(Some(ServerMessage::Key(key))) => {
                            if !send(Event::Key(key)) {
                                break;
                            }
                        }
                        Ok(Some(ServerMessage::Mouse(mouse))) => {
                            if !send(Event::Mouse(mouse)) {
                                break;
                            }
                        }
                        Ok(Some(ServerMessage::Paste { text })) => {
                            if !send(Event::Paste(text)) {
                                break;
                            }
                        }
                        Ok(Some(ServerMessage::Focus { gained })) => {
                            if !send(Event::Focus(gained)) {
                                break;
                            }
                        }
                        Ok(Some(ServerMessage::Quit)) => {
                            let _ = send(Event::Quit);
                            break;
                        }
                        Ok(None) | Err(_) => break,
                    }
                }
            })?;

        Ok(Self {
            terminal,
            events,
            size,
            _reader: reader,
        })
    }

    /// The grid size the app is currently rendering into.
    pub fn size(&self) -> Size {
        self::size(&self.size)
    }

    /// Direct access to the terminal, for apps that need cursor control.
    pub fn terminal_mut(&mut self) -> &mut Terminal<LumenBackend<Stdout>> {
        &mut self.terminal
    }

    /// Render one frame. ratatui diffs against the previous frame, so only
    /// changed cells reach the service.
    pub fn draw<F>(&mut self, render: F) -> io::Result<()>
    where
        F: FnOnce(&mut Frame),
    {
        self.terminal.draw(render).map(|_| ())
    }

    /// Wait up to `timeout` for input, returning `None` on a quiet tick.
    ///
    /// A disconnected service is reported as [`Event::Quit`] so the app's loop
    /// ends the same way whether the service said goodbye or vanished.
    pub fn poll(&mut self, timeout: Duration) -> Option<Event> {
        match self.events.recv_timeout(timeout) {
            Ok(event) => Some(event),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => Some(Event::Quit),
        }
    }
}

fn size(size: &Arc<Mutex<Size>>) -> Size {
    *size.lock().expect("session size mutex poisoned")
}

fn set_size(size: &Arc<Mutex<Size>>, cols: u16, rows: u16) {
    *size.lock().expect("session size mutex poisoned") = Size::new(cols.max(1), rows.max(1));
}

fn env_dimension(name: &str) -> Option<u16> {
    std::env::var(name).ok()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_dimensions_parse_or_default() {
        assert_eq!(env_dimension("LUMEN_RATATUI_TEST_UNSET"), None);
    }

    #[test]
    fn set_size_clamps_to_at_least_one() {
        let size = Arc::new(Mutex::new(Size::new(1, 1)));
        set_size(&size, 0, 0);
        assert_eq!(self::size(&size), Size::new(1, 1));
    }
}
