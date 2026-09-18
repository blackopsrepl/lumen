//! Length-prefixed JSON wire protocol shared by the Lumen service and the
//! app-side [`crate::LumenBackend`].
//!
//! The app owns a ratatui `Terminal` and writes the cell diffs ratatui hands it
//! through [`AppMessage`]; the service answers with [`ServerMessage`] input and
//! resize events. Messages are a `u32` big-endian length followed by that many
//! bytes of JSON. The service passes its configured grid to the app in a
//! [`ServerMessage::Size`] before the app draws, so both sides start from the
//! same dimensions.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

/// Largest message either side will accept, so a corrupt length prefix cannot
/// make the peer allocate without bound.
pub const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;

/// A cell color. Mirrors ratatui's `Color` on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Color {
    #[default]
    Reset,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    Gray,
    DarkGray,
    LightRed,
    LightGreen,
    LightYellow,
    LightBlue,
    LightMagenta,
    LightCyan,
    White,
    /// 24-bit true color.
    Rgb(u8, u8, u8),
    /// 8-bit 256-color palette index.
    Indexed(u8),
}

/// Style modifier bits. The values match ratatui's `Modifier` bitflags so the
/// backend can map them directly; they are protocol, not ABI.
pub mod modifier {
    pub const BOLD: u16 = 1 << 0;
    pub const DIM: u16 = 1 << 1;
    pub const ITALIC: u16 = 1 << 2;
    pub const UNDERLINED: u16 = 1 << 3;
    pub const SLOW_BLINK: u16 = 1 << 4;
    pub const RAPID_BLINK: u16 = 1 << 5;
    pub const REVERSED: u16 = 1 << 6;
    pub const HIDDEN: u16 = 1 << 7;
    pub const CROSSED_OUT: u16 = 1 << 8;
}

/// Keyboard modifier bits carried by [`KeyEvent`] and [`MouseEvent`].
pub mod key_mod {
    pub const SHIFT: u8 = 1 << 0;
    pub const CONTROL: u8 = 1 << 1;
    pub const ALT: u8 = 1 << 2;
}

/// Where the terminal cursor is, and whether the app wants it shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Cursor {
    pub x: u16,
    pub y: u16,
    pub visible: bool,
}

/// One changed cell. `symbol` may be a multi-codepoint grapheme; ratatui's
/// buffer diff never emits the continuation cells of a wide glyph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellOp {
    pub x: u16,
    pub y: u16,
    pub symbol: String,
    #[serde(default)]
    pub fg: Color,
    #[serde(default)]
    pub bg: Color,
    #[serde(default)]
    pub mods: u16,
}

/// Messages the app sends to the service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppMessage {
    /// The app confirms the grid it has adopted.
    Ready { cols: u16, rows: u16 },
    /// The app's grid changed size.
    Resize { cols: u16, rows: u16 },
    /// Reset every cell and the cursor.
    Clear,
    /// One frame's worth of changed cells plus cursor state.
    Frame { cells: Vec<CellOp>, cursor: Cursor },
    /// The app's window title, if it has one.
    Title { title: String },
}

/// A key the human pressed, in the service's cell-grid coordinate space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyEvent {
    pub code: KeyCode,
    #[serde(default)]
    pub mods: u8,
}

/// A key code independent of any backend library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyCode {
    Char(char),
    Enter,
    Esc,
    Backspace,
    Tab,
    BackTab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    Insert,
    F(u8),
}

/// A mouse event in cell coordinates (column, row), not pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MouseEvent {
    pub kind: MouseKind,
    pub col: u16,
    pub row: u16,
    #[serde(default)]
    pub button: MouseButton,
    #[serde(default)]
    pub mods: u8,
}

/// Which pointer button a [`MouseEvent`] refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    #[default]
    Left,
    Middle,
    Right,
    /// No button, for movement and scroll events.
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseKind {
    Down,
    Up,
    Drag,
    Moved,
    ScrollUp,
    ScrollDown,
}

/// Messages the service sends to the app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// The grid the service wants the app to render into. Sent once on connect
    /// and again whenever the viewer resizes.
    Size {
        cols: u16,
        rows: u16,
    },
    Key(KeyEvent),
    Mouse(MouseEvent),
    Paste {
        text: String,
    },
    Focus {
        gained: bool,
    },
    /// The session is ending; the app should exit cleanly.
    Quit,
}

