//! Terminal sessions backed by a real PTY.
//!
//! This is the generic path: Lumen allocates a pseudoterminal, runs *any*
//! program in it, and parses the program's output with a terminal emulator
//! ([`vt100`]). The parsed grid is the same [`Screen`] the ratatui path
//! produces, so the viewer, `GET /screen`, and snapshots do not care which
//! backend fed them. An unmodified ratatui app — or `htop`, or `vim`, or a
//! shell — runs here without knowing Lumen exists.
//!
//! The app owns a real terminal, so input is encoded as VT byte sequences:
//! keys become their xterm encodings, mouse events become SGR mouse reports
//! only when the running program enabled a mouse mode, and a resize sends
//! `TIOCSWINSZ` plus `SIGWINCH`.

use anyhow::{Context, Result};
use bytes::Bytes;
use lumen_ratatui::protocol::{key_mod, modifier, KeyCode, MouseButton, MouseKind};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{watch, Mutex as AsyncMutex};

/// How long a read may block before the loop checks the stop flag.
const READ_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(200);

/// A running program in a pseudoterminal, with its parsed screen.
pub struct PtySession {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: AsyncMutex<Box<dyn Child + Send + Sync>>,
    parser: Mutex<vt100::Parser>,
    mirror: Arc<PtyMirror>,
    size: Mutex<(u16, u16)>,
    alive: AtomicBool,
}

/// The shared grid state, mirroring the ratatui session's `Mirror`.
struct PtyMirror {
    frames: watch::Sender<Option<Bytes>>,
    streaming: AtomicBool,
}

impl PtyMirror {
    fn new() -> Self {
        let (frames, _) = watch::channel(None);
        Self {
            frames,
            streaming: AtomicBool::new(false),
        }
    }

    fn snapshot(&self, screen: &vt100::Screen) -> Bytes {
        screen_snapshot(screen)
    }

    fn publish(&self, snapshot: Bytes) {
        if self.streaming.load(Ordering::SeqCst) {
            self.frames.send_replace(Some(snapshot));
        }
    }
}

