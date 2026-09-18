//! A ratatui [`Backend`] that writes cell diffs to Lumen over stdout.
//!
//! ratatui's `Terminal` already diffs its two buffers, so [`Backend::draw`]
//! receives exactly the cells that changed. This backend buffers those cells
//! and emits them as one [`AppMessage::Frame`] in [`Backend::flush`], which
//! ratatui calls once per `Terminal::draw`.

use crate::protocol::{self, modifier, AppMessage, CellOp, Color, Cursor};
use ratatui::backend::{Backend, ClearType, WindowSize};
use ratatui::buffer::{Cell, CellDiffOption};
use ratatui::layout::{Position, Size};
use ratatui::style::{Color as RatColor, Modifier};
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

/// A ratatui backend that streams cell diffs to Lumen.
pub struct LumenBackend<W: Write> {
    writer: W,
    /// Shared with the session's input thread: a `ServerMessage::Size` updates
    /// it, and the next `Terminal::draw` autoresizes to match.
    size: Arc<Mutex<Size>>,
    pending: Vec<CellOp>,
    cursor: Cursor,
    last_cursor: Cursor,
}

impl<W: Write> LumenBackend<W> {
    /// Build a backend that writes to `writer` and reports `size`.
    pub fn new(writer: W, size: Arc<Mutex<Size>>) -> Self {
        Self {
            writer,
            size,
            pending: Vec::new(),
            cursor: Cursor::default(),
            last_cursor: Cursor::default(),
        }
    }

    /// Build a backend with its own fixed size, for tests and simple apps.
    pub fn with_size(writer: W, cols: u16, rows: u16) -> Self {
        Self::new(writer, Arc::new(Mutex::new(Size::new(cols, rows))))
    }

    /// The underlying writer, so callers can inspect what was emitted.
    pub fn writer(&self) -> &W {
        &self.writer
    }

    /// The size this backend currently reports.
    pub fn size_handle(&self) -> Arc<Mutex<Size>> {
        self.size.clone()
    }

    fn emit(&mut self, message: &AppMessage) -> io::Result<()> {
        protocol::write_message(&mut self.writer, message)
    }
}

