//! Isolated headless Wayland sessions for Quickshell and other desktop surfaces.

use anyhow::{anyhow, bail, Context, Result};
use bytes::Bytes;
use jpeg_encoder::{ColorType, Encoder};
use std::fs::File;
use std::io;
use std::os::fd::{AsFd, AsRawFd, FromRawFd};
use std::os::unix::fs::FileTypeExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tokio::process::{Child, Command};
use tokio::sync::watch;
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::protocol::wl_pointer::{Axis, ButtonState};
use wayland_client::protocol::wl_registry;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::protocol::wl_shm::{Format, WlShm};
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, WEnum};
use wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_frame_v1;
use wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1;
use wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1;
use wayland_protocols_wlr::virtual_pointer::v1::client::zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1;
use wayland_protocols_wlr::virtual_pointer::v1::client::zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1;

const WAYLAND_READY_TIMEOUT: Duration = Duration::from_secs(10);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const SCREENSHOT_TIMEOUT: Duration = Duration::from_secs(10);
const WAYLAND_POLL_TIMEOUT_MS: i32 = 100;
const STREAM_FRAME_INTERVAL: Duration = Duration::from_millis(100);

/// A mouse event expressed in output pixels.
#[derive(Debug, Clone, Copy)]
pub enum MouseAction {
    Down,
    Up,
    Move,
}

/// A program Lumen runs inside a desktop session.
#[derive(Debug, Clone)]
pub struct DesktopApp {
    pub program: String,
    pub args: Vec<String>,
}

/// The helper binaries a desktop session runs.
#[derive(Debug, Clone, Copy)]
pub struct DesktopBins<'a> {
    /// The Wayland compositor.
    pub sway: &'a str,
    /// Text input for the compositor.
    pub wtype: &'a str,
    /// The session bus daemon.
    pub dbus: &'a str,
    /// The AT-SPI registry daemon, or `None` to discover one.
    pub registryd: Option<&'a str>,
}

/// A child compositor and one application, with a native Wayland capture path.
/// Sessions whose application publishes an accessibility tree additionally run a
/// private D-Bus and an AT-SPI registry.
pub struct DesktopSession {
    pub display: String,
    pub width: u32,
    pub height: u32,
    runtime_dir: PathBuf,
    wtype_bin: String,
    /// Address of the private D-Bus an accessibility-publishing application
    /// uses, or `None` for a session that does not publish one.
    bus_address: Option<String>,
    commands: mpsc::Sender<DesktopCommand>,
    thread: Mutex<Option<JoinHandle<()>>>,
    alive: Arc<std::sync::atomic::AtomicBool>,
    frames: watch::Sender<Option<Bytes>>,
    sway: Mutex<Option<Child>>,
    app: Mutex<Option<Child>>,
    dbus: Mutex<Option<Child>>,
    registryd: Mutex<Option<Child>>,
}

impl DesktopSession {
    /// Start one headless Sway output and one application on it. When
    /// `accessibility` is set the session also starts a private session bus and
    /// an AT-SPI registry so the application can publish an accessibility tree.
    pub async fn launch(
        bins: &DesktopBins<'_>,
        app: &DesktopApp,
        accessibility: bool,
        profile: &Path,
        width: u32,
        height: u32,
    ) -> Result<Arc<Self>> {
        if width == 0 || height == 0 {
            bail!("desktop dimensions must be positive");
        }
        if width > u16::MAX as u32 || height > u16::MAX as u32 {
            bail!("desktop dimensions exceed the viewer's maximum frame size");
        }

        let runtime_target = profile.join("runtime");
        std::fs::create_dir_all(&runtime_target)
            .with_context(|| format!("creating {}", runtime_target.display()))?;
        std::fs::set_permissions(
            &runtime_target,
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )?;
        // Sway puts its IPC socket beneath XDG_RUNTIME_DIR. Keep the actual
        // directory in the owned profile, but expose it through a short link:
        // Unix socket paths have a platform limit of roughly 108 bytes.
        let runtime_dir = std::env::temp_dir().join(format!("lumen-wl-{}", unique_suffix()));
        if let Err(err) = std::os::unix::fs::symlink(&runtime_target, &runtime_dir) {
            return Err(err).with_context(|| format!("linking {}", runtime_dir.display()));
        }

        // An application publishes its accessibility tree on a session bus, and
        // the registry that indexes it cannot be started lazily on a private
        // bus: when the host runs systemd the bus launcher delegates activation
        // to systemd, which can only reach the real session bus. Start the bus
        // and registry eagerly, and only for a session that needs them.
        let mut bus_address = None;
        let mut dbus = None;
        let mut registryd_child = None;
        if accessibility {
            let bus_socket = runtime_dir.join("bus");
            let address = format!("unix:path={}", bus_socket.display());
            let mut bus = match Command::new(bins.dbus)
                .arg("--session")
                .arg("--nofork")
                .arg(format!("--address={address}"))
                .env("XDG_RUNTIME_DIR", &runtime_dir)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::inherit())
                .kill_on_drop(true)
                .process_group(0)
                .spawn()
            {
                Ok(child) => child,
                Err(err) => {
                    let _ = std::fs::remove_file(&runtime_dir);
                    return Err(err).with_context(|| format!("spawning {}", bins.dbus));
                }
            };
            if let Err(err) = wait_for_bus(&mut bus, &bus_socket).await {
                terminate_process_group(&mut bus).await;
                let _ = std::fs::remove_file(&runtime_dir);
                return Err(err);
            }

            let registryd_bin = match find_registryd(bins.registryd) {
                Ok(bin) => bin,
                Err(err) => {
                    terminate_process_group(&mut bus).await;
                    let _ = std::fs::remove_file(&runtime_dir);
                    return Err(err);
                }
            };
            let mut registry = match Command::new(&registryd_bin)
                .env("DBUS_SESSION_BUS_ADDRESS", &address)
                .env("XDG_RUNTIME_DIR", &runtime_dir)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::inherit())
                .kill_on_drop(true)
                .process_group(0)
                .spawn()
            {
                Ok(child) => child,
                Err(err) => {
                    terminate_process_group(&mut bus).await;
                    let _ = std::fs::remove_file(&runtime_dir);
                    return Err(err)
                        .with_context(|| format!("spawning {}", registryd_bin.display()));
                }
            };
            if let Err(err) = crate::accessibility::await_registry(&address).await {
                terminate_process_group(&mut registry).await;
                terminate_process_group(&mut bus).await;
                let _ = std::fs::remove_file(&runtime_dir);
                return Err(err);
            }

