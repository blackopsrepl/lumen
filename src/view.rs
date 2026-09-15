use crate::cdp::CdpSession;
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

/// Fans one browser's screencast out to any number of viewers.
///
/// The screencast starts on the first subscriber and streams the shared page at
/// its native size; every viewer sees the same frames, and the hub is the single
/// place that acknowledges frames back to Chromium for flow control.
pub struct ViewHub {
    session: Arc<CdpSession>,
    frames: broadcast::Sender<Arc<Vec<u8>>>,
    /// The most recent frame, so a viewer joining an idle page paints
    /// immediately instead of waiting for the content to change.
    latest: Arc<Mutex<Option<Arc<Vec<u8>>>>>,
    started: Arc<AtomicBool>,
    lifecycle: Mutex<()>,
    pump: Mutex<Option<JoinHandle<()>>>,
    control: Mutex<Control>,
}

impl ViewHub {
    pub fn new(session: Arc<CdpSession>) -> Self {
        let (frames, _) = broadcast::channel(8);
        Self {
            session,
            frames,
            latest: Arc::new(Mutex::new(None)),
            started: Arc::new(AtomicBool::new(false)),
            lifecycle: Mutex::new(()),
            pump: Mutex::new(None),
            control: Mutex::new(Control::Agent),
        }
    }

    /// Join the frame stream, starting the screencast if this is the first viewer.
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
        let page = self.session.current_page().await;
        let mut stream = match page.event_listener::<EventScreencastFrame>().await {
            Ok(stream) => stream,
            Err(err) => {
                tracing::warn!("screencast listener failed: {err}");
                return;
            }
        };
        if let Err(err) = self.session.start_screencast_on(&page).await {
            tracing::warn!("starting screencast failed: {err}");
            return;
        }

        let session = self.session.clone();
        let frames = self.frames.clone();
        let latest = self.latest.clone();
        let started = self.started.clone();
        let pump = tokio::spawn(async move {
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
                let _ = session.ack_frame_on(&page, frame.session_id).await;
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
        let _ = self.session.stop_screencast().await;
    }
}
