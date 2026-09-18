//! Ratatui terminal sessions.
//!
//! Lumen launches a ratatui app that links `lumen-ratatui` and speaks the
//! framed protocol in [`lumen_ratatui::protocol`] over its stdin/stdout. The app
//! owns rendering; ratatui's buffer diff means Lumen only ever receives changed
//! cells. Lumen applies them to an authoritative [`Screen`] and fans a full
//! snapshot out to viewers, so a viewer that connects late — or falls behind —
//! always sees a consistent grid.
//!
//! No PTY is involved. The grid is the transport, which is why the agent can
//! read it as text and the viewer can render it as cells rather than pixels.

use crate::supervisor::terminate_process_group;
use anyhow::{Context, Result};
use bytes::Bytes;
use lumen_ratatui::protocol::{
    self, AppMessage, CellOp, Color, Cursor, KeyCode, MouseButton, MouseEvent, MouseKind,
    ServerMessage,
};
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, watch, Mutex as AsyncMutex};

/// One cell of the authoritative grid.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Cell {
    symbol: String,
    fg: Color,
    bg: Color,
    mods: u16,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            symbol: " ".into(),
            fg: Color::Reset,
            bg: Color::Reset,
            mods: 0,
        }
    }
}

impl Cell {
    /// A cell that carries no visible information, so snapshots can omit it.
    fn is_blank(&self) -> bool {
        self.symbol == " " && self.fg == Color::Reset && self.bg == Color::Reset && self.mods == 0
    }
}

/// The authoritative terminal grid.
#[derive(Clone, Debug)]
pub struct Screen {
    cols: u16,
    rows: u16,
    cells: Vec<Cell>,
    cursor: Cursor,
}

impl Screen {
    /// A blank grid of the given size.
    pub fn new(cols: u16, rows: u16) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        Self {
            cols,
            rows,
            cells: vec![Cell::default(); cols as usize * rows as usize],
            cursor: Cursor::default(),
        }
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }

    pub fn rows(&self) -> u16 {
        self.rows
    }

    /// Apply one changed cell. Out-of-bounds cells are ignored so a misbehaving
    /// app cannot grow the grid behind Lumen's back.
    pub fn apply(&mut self, op: &CellOp) {
        if op.x >= self.cols || op.y >= self.rows {
            return;
        }
        let index = op.y as usize * self.cols as usize + op.x as usize;
        self.cells[index] = Cell {
            symbol: op.symbol.clone(),
            fg: op.fg,
            bg: op.bg,
            mods: op.mods,
        };
    }

    pub fn set_cursor(&mut self, cursor: Cursor) {
        self.cursor = cursor;
    }

    /// Reset every cell and the cursor.
    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            *cell = Cell::default();
        }
        self.cursor = Cursor::default();
    }

    /// Resize to a new grid, clearing it: neither side has meaningful content to
    /// reflow, and the app redraws in full after a resize.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        *self = Self::new(cols, rows);
    }

    /// The visible grid as plain text, one line per row with trailing blanks
    /// trimmed, so an agent can read the screen without a terminal emulator.
    pub fn text(&self) -> String {
        let mut out = String::with_capacity(self.rows as usize * (self.cols as usize + 1));
        for row in 0..self.rows as usize {
            let start = row * self.cols as usize;
            let mut line = String::with_capacity(self.cols as usize);
            for cell in &self.cells[start..start + self.cols as usize] {
                line.push_str(&cell.symbol);
            }
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out
    }

    /// A serializable snapshot of the non-blank cells plus cursor state.
    ///
    /// A snapshot is idempotent, which is what lets a viewer recover from a
    /// dropped frame by simply receiving the next one.
    pub fn snapshot(&self) -> Bytes {
        #[derive(Serialize)]
        struct CellWire<'a> {
            x: u16,
            y: u16,
            s: &'a str,
            fg: Color,
            bg: Color,
            m: u16,
        }
        #[derive(Serialize)]
        struct Snapshot<'a> {
            cols: u16,
            rows: u16,
            cx: u16,
            cy: u16,
            cv: bool,
            cells: Vec<CellWire<'a>>,
        }

        let mut cells = Vec::new();
        for y in 0..self.rows {
            for x in 0..self.cols {
                let index = y as usize * self.cols as usize + x as usize;
                let cell = &self.cells[index];
                if cell.is_blank() {
                    continue;
                }
                cells.push(CellWire {
                    x,
                    y,
                    s: &cell.symbol,
                    fg: cell.fg,
                    bg: cell.bg,
                    m: cell.mods,
                });
            }
        }
        let snapshot = Snapshot {
            cols: self.cols,
            rows: self.rows,
            cx: self.cursor.x,
            cy: self.cursor.y,
            cv: self.cursor.visible,
            cells,
        };
        Bytes::from(serde_json::to_vec(&snapshot).expect("a screen snapshot serializes"))
    }
}