impl<W: Write> Backend for LumenBackend<W> {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        for (x, y, cell) in content {
            if cell.diff_option == CellDiffOption::Skip {
                continue;
            }
            self.pending.push(CellOp {
                x,
                y,
                symbol: cell.symbol().to_string(),
                fg: color(cell.fg),
                bg: color(cell.bg),
                mods: modifiers(cell.modifier),
            });
        }
        Ok(())
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        self.cursor.visible = false;
        Ok(())
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        self.cursor.visible = true;
        Ok(())
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        Ok(Position {
            x: self.cursor.x,
            y: self.cursor.y,
        })
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        let position = position.into();
        self.cursor.x = position.x;
        self.cursor.y = position.y;
        Ok(())
    }

    fn clear(&mut self) -> io::Result<()> {
        self.emit(&AppMessage::Clear)?;
        self.pending.clear();
        self.cursor = Cursor::default();
        self.last_cursor = Cursor::default();
        Ok(())
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        // The app uses the fullscreen viewport, where ratatui clears `All` and
        // then resets its back buffer, so the next frame redraws every cell.
        // Other regions have no meaning for a whole-grid transport.
        if matches!(clear_type, ClearType::All) {
            self.clear()?;
        }
        Ok(())
    }

    fn size(&self) -> io::Result<Size> {
        Ok(*self.size.lock().expect("backend size mutex poisoned"))
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        let size = *self.size.lock().expect("backend size mutex poisoned");
        Ok(WindowSize {
            columns_rows: size,
            // The transport is cell-based; there is no pixel geometry.
            pixels: Size::new(0, 0),
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.pending.is_empty() && self.cursor == self.last_cursor {
            return Ok(());
        }
        let frame = AppMessage::Frame {
            cells: std::mem::take(&mut self.pending),
            cursor: self.cursor,
        };
        self.emit(&frame)?;
        self.last_cursor = self.cursor;
        Ok(())
    }

    fn append_lines(&mut self, _n: u16) -> io::Result<()> {
        // Only meaningful for inline viewports, which this transport does not use.
        Ok(())
    }
}

/// Map a ratatui color onto the wire color.
fn color(color: RatColor) -> Color {
    match color {
        RatColor::Reset => Color::Reset,
        RatColor::Black => Color::Black,
        RatColor::Red => Color::Red,
        RatColor::Green => Color::Green,
        RatColor::Yellow => Color::Yellow,
        RatColor::Blue => Color::Blue,
        RatColor::Magenta => Color::Magenta,
        RatColor::Cyan => Color::Cyan,
        RatColor::Gray => Color::Gray,
        RatColor::DarkGray => Color::DarkGray,
        RatColor::LightRed => Color::LightRed,
        RatColor::LightGreen => Color::LightGreen,
        RatColor::LightYellow => Color::LightYellow,
        RatColor::LightBlue => Color::LightBlue,
        RatColor::LightMagenta => Color::LightMagenta,
        RatColor::LightCyan => Color::LightCyan,
        RatColor::White => Color::White,
        RatColor::Rgb(r, g, b) => Color::Rgb(r, g, b),
        RatColor::Indexed(index) => Color::Indexed(index),
    }
}

/// Pack ratatui modifiers into the protocol bitmask.
fn modifiers(modifier: Modifier) -> u16 {
    let mut bits = 0;
    for (flag, bit) in [
        (Modifier::BOLD, modifier::BOLD),
        (Modifier::DIM, modifier::DIM),
        (Modifier::ITALIC, modifier::ITALIC),
        (Modifier::UNDERLINED, modifier::UNDERLINED),
        (Modifier::SLOW_BLINK, modifier::SLOW_BLINK),
        (Modifier::RAPID_BLINK, modifier::RAPID_BLINK),
        (Modifier::REVERSED, modifier::REVERSED),
        (Modifier::HIDDEN, modifier::HIDDEN),
        (Modifier::CROSSED_OUT, modifier::CROSSED_OUT),
    ] {
        if modifier.contains(flag) {
            bits |= bit;
        }
    }
    bits
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Stylize;
    use ratatui::widgets::Paragraph;
    use ratatui::Terminal;

    /// Drain the framed messages a terminal has written so far.
    fn messages(bytes: &[u8]) -> Vec<AppMessage> {
        let mut reader = bytes;
        let mut out = Vec::new();
        while let Some(message) = protocol::read_message(&mut reader).unwrap() {
            out.push(message);
        }
        out
    }

    #[test]
    fn renders_a_widget_as_cell_diffs() {
        let backend = LumenBackend::with_size(Vec::new(), 20, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| frame.render_widget(Paragraph::new("hi").bold(), frame.area()))
            .unwrap();

        let bytes = terminal.backend_mut().writer().clone();
        let frames: Vec<_> = messages(&bytes)
            .into_iter()
            .filter_map(|message| match message {
                AppMessage::Frame { cells, .. } => Some(cells),
                _ => None,
            })
            .collect();
        let cells = frames.last().expect("a frame was emitted");
        let first = cells
            .iter()
            .find(|cell| cell.x == 0 && cell.y == 0)
            .unwrap();
        assert_eq!(first.symbol, "h");
        assert_eq!(first.mods & modifier::BOLD, modifier::BOLD);
        let second = cells
            .iter()
            .find(|cell| cell.x == 1 && cell.y == 0)
            .unwrap();
        assert_eq!(second.symbol, "i");
    }

    #[test]
    fn a_static_redraw_emits_nothing() {
        let backend = LumenBackend::with_size(Vec::new(), 20, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        let render = |frame: &mut ratatui::Frame| {
            frame.render_widget(Paragraph::new("same"), frame.area());
        };
        terminal.draw(render).unwrap();
        let after_first = terminal.backend_mut().writer().len();
        terminal.draw(render).unwrap();
        let after_second = terminal.backend_mut().writer().len();
        assert_eq!(
            after_first, after_second,
            "an identical redraw must not emit a frame"
        );
    }

    #[test]
    fn clear_is_emitted_for_a_full_clear() {
        let backend = LumenBackend::with_size(Vec::new(), 4, 2);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.clear().unwrap();
        let bytes = terminal.backend_mut().writer().clone();
        assert!(messages(&bytes)
            .iter()
            .any(|message| matches!(message, AppMessage::Clear)));
    }

    #[test]
    fn colors_and_modifiers_map_to_the_protocol() {
        assert_eq!(color(RatColor::Rgb(1, 2, 3)), Color::Rgb(1, 2, 3));
        assert_eq!(color(RatColor::Indexed(42)), Color::Indexed(42));
        assert_eq!(
            modifiers(Modifier::ITALIC | Modifier::CROSSED_OUT),
            modifier::ITALIC | modifier::CROSSED_OUT
        );
    }
}