impl PtySession {
    /// Spawn `program` (with `args`) in a PTY of the given size, `cwd` as its
    /// working directory.
    pub async fn launch(
        program: &str,
        args: &[String],
        cwd: &Path,
        cols: u16,
        rows: u16,
    ) -> Result<Arc<Self>> {
        let cols = cols.max(1);
        let rows = rows.max(1);
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("opening a pseudoterminal")?;

        // A login-ish environment, so programs find their config and a
        // reasonable PATH even though Lumen itself may be minimal.
        let mut cmd = CommandBuilder::new(program);
        for arg in args {
            cmd.arg(arg);
        }
        cmd.cwd(cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("LINES", rows.to_string());
        cmd.env("COLUMNS", cols.to_string());
        if let Ok(path) = std::env::var("PATH") {
            cmd.env("PATH", path);
        }
        for key in ["HOME", "USER", "SHELL", "LANG", "LC_ALL"] {
            if let Ok(value) = std::env::var(key) {
                cmd.env(key, value);
            }
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .with_context(|| format!("spawning {program} in a pseudoterminal"))?;
        // The child holds the slave; the parent must drop its copy so the
        // master sees EOF when the child exits.
        drop(pair.slave);

        let reader = pair.master.try_clone_reader().context("pty reader")?;
        let writer = pair.master.take_writer().context("pty writer")?;

        let mirror = Arc::new(PtyMirror::new());
        let parser = Mutex::new(vt100::Parser::new(rows, cols, 0));

        let session = Arc::new(Self {
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            child: AsyncMutex::new(child),
            parser,
            mirror,
            size: Mutex::new((cols, rows)),
            alive: AtomicBool::new(true),
        });

        spawn_reader(session.clone(), reader);
        Ok(session)
    }

    /// Join the frame stream. The receiver stores only the newest snapshot.
    pub fn frames(&self) -> watch::Receiver<Option<Bytes>> {
        self.mirror.frames.subscribe()
    }

    /// Start or stop publishing snapshots, matching the other backends.
    pub fn set_streaming(&self, streaming: bool) -> Result<()> {
        self.mirror.streaming.store(streaming, Ordering::SeqCst);
        if streaming {
            self.publish();
        }
        Ok(())
    }

    /// The visible grid as plain text.
    pub fn screen_text(&self) -> String {
        self.parser
            .lock()
            .expect("parser mutex poisoned")
            .screen()
            .contents()
    }

    /// A snapshot of the grid for a newly attached viewer.
    pub fn snapshot(&self) -> Bytes {
        self.publish_current()
    }

    fn publish(&self) {
        let snapshot = self.publish_current();
        if self.mirror.streaming.load(Ordering::SeqCst) {
            self.mirror.frames.send_replace(Some(snapshot));
        }
    }

    fn publish_current(&self) -> Bytes {
        let parser = self.parser.lock().expect("parser mutex poisoned");
        self.mirror.snapshot(parser.screen())
    }

    /// Write bytes to the program's stdin.
    pub fn write(&self, bytes: &[u8]) -> Result<()> {
        let mut writer = self.writer.lock().expect("pty writer mutex poisoned");
        writer.write_all(bytes).context("writing to the pty")?;
        writer.flush().context("flushing the pty")?;
        Ok(())
    }

    /// Type text, with newline and tab mapped to their control bytes.
    pub fn text(&self, text: &str) -> Result<()> {
        self.write(text.as_bytes())
    }

    /// Deliver a key as its terminal byte encoding.
    pub fn key(&self, code: KeyCode, mods: u8) -> Result<()> {
        let bytes = key_bytes(code, mods);
        self.write(&bytes)
    }

    /// Deliver a mouse event. Programs only receive mouse input after they
    /// enable a tracking mode, which the emulator tracks.
    pub fn mouse(
        &self,
        kind: MouseKind,
        col: u16,
        row: u16,
        button: MouseButton,
        mods: u8,
    ) -> Result<()> {
        let mode = self.mouse_mode();
        if mode == MouseMode::Off {
            return Ok(());
        }
        let bytes = mouse_bytes(mode, kind, col, row, button, mods);
        self.write(&bytes)
    }

    /// Resize both the kernel PTY and the emulator grid.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let cols = cols.max(1);
        let rows = rows.max(1);
        {
            let master = self.master.lock().expect("pty master mutex poisoned");
            master
                .resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .context("resizing the pty")?;
        }
        {
            let mut parser = self.parser.lock().expect("parser mutex poisoned");
            // vt100 has no resize; recreate the parser at the new size. The
            // program repaints its whole screen after SIGWINCH, so losing the
            // old contents is correct rather than merely acceptable.
            *parser = vt100::Parser::new(rows, cols, 0);
        }
        *self.size.lock().expect("size mutex poisoned") = (cols, rows);
        // The parity of the program's view and ours is now re-established;
        // publish the cleared grid so viewers do not hold a stale size.
        self.publish();
        Ok(())
    }

    pub fn size(&self) -> (u16, u16) {
        *self.size.lock().expect("size mutex poisoned")
    }

    /// Whether the program is still running.
    pub async fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// End the session: close the PTY and signal the child's process group.
    pub async fn shutdown(&self) {
        self.alive.store(false, Ordering::SeqCst);
        let mut child = self.child.lock().await;
        terminate_pty_child(child.as_mut()).await;
    }

    /// Which mouse tracking mode the program enabled, from the terminal modes
    /// the emulator has parsed.
    fn mouse_mode(&self) -> MouseMode {
        let parser = self.parser.lock().expect("parser mutex poisoned");
        let screen = parser.screen();
        // vt100 exposes whether a private mode is set through `mouse_protocol_mode`.
        match screen.mouse_protocol_mode() {
            vt100::MouseProtocolMode::None => MouseMode::Off,
            vt100::MouseProtocolMode::Press => MouseMode::Press,
            vt100::MouseProtocolMode::PressRelease => MouseMode::PressRelease,
            vt100::MouseProtocolMode::ButtonMotion => MouseMode::ButtonMotion,
            vt100::MouseProtocolMode::AnyMotion => MouseMode::AnyMotion,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MouseMode {
    Off,
    Press,
    PressRelease,
    ButtonMotion,
    AnyMotion,
}

/// Read the PTY until the program exits or the session is stopped.
fn spawn_reader(session: Arc<PtySession>, mut reader: Box<dyn Read + Send>) {
    let mirror = session.mirror.clone();
    let _ = std::thread::Builder::new()
        .name("lumen-pty-reader".into())
        .spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        {
                            let mut parser = session.parser.lock().expect("parser mutex poisoned");
                            parser.process(&buf[..n]);
                        }
                        let snapshot = session.publish_current();
                        mirror.publish(snapshot);
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        if !session.alive.load(Ordering::SeqCst) {
                            break;
                        }
                        std::thread::sleep(READ_TIMEOUT);
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            session.alive.store(false, Ordering::SeqCst);
            // Publish a final snapshot so a viewer sees the last state.
            let snapshot = session.publish_current();
            mirror.frames.send_replace(Some(snapshot));
        });
}

/// Terminate the PTY child, reusing the process-group teardown used elsewhere.
async fn terminate_pty_child(child: &mut dyn Child) {
    if let Some(pid) = child.process_id() {
        // SAFETY: signals only this child's process group, created by the PTY
        // spawn; errors are ignored intentionally.
        unsafe {
            libc::kill(-(pid as i32), libc::SIGTERM);
        }
        for _ in 0..20 {
            if child.try_wait().ok().flatten().is_some() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    let _ = child.kill();
}

/// Encode a key press as the byte sequence an xterm would send.
pub fn key_bytes(code: KeyCode, mods: u8) -> Vec<u8> {
    let alt = mods & key_mod::ALT != 0;
    let mut out: Vec<u8> = Vec::new();
    match code {
        KeyCode::Char(ch) => {
            // Control characters: Ctrl+A is 0x01, Ctrl+Space is 0x00.
            if mods & key_mod::CONTROL != 0 {
                let upper = ch.to_ascii_uppercase();
                if ('@'..='_').contains(&upper) {
                    out.push((upper as u8) - 0x40);
                } else if ch == ' ' {
                    out.push(0);
                } else {
                    out.extend_from_slice(ch.to_string().as_bytes());
                }
            } else {
                out.extend_from_slice(ch.to_string().as_bytes());
            }
            if alt {
                out.insert(0, 0x1b);
            }
        }
        KeyCode::Enter => out.extend_from_slice(if alt { b"\x1b\r" } else { b"\r" }),
        KeyCode::Tab => out.extend_from_slice(if alt { b"\x1b\t" } else { b"\t" }),
        KeyCode::BackTab => out.extend_from_slice(b"\x1b[Z"),
        KeyCode::Backspace => {
            if alt {
                out.push(0x1b);
            }
            out.push(0x7f);
        }
        KeyCode::Esc => out.push(0x1b),
        KeyCode::Up => out.extend_from_slice(&csi_or_ss3(b'A', mods)),
        KeyCode::Down => out.extend_from_slice(&csi_or_ss3(b'B', mods)),
        KeyCode::Right => out.extend_from_slice(&csi_or_ss3(b'C', mods)),
        KeyCode::Left => out.extend_from_slice(&csi_or_ss3(b'D', mods)),
        KeyCode::Home => out.extend_from_slice(&csi_or_ss3(b'H', mods)),
        KeyCode::End => out.extend_from_slice(&csi_or_ss3(b'F', mods)),
        KeyCode::PageUp => out.extend_from_slice(b"\x1b[5~"),
        KeyCode::PageDown => out.extend_from_slice(b"\x1b[6~"),
        KeyCode::Insert => out.extend_from_slice(b"\x1b[2~"),
        KeyCode::Delete => out.extend_from_slice(b"\x1b[3~"),
        KeyCode::F(n) => out.extend_from_slice(&f_key_bytes(n)),
    }
    out
}

/// Arrows/Home/End use `CSI` form when modified (or in application cursor mode
/// they use `SS3`); plain presses use `SS3` for the legacy keys and `CSI` for
/// the rest, which is what xterm-compatible apps expect.
fn csi_or_ss3(letter: u8, mods: u8) -> Vec<u8> {
    let modifier_param = modifier_param(mods);
    match modifier_param {
        Some(param) => format!("\x1b[1;{param}{}", letter as char).into_bytes(),
        // Unmodified arrow/home/end in normal mode are `SS3`; applications that
        // need `CSI` set DECCKM, but `CSI` is accepted more broadly.
        None => vec![0x1b, b'[', letter],
    }
}

/// The xterm modifier parameter: 2=Shift, 3=Alt, 5=Ctrl, with sums for combos.
fn modifier_param(mods: u8) -> Option<u8> {
    // xterm's parameter is the sum of modifier bits plus one: 1=none, 2=Shift,
    // 3=Alt, 4=Shift+Alt, 5=Ctrl, and so on.
    let mut value = 0;
    if mods & key_mod::SHIFT != 0 {
        value += 1;
    }
    if mods & key_mod::ALT != 0 {
        value += 2;
    }
    if mods & key_mod::CONTROL != 0 {
        value += 4;
    }
    (value > 0).then_some(value + 1)
}

fn f_key_bytes(n: u8) -> Vec<u8> {
    let code = match n {
        1 => "\x1bOP",
        2 => "\x1bOQ",
        3 => "\x1bOR",
        4 => "\x1bOS",
        5 => "\x1b[15~",
        6 => "\x1b[17~",
        7 => "\x1b[18~",
        8 => "\x1b[19~",
        9 => "\x1b[20~",
        10 => "\x1b[21~",
        11 => "\x1b[23~",
        12 => "\x1b[24~",
        _ => return Vec::new(),
    };
    code.as_bytes().to_vec()
}

/// Encode a mouse event as an SGR (1006) report.
///
/// The `mode` gate lives in [`PtySession::mouse`], which consults the emulator;
/// this function encodes whatever it is told to, so `Off` is treated as the
/// least capable reporting mode (press only).
fn mouse_bytes(
    mode: MouseMode,
    kind: MouseKind,
    col: u16,
    row: u16,
    button: MouseButton,
    mods: u8,
) -> Vec<u8> {
    let release = matches!(kind, MouseKind::Up);
    let (motion, scroll) = match kind {
        MouseKind::ScrollUp => (false, Some(0)),
        MouseKind::ScrollDown => (false, Some(1)),
        _ => (matches!(kind, MouseKind::Moved | MouseKind::Drag), None),
    };
    if motion
        && matches!(
            mode,
            MouseMode::Press | MouseMode::PressRelease | MouseMode::Off
        )
    {
        // These modes do not report motion.
        return Vec::new();
    }
    let mut code: u16 = match scroll {
        Some(direction) => 64 + direction,
        None => match button {
            MouseButton::Left => 0,
            MouseButton::Middle => 1,
            MouseButton::Right => 2,
            MouseButton::None => 3,
        },
    };
    if motion {
        code += 32;
    }
    if mods & key_mod::SHIFT != 0 {
        code += 4;
    }
    if mods & key_mod::ALT != 0 {
        code += 8;
    }
    if mods & key_mod::CONTROL != 0 {
        code += 16;
    }
    // SGR reports are 1-based, and the final byte is `m` for release.
    format!(
        "\x1b[<{};{};{}{}",
        code,
        col as u32 + 1,
        row as u32 + 1,
        if release { 'm' } else { 'M' }
    )
    .into_bytes()
}

/// Build the viewer snapshot from a `vt100` screen.
///
/// The JSON shape matches the ratatui path, so the viewer is identical; only
/// the source of the cells differs.
fn screen_snapshot(screen: &vt100::Screen) -> Bytes {
    use lumen_ratatui::protocol::Color;
    use serde::Serialize;

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

    let (rows, cols) = screen.size();
    let (cursor_row, cursor_col) = screen.cursor_position();
    let mut cells = Vec::new();
    for row in 0..rows {
        for col in 0..cols {
            let Some(cell) = screen.cell(row, col) else {
                continue;
            };
            let symbol = cell.contents();
            let fg = vt_color(cell.fgcolor());
            let bg = vt_color(cell.bgcolor());
            let mut mods = 0u16;
            if cell.bold() {
                mods |= modifier::BOLD;
            }
            if cell.dim() {
                mods |= modifier::DIM;
            }
            if cell.italic() {
                mods |= modifier::ITALIC;
            }
            if cell.underline() {
                mods |= modifier::UNDERLINED;
            }
            if cell.inverse() {
                mods |= modifier::REVERSED;
            }
            // Skip truly blank cells so the payload stays small, exactly as the
            // ratatui path does.
            let blank = (symbol.is_empty() || symbol == " ")
                && fg == Color::Reset
                && bg == Color::Reset
                && mods == 0;
            if blank {
                continue;
            }
            let owned = if symbol.is_empty() { " " } else { symbol };
            cells.push(CellWire {
                x: col,
                y: row,
                s: owned,
                fg,
                bg,
                m: mods,
            });
        }
    }
    let snapshot = Snapshot {
        cols,
        rows,
        cx: cursor_col,
        cy: cursor_row,
        cv: !screen.hide_cursor(),
        cells,
    };
    Bytes::from(serde_json::to_vec(&snapshot).expect("a terminal snapshot serializes"))
}

fn vt_color(color: vt100::Color) -> lumen_ratatui::protocol::Color {
    use lumen_ratatui::protocol::Color;
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(index) => Color::Indexed(index),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

/// Parse a command line into a program and its arguments, so `path` may be a
/// shell command. Quoting is shell-like to a first approximation.
pub fn parse_command(line: &str) -> Result<(String, Vec<String>)> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match (quote, ch) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => current.push(c),
            (None, '\'' | '"') => quote = Some(ch),
            (None, '\\') => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            (None, c) if c.is_whitespace() => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
            }
            (None, c) => current.push(c),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    let mut parts = parts.into_iter();
    let program = parts.next().context("empty command")?;
    Ok((program, parts.collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_encode_like_a_terminal() {
        assert_eq!(key_bytes(KeyCode::Up, 0), b"\x1b[A");
        assert_eq!(key_bytes(KeyCode::Char('a'), 0), b"a");
        assert_eq!(key_bytes(KeyCode::Enter, 0), b"\r");
        assert_eq!(key_bytes(KeyCode::Tab, 0), b"\t");
        assert_eq!(key_bytes(KeyCode::PageUp, 0), b"\x1b[5~");
        // Ctrl+C is 0x03; Ctrl+A is 0x01.
        assert_eq!(key_bytes(KeyCode::Char('c'), key_mod::CONTROL), vec![0x03]);
        assert_eq!(key_bytes(KeyCode::Char('a'), key_mod::CONTROL), vec![0x01]);
        // A plain arrow with no modifier uses CSI form.
        assert_eq!(key_bytes(KeyCode::Left, 0), b"\x1b[D");
        // A modified arrow carries the modifier parameter.
        assert_eq!(key_bytes(KeyCode::Right, key_mod::SHIFT), b"\x1b[1;2C");
    }

    #[test]
    fn mouse_reports_are_sgr_one_based() {
        let bytes = mouse_bytes(
            MouseMode::PressRelease,
            MouseKind::Down,
            0,
            0,
            MouseButton::Left,
            0,
        );
        assert_eq!(bytes, b"\x1b[<0;1;1M");
        let release = mouse_bytes(
            MouseMode::PressRelease,
            MouseKind::Up,
            4,
            2,
            MouseButton::Left,
            0,
        );
        assert_eq!(release, b"\x1b[<0;5;3m");
        let scroll = mouse_bytes(
            MouseMode::AnyMotion,
            MouseKind::ScrollUp,
            1,
            1,
            MouseButton::None,
            0,
        );
        assert_eq!(scroll, b"\x1b[<64;2;2M");
    }

    #[test]
    fn press_reports_are_emitted_but_motion_is_not_without_tracking() {
        // A press is still encoded: the session layer gates on the emulator's
        // live mouse mode, so the encoder only decides the byte shape.
        assert_eq!(
            mouse_bytes(MouseMode::Off, MouseKind::Down, 1, 1, MouseButton::Left, 0),
            b"\x1b[<0;2;2M"
        );
        // Motion in a press-only mode is suppressed, which is what the mode means.
        assert!(
            mouse_bytes(MouseMode::Off, MouseKind::Moved, 1, 1, MouseButton::None, 0).is_empty()
        );
    }

    #[test]
    fn parses_a_command_line_with_quotes() {
        let (program, args) = parse_command("echo 'hello world' --flag").unwrap();
        assert_eq!(program, "echo");
        assert_eq!(args, vec!["hello world", "--flag"]);
        let (program, args) = parse_command("/usr/bin/fish -l").unwrap();
        assert_eq!(program, "/usr/bin/fish");
        assert_eq!(args, vec!["-l"]);
    }

    #[test]
    fn snapshots_only_styled_or_visible_cells() {
        let mut parser = vt100::Parser::new(2, 10, 0);
        parser.process(b"\x1b[31mhi\x1b[0m");
        let json: serde_json::Value =
            serde_json::from_slice(&screen_snapshot(parser.screen())).unwrap();
        let cells = json["cells"].as_array().unwrap();
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0]["s"], "h");
        assert_eq!(cells[0]["fg"], serde_json::json!({ "indexed": 1 }));
    }
}
