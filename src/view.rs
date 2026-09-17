use crate::cdp::CdpSession;
use crate::desktop::DesktopSession;
use base64::Engine as _;
use chromiumoxide::cdp::browser_protocol::page::EventScreencastFrame;
use futures::StreamExt;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tokio::task::JoinHandle;

/// Who currently owns the input path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Control {
    Agent,
    Human,
}

/// Fans one browser or desktop session's frames out to any number of viewers.
///
/// Capture starts on the first subscriber and streams the shared session at its
/// native size; every viewer sees the same frames, and browser frame events are
/// acknowledged by the hub for flow control.
pub struct ViewHub {
    source: ViewSource,
    frames: broadcast::Sender<Arc<Vec<u8>>>,
    /// The most recent frame, so a viewer joining an idle page paints
    /// immediately instead of waiting for the content to change.
    latest: Arc<Mutex<Option<Arc<Vec<u8>>>>>,
    started: Arc<AtomicBool>,
    lifecycle: Mutex<()>,
    pump: Mutex<Option<JoinHandle<()>>>,
    control: Mutex<Control>,
}

#[derive(Clone)]
enum ViewSource {
    Browser(Arc<CdpSession>),
    Desktop(Arc<DesktopSession>),
}

impl ViewHub {
    pub fn new(session: Arc<CdpSession>) -> Self {
        let (frames, _) = broadcast::channel(8);
        Self {
            source: ViewSource::Browser(session),
            frames,
            latest: Arc::new(Mutex::new(None)),
            started: Arc::new(AtomicBool::new(false)),
            lifecycle: Mutex::new(()),
            pump: Mutex::new(None),
            control: Mutex::new(Control::Agent),
        }
    }

    pub fn new_desktop(session: Arc<DesktopSession>) -> Self {
        let (frames, _) = broadcast::channel(8);
        Self {
            source: ViewSource::Desktop(session),
            frames,
            latest: Arc::new(Mutex::new(None)),
            started: Arc::new(AtomicBool::new(false)),
            lifecycle: Mutex::new(()),
            pump: Mutex::new(None),
            control: Mutex::new(Control::Agent),
        }
    }

    /// Join the frame stream, starting capture if this is the first viewer.
    pub async fn subscribe(&self) -> broadcast::Receiver<Arc<Vec<u8>>> {
        let receiver = self.frames.subscribe();
        self.ensure_started().await;
        receiver
    }

    /// The most recent frame, if any screencast has produced one.
    pub async fn latest_frame(&self) -> Option<Arc<Vec<u8>>> {
        self.latest.lock().await.clone()
    }

    /// Turn the screencast on or off. Off frees the browser from encoding frames
    /// nobody is watching.
    pub async fn set_visible(&self, visible: bool) {
        if visible {
            self.ensure_started().await;
        } else {
            let _lifecycle = self.lifecycle.lock().await;
            self.stop_locked().await;
        }
    }

    /// Rebind the screencast to the currently active page after a tab switch.
    pub async fn rebind(&self) {
        let _lifecycle = self.lifecycle.lock().await;
        if !self.started.load(Ordering::SeqCst) {
            return;
        }
        self.stop_locked().await;
        self.start_locked().await;
    }

    pub async fn control(&self) -> Control {
        *self.control.lock().await
    }

    pub async fn set_control(&self, owner: Control) {
        *self.control.lock().await = owner;
    }

    async fn ensure_started(&self) {
        let _lifecycle = self.lifecycle.lock().await;
        if self.started.load(Ordering::SeqCst) {
            return;
        }
        self.start_locked().await;
    }

    async fn start_locked(&self) {
        let (stream, page) = match &self.source {
            ViewSource::Browser(session) => {
                let page = session.current_page().await;
                let stream = match page.event_listener::<EventScreencastFrame>().await {
                    Ok(stream) => stream,
                    Err(err) => {
                        tracing::warn!("screencast listener failed: {err}");
                        return;
                    }
                };
                if let Err(err) = session.start_screencast_on(&page).await {
                    tracing::warn!("starting screencast failed: {err}");
                    return;
                }
                (Some(stream), Some((session.clone(), page)))
            }
            ViewSource::Desktop(session) => {
                if let Err(err) = session.set_streaming(true) {
                    tracing::warn!("starting desktop capture failed: {err}");
                    return;
                }
                (None, None)
            }
        };

        let desktop_stream = match &self.source {
            ViewSource::Desktop(session) => Some(session.frames()),
            ViewSource::Browser(_) => None,
        };
        let frames = self.frames.clone();
        let latest = self.latest.clone();
        let started = self.started.clone();
        let pump = tokio::spawn(async move {
            if let Some(mut stream) = stream {
                while let Some(frame) = stream.next().await {
                    let encoded: &str = frame.data.as_ref();
                    match base64::engine::general_purpose::STANDARD.decode(encoded) {
                        Ok(bytes) => {
                            let frame = Arc::new(bytes);
                            *latest.lock().await = Some(frame.clone());
                            let _ = frames.send(frame);
                        }
                        Err(err) => tracing::warn!("screencast frame decode failed: {err}"),
                    }
                    if let Some((session, page)) = &page {
                        let _ = session.ack_frame_on(page, frame.session_id).await;
                    }
                }
            } else if let Some(mut stream) = desktop_stream {
                while let Ok(frame) = stream.recv().await {
                    *latest.lock().await = Some(frame.clone());
                    let _ = frames.send(frame);
                }
            }
            started.store(false, Ordering::SeqCst);
        });
        self.started.store(true, Ordering::SeqCst);
        *self.pump.lock().await = Some(pump);
    }

    async fn stop_locked(&self) {
        self.started.store(false, Ordering::SeqCst);
        if let Some(pump) = self.pump.lock().await.take() {
            pump.abort();
        }
        match &self.source {
            ViewSource::Browser(session) => {
                let _ = session.stop_screencast().await;
            }
            ViewSource::Desktop(session) => {
                let _ = session.set_streaming(false);
            }
        }
    }
}