            bus_address = Some(address);
            dbus = Some(bus);
            registryd_child = Some(registry);
        }

        let display_hint = format!("lumen-{}-{}", std::process::id(), unique_suffix());
        let sway_config = profile.join("sway.conf");
        std::fs::write(
            &sway_config,
            format!(
                "output * mode {width}x{height}\ndefault_border none\nfocus_follows_mouse no\n"
            ),
        )
        .with_context(|| format!("writing {}", sway_config.display()))?;

        let sway = Command::new(bins.sway)
            .args(["-c", sway_config.to_string_lossy().as_ref()])
            .env("XDG_RUNTIME_DIR", &runtime_dir)
            .env("WAYLAND_DISPLAY", &display_hint)
            .env("WLR_BACKENDS", "headless")
            .env("WLR_RENDERER", "pixman")
            .env("WLR_HEADLESS_OUTPUTS", "1")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .process_group(0)
            .spawn()
            .with_context(|| format!("spawning {}", bins.sway));
        let mut sway = match sway {
            Ok(sway) => sway,
            Err(err) => {
                terminate_if_running(&mut registryd_child).await;
                terminate_if_running(&mut dbus).await;
                let _ = std::fs::remove_file(&runtime_dir);
                return Err(err);
            }
        };
        let display = match wait_for_socket(&mut sway, &runtime_dir).await {
            Ok(display) => display,
            Err(err) => {
                terminate_process_group(&mut sway).await;
                terminate_if_running(&mut registryd_child).await;
                terminate_if_running(&mut dbus).await;
                let _ = std::fs::remove_file(&runtime_dir);
                return Err(err);
            }
        };

        let mut app_command = Command::new(&app.program);
        app_command
            .args(&app.args)
            .env("XDG_RUNTIME_DIR", &runtime_dir)
            .env("WAYLAND_DISPLAY", &display)
            .env("QT_QPA_PLATFORM", "wayland")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .process_group(0);
        if let Some(address) = &bus_address {
            app_command
                .env("DBUS_SESSION_BUS_ADDRESS", address)
                // Qt only keeps its AT-SPI bridge alive when told to; on a
                // headless session it otherwise drops accessibility.
                .env("QT_ACCESSIBILITY", "1")
                .env("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "1");
        }
        let mut app_child = match app_command.spawn() {
            Ok(child) => child,
            Err(err) => {
                terminate_process_group(&mut sway).await;
                terminate_if_running(&mut registryd_child).await;
                terminate_if_running(&mut dbus).await;
                let _ = std::fs::remove_file(&runtime_dir);
                return Err(err).with_context(|| format!("spawning {}", app.program));
            }
        };

        let (commands, command_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (frames, _) = watch::channel(None);
        let alive = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let thread_alive = alive.clone();
        let thread_frames = frames.clone();
        let thread_display = display.clone();
        let thread_runtime = runtime_dir.clone();
        let thread = match std::thread::Builder::new()
            .name(format!("lumen-wayland-{display}"))
            .spawn(move || {
                run_wayland(WaylandRun {
                    runtime_dir: thread_runtime,
                    display: thread_display,
                    width,
                    height,
                    commands: command_rx,
                    frames: thread_frames,
                    alive: thread_alive,
                    ready: ready_tx,
                });
            })
            .context("starting Wayland capture thread")
        {
            Ok(thread) => thread,
            Err(err) => {
                terminate_process_group(&mut app_child).await;
                terminate_process_group(&mut sway).await;
                terminate_if_running(&mut registryd_child).await;
                terminate_if_running(&mut dbus).await;
                let _ = std::fs::remove_file(&runtime_dir);
                return Err(err);
            }
        };

        let session = Arc::new(Self {
            display,
            width,
            height,
            runtime_dir,
            wtype_bin: bins.wtype.to_string(),
            bus_address,
            commands,
            thread: Mutex::new(Some(thread)),
            alive,
            frames,
            sway: Mutex::new(Some(sway)),
            app: Mutex::new(Some(app_child)),
            dbus: Mutex::new(dbus),
            registryd: Mutex::new(registryd_child),
        });

        let ready = tokio::task::spawn_blocking(move || {
            ready_rx
                .recv_timeout(WAYLAND_READY_TIMEOUT)
                .map_err(|_| anyhow!("timed out waiting for Wayland capture setup"))
        })
        .await
        .context("waiting for Wayland capture setup")??;
        if let Err(err) = ready {
            session.shutdown().await;
            return Err(anyhow!(err));
        }
        Ok(session)
    }

    pub fn frames(&self) -> watch::Receiver<Option<Bytes>> {
        self.frames.subscribe()
    }

    /// Address of the session's private D-Bus, when it publishes an
    /// accessibility tree.
    pub fn bus_address(&self) -> Option<&str> {
        self.bus_address.as_deref()
    }

    pub fn set_streaming(&self, streaming: bool) -> Result<()> {
        self.commands
            .send(if streaming {
                DesktopCommand::Start
            } else {
                DesktopCommand::Stop
            })
            .map_err(|_| anyhow!("Wayland capture thread is not running"))
    }

    pub async fn mouse(&self, action: MouseAction, x: f64, y: f64, button: &str) -> Result<()> {
        self.command_with_reply(|reply| DesktopCommand::Mouse {
            action,
            x,
            y,
            button: pointer_button(button),
            reply,
        })
        .await
    }

    pub async fn wheel(&self, dx: f64, dy: f64) -> Result<()> {
        self.command_with_reply(|reply| DesktopCommand::Wheel { dx, dy, reply })
            .await
    }

    pub async fn text(&self, text: &str) -> Result<()> {
        let status = Command::new(&self.wtype_bin)
            .arg("--")
            .arg(text)
            .env("XDG_RUNTIME_DIR", &self.runtime_dir)
            .env("WAYLAND_DISPLAY", &self.display)
            .status()
            .await
            .with_context(|| format!("spawning {}", self.wtype_bin))?;
        if !status.success() {
            bail!("{} exited with {status}", self.wtype_bin);
        }
        Ok(())
    }

    pub async fn screenshot(&self) -> Result<Vec<u8>> {
        let (reply, result) = sync_reply();
        self.commands
            .send(DesktopCommand::Screenshot(reply))
            .map_err(|_| anyhow!("Wayland capture thread is not running"))?;
        tokio::task::spawn_blocking(move || {
            result
                .recv_timeout(SCREENSHOT_TIMEOUT)
                .map_err(|_| anyhow!("timed out waiting for desktop screenshot"))?
                .map_err(anyhow::Error::msg)
        })
        .await
        .context("waiting for desktop screenshot")?
    }

    pub async fn is_alive(&self) -> bool {
        if !self.alive.load(std::sync::atomic::Ordering::SeqCst) {
            return false;
        }
        let sway_alive = self
            .sway
            .lock()
            .expect("sway mutex poisoned")
            .as_mut()
            .map(Child::try_wait);
        let app_alive = self
            .app
            .lock()
            .expect("app mutex poisoned")
            .as_mut()
            .map(Child::try_wait);
        matches!(sway_alive, Some(Ok(None))) && matches!(app_alive, Some(Ok(None)))
    }

    pub async fn shutdown(&self) {
        let _ = self.commands.send(DesktopCommand::Shutdown);
        if let Some(thread) = self.thread.lock().expect("thread mutex poisoned").take() {
            let _ = thread.join();
        }
        let app = self.app.lock().expect("app mutex poisoned").take();
        if let Some(mut app) = app {
            terminate_process_group(&mut app).await;
        }
        let sway = self.sway.lock().expect("sway mutex poisoned").take();
        if let Some(mut sway) = sway {
            terminate_process_group(&mut sway).await;
        }
        let registryd = self
            .registryd
            .lock()
            .expect("registryd mutex poisoned")
            .take();
        if let Some(mut registryd) = registryd {
            terminate_process_group(&mut registryd).await;
        }
        let dbus = self.dbus.lock().expect("dbus mutex poisoned").take();
        if let Some(mut dbus) = dbus {
            terminate_process_group(&mut dbus).await;
        }
        let _ = std::fs::remove_file(&self.runtime_dir);
    }

    async fn command_with_reply<F>(&self, command: F) -> Result<()>
    where
        F: FnOnce(SyncSender<std::result::Result<(), String>>) -> DesktopCommand,
    {
        let (reply, result) = sync_reply();
        self.commands
            .send(command(reply))
            .map_err(|_| anyhow!("Wayland capture thread is not running"))?;
        tokio::task::spawn_blocking(move || {
            result
                .recv_timeout(COMMAND_TIMEOUT)
                .map_err(|_| anyhow!("timed out waiting for Wayland input"))?
                .map_err(anyhow::Error::msg)
        })
        .await
        .context("waiting for Wayland input")?
    }
}