/// The shared grid and its frame channel, owned separately from the process so
/// the reader task never needs a handle back to the session.
struct Mirror {
    screen: Mutex<Screen>,
    frames: watch::Sender<Option<Bytes>>,
    streaming: AtomicBool,
}

impl Mirror {
    fn new(screen: Screen) -> Self {
        let (frames, _) = watch::channel(None);
        Self {
            screen: Mutex::new(screen),
            frames,
            streaming: AtomicBool::new(false),
        }
    }

    fn text(&self) -> String {
        self.screen.lock().expect("screen mutex poisoned").text()
    }

    fn snapshot(&self) -> Bytes {
        self.screen
            .lock()
            .expect("screen mutex poisoned")
            .snapshot()
    }

    fn set_streaming(&self, streaming: bool) {
        self.streaming.store(streaming, Ordering::SeqCst);
        if streaming {
            // Publish immediately so a viewer that attaches to an otherwise
            // static screen still sees it.
            self.publish();
        }
    }

    fn publish(&self) {
        let snapshot = self.snapshot();
        self.frames.send_replace(Some(snapshot));
    }

    fn apply_frame(&self, cells: &[CellOp], cursor: Cursor) {
        let snapshot = {
            let mut screen = self.screen.lock().expect("screen mutex poisoned");
            for op in cells {
                screen.apply(op);
            }
            screen.set_cursor(cursor);
            screen.snapshot()
        };
        if self.streaming.load(Ordering::SeqCst) {
            self.frames.send_replace(Some(snapshot));
        }
    }

    fn apply_clear(&self) {
        let snapshot = {
            let mut screen = self.screen.lock().expect("screen mutex poisoned");
            screen.clear();
            screen.snapshot()
        };
        if self.streaming.load(Ordering::SeqCst) {
            self.frames.send_replace(Some(snapshot));
        }
    }

    fn apply_resize(&self, cols: u16, rows: u16) {
        let snapshot = {
            let mut screen = self.screen.lock().expect("screen mutex poisoned");
            screen.resize(cols, rows);
            screen.snapshot()
        };
        if self.streaming.load(Ordering::SeqCst) {
            self.frames.send_replace(Some(snapshot));
        }
    }
}

/// A running ratatui app and the grid it is rendering into.
pub struct RatatuiSession {
    child: AsyncMutex<Child>,
    outgoing: mpsc::UnboundedSender<ServerMessage>,
    mirror: Arc<Mirror>,
}

impl RatatuiSession {
    /// Launch an app and begin mirroring its grid.
    ///
    /// The initial size is sent before the app's first draw, so both sides agree
    /// on the grid even if the environment disagrees.
    pub async fn launch(bin: &Path, profile: &Path, cols: u16, rows: u16) -> Result<Arc<Self>> {
        let cols = cols.max(1);
        let rows = rows.max(1);
        let mut child = Command::new(bin)
            .env("LUMEN_COLS", cols.to_string())
            .env("LUMEN_ROWS", rows.to_string())
            .current_dir(profile)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            // Its own process group, so teardown reaches everything it spawned.
            .process_group(0)
            .spawn()
            .with_context(|| format!("spawning ratatui app {}", bin.display()))?;

        let stdin = child.stdin.take().context("ratatui stdin unavailable")?;
        let stdout = child.stdout.take().context("ratatui stdout unavailable")?;
        let stderr = child.stderr.take().context("ratatui stderr unavailable")?;

        let mirror = Arc::new(Mirror::new(Screen::new(cols, rows)));
        let (outgoing, outgoing_rx) = mpsc::unbounded_channel();

        let session = Arc::new(Self {
            child: AsyncMutex::new(child),
            outgoing,
            mirror: mirror.clone(),
        });

        // Kick off the pipes before the initial size, so a chatty app cannot
        // fill a pipe while the service is still starting up.
        tokio::spawn(write_loop(stdin, outgoing_rx));
        tokio::spawn(read_loop(mirror, stdout));
        tokio::spawn(stderr_loop(bin.to_path_buf(), stderr));

        // Send the initial grid before the app can draw.
        session.send(&ServerMessage::Size { cols, rows })?;

        Ok(session)
    }

