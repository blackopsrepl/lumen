//! Ratatui backend, event source, and wire protocol for Lumen terminal
//! sessions.
//!
//! A ratatui app links this crate, builds a [`ratatui::Terminal`] on a
//! [`LumenBackend`], and reads input from a [`Session`]. Lumen launches the app
//! with its stdin/stdout wired to the session, so no terminal, PTY, or socket is
//! involved: ratatui's own buffer diff is the transport.
//!
//! ```no_run
//! use lumen_ratatui::Session;
//! use ratatui::widgets::Paragraph;
//!
//! let mut session = Session::connect().unwrap();
//! loop {
//!     session.draw(|frame| frame.render_widget(Paragraph::new("hello"), frame.area())).unwrap();
//!     if session.poll(std::time::Duration::from_millis(250)).is_none() {
//!         continue;
//!     }
//! }
//! ```

pub mod protocol;

#[cfg(feature = "backend")]
mod backend;
#[cfg(feature = "backend")]
mod session;

#[cfg(feature = "backend")]
pub use backend::LumenBackend;
#[cfg(feature = "backend")]
pub use session::{Event, Session};