impl Drop for DesktopSession {
    fn drop(&mut self) {
        let _ = self.commands.send(DesktopCommand::Shutdown);
        if let Some(thread) = self.thread.get_mut().expect("thread mutex poisoned").take() {
            let _ = thread.join();
        }
        let _ = std::fs::remove_file(&self.runtime_dir);
    }
}

enum DesktopCommand {
    Start,
    Stop,
    Mouse {
        action: MouseAction,
        x: f64,
        y: f64,
        button: u32,
        reply: SyncSender<std::result::Result<(), String>>,
    },
    Wheel {
        dx: f64,
        dy: f64,
        reply: SyncSender<std::result::Result<(), String>>,
    },
    Screenshot(SyncSender<std::result::Result<Vec<u8>, String>>),
    Shutdown,
}

type FrameResult = std::result::Result<Vec<u8>, String>;

struct RawFrame {
    pixels: Vec<u8>,
    format: Format,
    width: u32,
    height: u32,
    stride: u32,
    invert_y: bool,
}

struct EncodeJob {
    frame: RawFrame,
    stream: bool,
    screenshot: Option<SyncSender<FrameResult>>,
}

struct EncoderState {
    pending: Option<EncodeJob>,
    shutdown: bool,
}

struct EncoderWorker {
    state: Arc<(Mutex<EncoderState>, Condvar)>,
    thread: Option<JoinHandle<()>>,
}