    /// Join the frame stream. The receiver stores only the newest snapshot.
    pub fn frames(&self) -> watch::Receiver<Option<Bytes>> {
        self.mirror.frames.subscribe()
    }

    /// Start or stop publishing snapshots, mirroring the desktop path so
    /// capture only runs while a viewer is attached.
    pub fn set_streaming(&self, streaming: bool) -> Result<()> {
        self.mirror.set_streaming(streaming);
        Ok(())
    }

    /// The authoritative grid as plain text.
    pub fn screen_text(&self) -> String {
        self.mirror.text()
    }

    /// A snapshot of the grid for a newly attached viewer.
    pub fn snapshot(&self) -> Bytes {
        self.mirror.snapshot()
    }

    /// Type text: one key per character, with newline mapped to Enter.
    pub fn text(&self, text: &str) -> Result<()> {
        for ch in text.chars() {
            self.key(key_code_for(ch), 0)?;
        }
        Ok(())
    }

    /// Deliver a key press.
    pub fn key(&self, code: KeyCode, mods: u8) -> Result<()> {
        self.send(&ServerMessage::Key(protocol::KeyEvent { code, mods }))
    }

    /// Deliver a mouse event in cell coordinates.
    pub fn mouse(
        &self,
        kind: MouseKind,
        col: u16,
        row: u16,
        button: MouseButton,
        mods: u8,
    ) -> Result<()> {
        self.send(&ServerMessage::Mouse(MouseEvent {
            kind,
            col,
            row,
            button,
            mods,
        }))
    }

    /// Whether the app process is still running.
    pub async fn is_alive(&self) -> bool {
        let mut child = self.child.lock().await;
        matches!(child.try_wait(), Ok(None))
    }

    /// End the session: ask the app to exit, then signal its process group.
    pub async fn shutdown(&self) {
        let _ = self.send(&ServerMessage::Quit);
        let mut child = self.child.lock().await;
        terminate_process_group(&mut child).await;
    }

    fn send(&self, message: &ServerMessage) -> Result<()> {
        self.outgoing
            .send(message.clone())
            .map_err(|_| anyhow::anyhow!("ratatui app is no longer accepting input"))
    }
}

/// Own the app's stdin: writes are serialized here so a control-plane handler
/// never blocks on a pipe.
async fn write_loop(mut stdin: ChildStdin, mut outgoing: mpsc::UnboundedReceiver<ServerMessage>) {
    while let Some(message) = outgoing.recv().await {
        if write_message(&mut stdin, &message).await.is_err() {
            break;
        }
    }
    let _ = stdin.shutdown().await;
}

/// Read the app's frames until it exits.
async fn read_loop(mirror: Arc<Mirror>, mut stdout: ChildStdout) {
    loop {
        match read_message::<_, AppMessage>(&mut stdout).await {
            Ok(Some(AppMessage::Frame { cells, cursor })) => mirror.apply_frame(&cells, cursor),
            Ok(Some(AppMessage::Clear)) => mirror.apply_clear(),
            Ok(Some(AppMessage::Resize { cols, rows })) => mirror.apply_resize(cols, rows),
            Ok(Some(AppMessage::Ready { .. })) | Ok(Some(AppMessage::Title { .. })) => {}
            Ok(None) => break,
            Err(err) => {
                tracing::warn!("ratatui app protocol error: {err}");
                break;
            }
        }
    }
}

/// Drain the app's stderr into the service log.
async fn stderr_loop(bin: std::path::PathBuf, stderr: ChildStderr) {
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        tracing::debug!(app = %bin.display(), "{line}");
    }
}