/// Write one framed message and flush it.
pub fn write_message<W, T>(writer: &mut W, message: &T) -> io::Result<()>
where
    W: Write,
    T: Serialize,
{
    let body = serde_json::to_vec(message)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    if body.len() > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("message of {} bytes exceeds the limit", body.len()),
        ));
    }
    writer.write_all(&(body.len() as u32).to_be_bytes())?;
    writer.write_all(&body)?;
    writer.flush()
}

/// Read one framed message, or `None` at a clean end of stream.
///
/// A partial header is an error, not an end of stream, so a message that is cut
/// off mid-frame is never mistaken for a clean shutdown.
pub fn read_message<R, T>(reader: &mut R) -> io::Result<Option<T>>
where
    R: Read,
    T: DeserializeOwned,
{
    let mut header = [0u8; 4];
    // Read the first byte separately: zero bytes is a clean end of stream, but
    // a header that starts and then stops is a truncated frame.
    match reader.read_exact(&mut header[..1]) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(err) => return Err(err),
    }
    reader.read_exact(&mut header[1..])?;
    let len = u32::from_be_bytes(header) as usize;
    if len > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("message of {len} bytes exceeds the limit"),
        ));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip<T>(message: T)
    where
        T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let mut buffer = Vec::new();
        write_message(&mut buffer, &message).unwrap();
        let decoded: T = read_message(&mut buffer.as_slice()).unwrap().unwrap();
        assert_eq!(decoded, message);
    }

    #[test]
    fn app_messages_round_trip() {
        round_trip(AppMessage::Ready {
            cols: 120,
            rows: 40,
        });
        round_trip(AppMessage::Clear);
        round_trip(AppMessage::Resize { cols: 80, rows: 24 });
        round_trip(AppMessage::Title {
            title: "trex".into(),
        });
        round_trip(AppMessage::Frame {
            cells: vec![
                CellOp {
                    x: 1,
                    y: 2,
                    symbol: "🦖".into(),
                    fg: Color::Indexed(197),
                    bg: Color::Rgb(10, 20, 30),
                    mods: modifier::BOLD | modifier::UNDERLINED,
                },
                CellOp {
                    x: 3,
                    y: 4,
                    symbol: " ".into(),
                    fg: Color::Reset,
                    bg: Color::Reset,
                    mods: 0,
                },
            ],
            cursor: Cursor {
                x: 3,
                y: 4,
                visible: true,
            },
        });
    }

    #[test]
    fn server_messages_round_trip() {
        round_trip(ServerMessage::Size {
            cols: 100,
            rows: 30,
        });
        round_trip(ServerMessage::Quit);
        round_trip(ServerMessage::Paste { text: "hi".into() });
        round_trip(ServerMessage::Focus { gained: false });
        round_trip(ServerMessage::Key(KeyEvent {
            code: KeyCode::Up,
            mods: key_mod::CONTROL,
        }));
        round_trip(ServerMessage::Key(KeyEvent {
            code: KeyCode::Char('q'),
            mods: 0,
        }));
        round_trip(ServerMessage::Mouse(MouseEvent {
            kind: MouseKind::ScrollDown,
            col: 7,
            row: 9,
            button: MouseButton::None,
            mods: key_mod::SHIFT,
        }));
    }

    #[test]
    fn a_clean_end_of_stream_is_none() {
        let decoded: Option<AppMessage> = read_message(&mut [].as_slice()).unwrap();
        assert!(decoded.is_none());
    }

    #[test]
    fn a_truncated_header_is_an_error_not_shutdown() {
        let bytes = [0u8, 0, 0];
        let decoded: io::Result<Option<AppMessage>> = read_message(&mut bytes.as_slice());
        assert!(decoded.is_err());
    }

    #[test]
    fn an_oversized_length_is_rejected() {
        let mut bytes = (MAX_MESSAGE_BYTES as u32 + 1).to_be_bytes().to_vec();
        bytes.extend_from_slice(b"whatever");
        let decoded: io::Result<Option<AppMessage>> = read_message(&mut bytes.as_slice());
        assert!(decoded.is_err());
    }
}