impl EncoderWorker {
    fn new(frames: watch::Sender<Option<Bytes>>) -> Result<Self> {
        let state = Arc::new((
            Mutex::new(EncoderState {
                pending: None,
                shutdown: false,
            }),
            Condvar::new(),
        ));
        let worker_state = state.clone();
        let thread = std::thread::Builder::new()
            .name("lumen-desktop-encoder".into())
            .spawn(move || run_encoder(worker_state, frames))
            .context("starting desktop encoder thread")?;
        Ok(Self {
            state,
            thread: Some(thread),
        })
    }

    fn submit(&self, job: EncodeJob) {
        let (state, wake) = &*self.state;
        let mut state = state.lock().expect("encoder mutex poisoned");
        if state.shutdown {
            if let Some(reply) = job.screenshot {
                let _ = reply.send(Err("desktop encoder is shutting down".into()));
            }
            return;
        }
        if state
            .pending
            .as_ref()
            .is_some_and(|pending| pending.screenshot.is_some())
        {
            if let Some(reply) = job.screenshot {
                let _ = reply.send(Err("desktop screenshot encoder is busy".into()));
            }
            return;
        }
        state.pending = Some(job);
        wake.notify_one();
    }
}

impl Drop for EncoderWorker {
    fn drop(&mut self) {
        let (state, wake) = &*self.state;
        let mut state = state.lock().expect("encoder mutex poisoned");
        state.shutdown = true;
        if let Some(job) = state.pending.take() {
            if let Some(reply) = job.screenshot {
                let _ = reply.send(Err("desktop encoder is shutting down".into()));
            }
        }
        wake.notify_one();
        drop(state);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run_encoder(state: Arc<(Mutex<EncoderState>, Condvar)>, frames: watch::Sender<Option<Bytes>>) {
    loop {
        let job = {
            let (state, wake) = &*state;
            let mut state = state.lock().expect("encoder mutex poisoned");
            while state.pending.is_none() && !state.shutdown {
                state = wake.wait(state).expect("encoder mutex poisoned");
            }
            if state.shutdown {
                return;
            }
            state.pending.take().expect("pending encode job")
        };

        match convert_frame(&job.frame) {
            Ok(rgb) => {
                if job.stream {
                    match encode_jpeg(&rgb, job.frame.width, job.frame.height) {
                        Ok(frame) => {
                            frames.send_replace(Some(Bytes::from(frame)));
                        }
                        Err(err) => tracing::warn!("encoding desktop frame failed: {err}"),
                    }
                }
                if let Some(reply) = job.screenshot {
                    let _ = reply.send(encode_png(&rgb, job.frame.width, job.frame.height));
                }
            }
            Err(err) => {
                if let Some(reply) = job.screenshot {
                    let _ = reply.send(Err(err.to_string()));
                }
            }
        }
    }
}

type SyncReply<T> = (
    SyncSender<std::result::Result<T, String>>,
    Receiver<std::result::Result<T, String>>,
);

fn sync_reply<T>() -> SyncReply<T> {
    mpsc::sync_channel(1)
}

fn pointer_button(button: &str) -> u32 {
    match button {
        "right" => 0x111,
        "middle" => 0x112,
        _ => 0x110,
    }
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

async fn wait_for_socket(child: &mut Child, runtime_dir: &Path) -> Result<String> {
    let deadline = tokio::time::Instant::now() + WAYLAND_READY_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        if let Some(display) = find_wayland_socket(runtime_dir) {
            return Ok(display);
        }
        if let Some(status) = child.try_wait().context("checking Sway")? {
            bail!("Sway exited before its Wayland socket was ready: {status}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    bail!(
        "timed out waiting for a Sway Wayland socket in {}",
        runtime_dir.display()
    )
}

/// Wait until the private session bus has created its socket.
async fn wait_for_bus(child: &mut Child, socket: &Path) -> Result<()> {
    let deadline = tokio::time::Instant::now() + WAYLAND_READY_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        if socket.exists() {
            return Ok(());
        }
        if let Some(status) = child.try_wait().context("checking the session bus")? {
            bail!("the session bus exited before it was ready: {status}");
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    bail!(
        "timed out waiting for the session bus at {}",
        socket.display()
    )
}

/// Where distributions install the AT-SPI registry daemon.
const REGISTRYD_CANDIDATES: [&str; 4] = [
    "at-spi2-registryd",
    "/usr/libexec/at-spi2-registryd",
    "/usr/libexec/at-spi2/at-spi2-registryd",
    "/usr/lib/at-spi2-core/at-spi2-registryd",
];

/// Resolve the accessibility registry daemon.
///
/// An explicit path from configuration wins; otherwise try the name on `PATH`
/// and the libexec locations used by Debian, Ubuntu, and openSUSE.
fn find_registryd(configured: Option<&str>) -> Result<PathBuf> {
    if let Some(path) = configured {
        let path = PathBuf::from(path);
        if executable_file(&path) {
            return Ok(path);
        }
        bail!(
            "configured accessibility registry {} is not an executable file",
            path.display()
        );
    }
    for candidate in REGISTRYD_CANDIDATES {
        if let Some(path) = which_executable(candidate) {
            return Ok(path);
        }
    }
    bail!("could not find at-spi2-registryd; install at-spi2-core or set LUMEN_ATSPI_REGISTRYD")
}

fn which_executable(name: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let path = PathBuf::from(name);
        return executable_file(&path).then_some(path);
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(name))
            .find(|candidate| executable_file(candidate))
    })
}

fn executable_file(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn find_wayland_socket(runtime_dir: &Path) -> Option<String> {
    std::fs::read_dir(runtime_dir).ok()?.find_map(|entry| {
        let entry = entry.ok()?;
        let name = entry.file_name().into_string().ok()?;
        if !name.starts_with("wayland-") || !entry.file_type().ok()?.is_socket() {
            return None;
        }
        Some(name)
    })
}

struct WaylandRun {
    runtime_dir: PathBuf,
    display: String,
    width: u32,
    height: u32,
    commands: Receiver<DesktopCommand>,
    frames: watch::Sender<Option<Bytes>>,
    alive: Arc<std::sync::atomic::AtomicBool>,
    ready: SyncSender<Result<(), String>>,
}

fn run_wayland(run: WaylandRun) {
    let WaylandRun {
        runtime_dir,
        display,
        width,
        height,
        commands,
        frames,
        alive,
        ready,
    } = run;
    let result = run_wayland_inner(
        runtime_dir,
        display,
        width,
        height,
        commands,
        frames,
        ready.clone(),
    );
    alive.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = ready.send(result.map_err(|err| err.to_string()));
}

fn run_wayland_inner(
    runtime_dir: PathBuf,
    display: String,
    width: u32,
    height: u32,
    commands: Receiver<DesktopCommand>,
    frames: watch::Sender<Option<Bytes>>,
    ready: SyncSender<Result<(), String>>,
) -> Result<()> {
    let socket = runtime_dir.join(&display);
    let (_connection, globals, mut queue) = {
        let mut last_error: Option<anyhow::Error> = None;
        let mut connected = None;
        for attempt in 0..20 {
            let stream = match std::os::unix::net::UnixStream::connect(&socket) {
                Ok(stream) => stream,
                Err(err) if attempt < 19 => {
                    last_error = Some(anyhow!(err));
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                }
                Err(err) => {
                    return Err(err).with_context(|| {
                        format!("connecting to Wayland socket {}", socket.display())
                    });
                }
            };
            let connection =
                Connection::from_socket(stream).context("creating Wayland connection")?;
            match registry_queue_init::<WaylandState>(&connection) {
                Ok((globals, queue)) => {
                    connected = Some((connection, globals, queue));
                    break;
                }
                Err(err) if attempt < 19 => {
                    last_error = Some(anyhow!(err));
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(err) => return Err(err).context("reading Wayland globals"),
            }
        }
        connected.ok_or_else(|| {
            last_error
                .map(|err| anyhow!("reading Wayland globals: {err}"))
                .unwrap_or_else(|| anyhow!("reading Wayland globals"))
        })?
    };
    let qh = queue.handle();
    let output = first_output(&globals, &qh)?;
    let shm: WlShm = globals.bind(&qh, 1..=1, ()).context("binding wl_shm")?;
    let screencopy: ZwlrScreencopyManagerV1 = globals
        .bind(&qh, 3..=3, ())
        .context("binding zwlr_screencopy_manager_v1 version 3")?;
    let pointer_manager: ZwlrVirtualPointerManagerV1 = globals
        .bind(&qh, 1..=2, ())
        .context("binding zwlr_virtual_pointer_manager_v1")?;
    let seat = first_seat(&globals, &qh);
    let pointer = pointer_manager.create_virtual_pointer(seat.as_ref(), &qh, ());
    let mut state = WaylandState {
        output,
        shm,
        screencopy,
        pointer,
        frame: None,
        buffer: None,
        pending: None,
        invert_y: false,
        streaming: false,
        stream_has_frame: false,
        damage_copy: false,
        next_stream_capture: Instant::now(),
        screenshot_waiter: None,
        encoder: EncoderWorker::new(frames)?,
        width,
        height,
        started: Instant::now(),
    };
    queue
        .roundtrip(&mut state)
        .context("initial Wayland roundtrip")?;
    let _ = ready.send(Ok(()));

    loop {
        if process_commands(&commands, &mut state, &qh)? {
            break;
        }
        queue
            .dispatch_pending(&mut state)
            .context("dispatching Wayland events")?;
        request_capture(&mut state, &qh);
        queue.flush().context("flushing Wayland requests")?;
        wait_for_wayland(&queue)?;
    }
    Ok(())
}

fn first_output<State>(
    globals: &wayland_client::globals::GlobalList,
    qh: &QueueHandle<State>,
) -> Result<WlOutput>
where
    State: Dispatch<WlOutput, ()> + 'static,
{
    let global = globals
        .contents()
        .with_list(|list| {
            list.iter()
                .find(|global| global.interface == "wl_output")
                .cloned()
        })
        .context("compositor did not advertise a wl_output")?;
    Ok(globals
        .registry()
        .bind(global.name, global.version.min(4), qh, ()))
}

fn first_seat<State>(
    globals: &wayland_client::globals::GlobalList,
    qh: &QueueHandle<State>,
) -> Option<WlSeat>
where
    State: Dispatch<WlSeat, ()> + 'static,
{
    let global = globals.contents().with_list(|list| {
        list.iter()
            .find(|global| global.interface == "wl_seat")
            .cloned()
    })?;
    Some(
        globals
            .registry()
            .bind(global.name, global.version.min(5), qh, ()),
    )
}

fn process_commands(
    commands: &Receiver<DesktopCommand>,
    state: &mut WaylandState,
    qh: &QueueHandle<WaylandState>,
) -> Result<bool> {
    while let Ok(command) = commands.try_recv() {
        match command {
            DesktopCommand::Start => {
                if !state.streaming {
                    state.streaming = true;
                    state.stream_has_frame = false;
                    state.next_stream_capture = Instant::now();
                }
                request_capture(state, qh);
            }
            DesktopCommand::Stop => {
                state.streaming = false;
                state.stream_has_frame = false;
                if state.screenshot_waiter.is_none() {
                    cancel_capture(state);
                }
            }
            DesktopCommand::Mouse {
                action,
                x,
                y,
                button,
                reply,
            } => {
                let result = send_mouse(state, action, x, y, button);
                let _ = reply.send(result.map_err(|err| err.to_string()));
            }
            DesktopCommand::Wheel { dx, dy, reply } => {
                let result = send_wheel(state, dx, dy);
                let _ = reply.send(result.map_err(|err| err.to_string()));
            }
            DesktopCommand::Screenshot(reply) => {
                if state.screenshot_waiter.is_some() {
                    let _ = reply.send(Err("desktop screenshot already in progress".into()));
                    continue;
                }
                state.screenshot_waiter = Some(reply);
                if state.damage_copy {
                    cancel_capture(state);
                }
                request_capture(state, qh);
            }
            DesktopCommand::Shutdown => return Ok(true),
        }
    }
    Ok(false)
}

fn request_capture(state: &mut WaylandState, qh: &QueueHandle<WaylandState>) {
    if (!state.streaming && state.screenshot_waiter.is_none())
        || state.frame.is_some()
        || state.buffer.is_some()
    {
        return;
    }
    let screenshot = state.screenshot_waiter.is_some();
    if !screenshot && Instant::now() < state.next_stream_capture {
        return;
    }
    state.pending = None;
    state.invert_y = false;
    state.damage_copy = state.streaming && state.stream_has_frame && !screenshot;
    state.frame = Some(state.screencopy.capture_output(1, &state.output, qh, ()));
}

fn cancel_capture(state: &mut WaylandState) {
    if let Some(frame) = state.frame.take() {
        frame.destroy();
    }
    state.buffer = None;
    state.pending = None;
    state.damage_copy = false;
}

fn send_mouse(
    state: &WaylandState,
    action: MouseAction,
    x: f64,
    y: f64,
    button: u32,
) -> Result<()> {
    if !x.is_finite() || !y.is_finite() {
        bail!("mouse coordinates must be finite");
    }
    let x = x.clamp(0.0, state.width.saturating_sub(1) as f64) as u32;
    let y = y.clamp(0.0, state.height.saturating_sub(1) as f64) as u32;
    let time = elapsed_millis(state.started);
    state.pointer.motion_absolute(
        time,
        x,
        y,
        state.width.saturating_sub(1),
        state.height.saturating_sub(1),
    );
    match action {
        MouseAction::Down => state.pointer.button(time, button, ButtonState::Pressed),
        MouseAction::Up => state.pointer.button(time, button, ButtonState::Released),
        MouseAction::Move => {}
    }
    state.pointer.frame();
    Ok(())
}

fn send_wheel(state: &WaylandState, dx: f64, dy: f64) -> Result<()> {
    if !dx.is_finite() || !dy.is_finite() {
        bail!("wheel deltas must be finite");
    }
    let time = elapsed_millis(state.started);
    if dx != 0.0 {
        state.pointer.axis(time, Axis::HorizontalScroll, dx);
    }
    if dy != 0.0 {
        state.pointer.axis(time, Axis::VerticalScroll, dy);
    }
    state.pointer.frame();
    Ok(())
}

fn elapsed_millis(started: Instant) -> u32 {
    started.elapsed().as_millis().min(u32::MAX as u128) as u32
}

fn wait_for_wayland<State>(queue: &EventQueue<State>) -> Result<()> {
    let Some(guard) = queue.prepare_read() else {
        return Ok(());
    };
    let mut pollfd = libc::pollfd {
        fd: guard.connection_fd().as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let result = unsafe { libc::poll(&mut pollfd, 1, WAYLAND_POLL_TIMEOUT_MS) };
    if result < 0 {
        let err = io::Error::last_os_error();
        if err.kind() == io::ErrorKind::Interrupted {
            return Ok(());
        }
        return Err(err).context("polling Wayland socket");
    }
    if result > 0 && pollfd.revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0 {
        guard.read().context("reading Wayland socket")?;
    }
    Ok(())
}

struct WaylandState {
    output: WlOutput,
    shm: WlShm,
    screencopy: ZwlrScreencopyManagerV1,
    pointer: ZwlrVirtualPointerV1,
    frame: Option<ZwlrScreencopyFrameV1>,
    buffer: Option<ShmBuffer>,
    pending: Option<BufferDescription>,
    invert_y: bool,
    streaming: bool,
    stream_has_frame: bool,
    damage_copy: bool,
    next_stream_capture: Instant,
    screenshot_waiter: Option<SyncSender<FrameResult>>,
    encoder: EncoderWorker,
    width: u32,
    height: u32,
    started: Instant,
}

struct BufferDescription {
    format: Format,
    width: u32,
    height: u32,
    stride: u32,
}

struct ShmBuffer {
    proxy: WlBuffer,
    _file: File,
    mapping: Mmap,
    format: Format,
    width: u32,
    height: u32,
    stride: u32,
    released: bool,
    ready: bool,
}

impl Drop for ShmBuffer {
    fn drop(&mut self) {
        self.proxy.destroy();
    }
}

struct Mmap {
    ptr: *mut u8,
    len: usize,
}

impl Mmap {
    fn new(file: &File, len: usize) -> Result<Self> {
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error()).context("mapping Wayland buffer");
        }
        Ok(Self {
            ptr: ptr.cast(),
            len,
        })
    }

    fn bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl Drop for Mmap {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr.cast(), self.len);
        }
    }
}

unsafe impl Send for Mmap {}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for WaylandState {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlOutput, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &WlOutput,
        _: wayland_client::protocol::wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSeat, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &WlSeat,
        _: wayland_client::protocol::wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlShm, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &WlShm,
        _: wayland_client::protocol::wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlShmPool, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &WlShmPool,
        _: wayland_client::protocol::wl_shm_pool::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlBuffer, ()> for WaylandState {
    fn event(
        state: &mut Self,
        proxy: &WlBuffer,
        event: wayland_client::protocol::wl_buffer::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if !matches!(event, wayland_client::protocol::wl_buffer::Event::Release) {
            return;
        }
        let cleanup = state
            .buffer
            .as_ref()
            .is_some_and(|buffer| buffer.proxy == *proxy && buffer.ready);
        if let Some(buffer) = state
            .buffer
            .as_mut()
            .filter(|buffer| buffer.proxy == *proxy)
        {
            buffer.released = true;
        }
        if cleanup {
            state.buffer = None;
            request_capture(state, qh);
        }
    }
}

impl Dispatch<ZwlrScreencopyManagerV1, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &ZwlrScreencopyManagerV1,
        _: wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrVirtualPointerManagerV1, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &ZwlrVirtualPointerManagerV1,
        _: wayland_protocols_wlr::virtual_pointer::v1::client::zwlr_virtual_pointer_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrVirtualPointerV1, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &ZwlrVirtualPointerV1,
        _: wayland_protocols_wlr::virtual_pointer::v1::client::zwlr_virtual_pointer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrScreencopyFrameV1, ()> for WaylandState {
    fn event(
        state: &mut Self,
        proxy: &ZwlrScreencopyFrameV1,
        event: zwlr_screencopy_frame_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_screencopy_frame_v1::Event::Buffer {
                format: WEnum::Value(format),
                width,
                height,
                stride,
            } if is_supported_format(format)
                && (state.pending.is_none() || format == Format::Xrgb8888) =>
            {
                state.pending = Some(BufferDescription {
                    format,
                    width,
                    height,
                    stride,
                });
            }
            zwlr_screencopy_frame_v1::Event::Flags { flags } => {
                state.invert_y = matches!(
                    flags,
                    WEnum::Value(flags)
                        if flags.contains(zwlr_screencopy_frame_v1::Flags::YInvert)
                );
            }
            zwlr_screencopy_frame_v1::Event::BufferDone => {
                let Some(description) = state.pending.take() else {
                    fail_frame(state, proxy, "compositor offered no supported shm format");
                    return;
                };
                match create_shm_buffer(state, &description, qh) {
                    Ok(buffer) => {
                        if state.damage_copy {
                            proxy.copy_with_damage(&buffer.proxy);
                        } else {
                            proxy.copy(&buffer.proxy);
                        }
                        state.buffer = Some(buffer);
                    }
                    Err(err) => fail_frame(state, proxy, &err.to_string()),
                }
            }
            zwlr_screencopy_frame_v1::Event::Ready { .. } => {
                let result = state
                    .buffer
                    .as_ref()
                    .map(|buffer| copy_frame(buffer, state.invert_y))
                    .unwrap_or_else(|| Err(anyhow!("Wayland frame was ready without a buffer")));
                match result {
                    Ok(frame) => {
                        if state.streaming {
                            state.stream_has_frame = true;
                            state.next_stream_capture =
                                Instant::now().checked_add(STREAM_FRAME_INTERVAL).unwrap();
                        }
                        state.encoder.submit(EncodeJob {
                            frame,
                            stream: state.streaming,
                            screenshot: state.screenshot_waiter.take(),
                        });
                    }
                    Err(err) => {
                        if let Some(waiter) = state.screenshot_waiter.take() {
                            let _ = waiter.send(Err(err.to_string()));
                        }
                    }
                }
                proxy.destroy();
                state.frame = None;
                state.damage_copy = false;
                if let Some(buffer) = state.buffer.as_mut() {
                    buffer.ready = true;
                    if buffer.released {
                        state.buffer = None;
                        request_capture(state, qh);
                    }
                }
            }
            zwlr_screencopy_frame_v1::Event::Failed => {
                fail_frame(state, proxy, "compositor failed to copy the desktop frame");
            }
            _ => {}
        }
    }
}

fn fail_frame(state: &mut WaylandState, frame: &ZwlrScreencopyFrameV1, message: &str) {
    if let Some(waiter) = state.screenshot_waiter.take() {
        let _ = waiter.send(Err(message.to_string()));
    }
    frame.destroy();
    state.frame = None;
    state.buffer = None;
    state.pending = None;
    state.damage_copy = false;
}

fn is_supported_format(format: Format) -> bool {
    matches!(format, Format::Xrgb8888 | Format::Argb8888)
}

fn create_shm_buffer(
    state: &WaylandState,
    description: &BufferDescription,
    qh: &QueueHandle<WaylandState>,
) -> Result<ShmBuffer> {
    let size = (description.stride as usize)
        .checked_mul(description.height as usize)
        .context("Wayland buffer size overflow")?;
    let file = memfd(size)?;
    let mapping = Mmap::new(&file, size)?;
    let pool = state.shm.create_pool(file.as_fd(), size as i32, qh, ());
    let proxy = pool.create_buffer(
        0,
        description.width as i32,
        description.height as i32,
        description.stride as i32,
        description.format,
        qh,
        (),
    );
    pool.destroy();
    Ok(ShmBuffer {
        proxy,
        _file: file,
        mapping,
        format: description.format,
        width: description.width,
        height: description.height,
        stride: description.stride,
        released: false,
        ready: false,
    })
}

fn memfd(size: usize) -> Result<File> {
    let name = std::ffi::CString::new("lumen-wayland-frame").expect("static memfd name");
    let fd = unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return Err(io::Error::last_os_error()).context("creating Wayland shm memfd");
    }
    let file = unsafe { File::from_raw_fd(fd) };
    if let Err(err) = file.set_len(size as u64) {
        return Err(err).context("sizing Wayland shm memfd");
    }
    Ok(file)
}