/// Framing for the service side. The app side uses the blocking helpers in
/// [`protocol`]; both write the same `u32` big-endian length plus JSON.
async fn write_message<W, T>(writer: &mut W, message: &T) -> std::io::Result<()>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let body = serde_json::to_vec(message)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    if body.len() > protocol::MAX_MESSAGE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "message exceeds the protocol limit",
        ));
    }
    writer.write_all(&(body.len() as u32).to_be_bytes()).await?;
    writer.write_all(&body).await?;
    writer.flush().await
}

async fn read_message<R, T>(reader: &mut R) -> std::io::Result<Option<T>>
where
    R: AsyncRead + Unpin,
    T: DeserializeOwned,
{
    let mut header = [0u8; 4];
    match reader.read_exact(&mut header[..1]).await {
        Ok(_) => {}
        Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(err) => return Err(err),
    }
    reader.read_exact(&mut header[1..]).await?;
    let len = u32::from_be_bytes(header) as usize;
    if len > protocol::MAX_MESSAGE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "message exceeds the protocol limit",
        ));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).await?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))
}

/// Map a typed character to the key the app receives.
fn key_code_for(ch: char) -> KeyCode {
    match ch {
        '\n' | '\r' => KeyCode::Enter,
        '\t' => KeyCode::Tab,
        other => KeyCode::Char(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumen_ratatui::protocol::modifier;

    fn op(x: u16, y: u16, symbol: &str) -> CellOp {
        CellOp {
            x,
            y,
            symbol: symbol.into(),
            fg: Color::Reset,
            bg: Color::Reset,
            mods: 0,
        }
    }

    #[test]
    fn applies_cells_and_renders_text() {
        let mut screen = Screen::new(5, 2);
        screen.apply(&op(0, 0, "h"));
        screen.apply(&op(1, 0, "i"));
        assert_eq!(screen.text(), "hi\n\n");
    }

    #[test]
    fn rejects_out_of_bounds_cells() {
        let mut screen = Screen::new(2, 2);
        screen.apply(&op(5, 5, "x"));
        assert_eq!(screen.text(), "\n\n");
    }

    #[test]
    fn clear_resets_every_cell() {
        let mut screen = Screen::new(3, 1);
        screen.apply(&op(0, 0, "a"));
        screen.apply(&op(2, 0, "b"));
        screen.clear();
        assert_eq!(screen.text(), "\n");
    }

    #[test]
    fn snapshot_omits_blank_cells_but_keeps_styled_ones() {
        let mut screen = Screen::new(4, 1);
        screen.apply(&op(1, 0, "x"));
        screen.apply(&CellOp {
            x: 3,
            y: 0,
            symbol: " ".into(),
            fg: Color::Reset,
            bg: Color::Rgb(1, 2, 3),
            mods: modifier::BOLD,
        });
        let json: serde_json::Value = serde_json::from_slice(&screen.snapshot()).unwrap();
        let cells = json["cells"].as_array().unwrap();
        assert_eq!(cells.len(), 2, "only the non-blank cells are listed");
        assert_eq!(cells[0]["s"], "x");
        assert_eq!(cells[1]["bg"], serde_json::json!({ "rgb": [1, 2, 3] }));
    }

    #[test]
    fn resize_clears_and_changes_dimensions() {
        let mut screen = Screen::new(2, 2);
        screen.apply(&op(0, 0, "a"));
        screen.resize(4, 3);
        assert_eq!((screen.cols(), screen.rows()), (4, 3));
        assert_eq!(screen.text(), "\n\n\n");
    }

    #[test]
    fn text_maps_newlines_and_tabs_to_keys() {
        assert_eq!(key_code_for('\n'), KeyCode::Enter);
        assert_eq!(key_code_for('\r'), KeyCode::Enter);
        assert_eq!(key_code_for('\t'), KeyCode::Tab);
        assert_eq!(key_code_for('x'), KeyCode::Char('x'));
    }

    #[test]
    fn streaming_publishes_only_when_enabled() {
        let mirror = Mirror::new(Screen::new(2, 1));
        let mut frames = mirror.frames.subscribe();
        mirror.apply_frame(&[op(0, 0, "a")], Cursor::default());
        assert!(frames.borrow_and_update().is_none(), "capture is off");
        mirror.set_streaming(true);
        assert!(frames.borrow_and_update().is_some(), "capture publishes");
        mirror.apply_frame(&[op(1, 0, "b")], Cursor::default());
        assert!(frames.borrow_and_update().is_some());
    }
}