fn copy_frame(buffer: &ShmBuffer, invert_y: bool) -> Result<RawFrame> {
    let expected = (buffer.stride as usize)
        .checked_mul(buffer.height as usize)
        .context("Wayland frame size overflow")?;
    if buffer.mapping.bytes().len() < expected {
        bail!("Wayland frame buffer is shorter than its dimensions");
    }
    Ok(RawFrame {
        pixels: buffer.mapping.bytes()[..expected].to_vec(),
        format: buffer.format,
        width: buffer.width,
        height: buffer.height,
        stride: buffer.stride,
        invert_y,
    })
}

fn convert_frame(frame: &RawFrame) -> Result<Vec<u8>> {
    let row_len = frame.width as usize * 3;
    let mut rgb = vec![0u8; row_len * frame.height as usize];
    for y in 0..frame.height as usize {
        let source_y = if frame.invert_y {
            frame.height as usize - 1 - y
        } else {
            y
        };
        let source_row = &frame.pixels[source_y * frame.stride as usize..];
        let target_row = &mut rgb[y * row_len..(y + 1) * row_len];
        for x in 0..frame.width as usize {
            let source_pixel = &source_row[x * 4..x * 4 + 4];
            let target_pixel = &mut target_row[x * 3..x * 3 + 3];
            match frame.format {
                Format::Xrgb8888 | Format::Argb8888 => {
                    target_pixel.copy_from_slice(&[
                        source_pixel[2],
                        source_pixel[1],
                        source_pixel[0],
                    ]);
                }
                _ => bail!("unsupported Wayland shm format"),
            }
        }
    }
    Ok(rgb)
}

fn encode_jpeg(rgb: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    let mut encoded = Vec::new();
    Encoder::new(&mut encoded, 70)
        .encode(rgb, width as u16, height as u16, ColorType::Rgb)
        .context("JPEG encoding")?;
    Ok(encoded)
}

fn encode_png(rgb: &[u8], width: u32, height: u32) -> FrameResult {
    let mut encoded = Vec::new();
    let mut encoder = png::Encoder::new(&mut encoded, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|err| err.to_string())?;
    writer
        .write_image_data(rgb)
        .map_err(|err| err.to_string())?;
    drop(writer);
    Ok(encoded)
}

/// Terminate a child the session may not have started.
async fn terminate_if_running(child: &mut Option<Child>) {
    if let Some(child) = child.as_mut() {
        terminate_process_group(child).await;
    }
}

async fn terminate_process_group(child: &mut Child) {
    let Some(pid) = child.id() else {
        return;
    };
    let pgid = pid as i32;
    unsafe {
        libc::kill(-pgid, libc::SIGTERM);
    }
    for _ in 0..20 {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    unsafe {
        libc::kill(-pgid, libc::SIGKILL);
    }
    let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_bgra_pixels_and_inverts_rows() {
        let frame = RawFrame {
            pixels: vec![3, 2, 1, 0, 6, 5, 4, 0],
            format: Format::Xrgb8888,
            width: 1,
            height: 2,
            stride: 4,
            invert_y: true,
        };

        assert_eq!(convert_frame(&frame).unwrap(), [4, 5, 6, 1, 2, 3]);
    }
}
