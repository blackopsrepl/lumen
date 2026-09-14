use crate::cdp::CdpSession;
use base64::Engine as _;
use chromiumoxide::cdp::browser_protocol::page::EventScreencastFrame;
use futures::StreamExt;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

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
    started: AtomicBool,
    control: Mutex<Control>,
}

impl ViewHub {
    pub fn new(session: Arc<CdpSession>) -> Self {
        let (frames, _) = broadcast::channel(8);
        Self {
            session,
            frames,
            started: AtomicBool::new(false),
            control: Mutex::new(Control::Agent),
        }
    }

    /// Join the frame stream, starting the screencast if this is the first viewer.
    pub async fn subscribe(&self) -> broadcast::Receiver<Arc<Vec<u8>>> {
        self.ensure_started().await;
        self.frames.subscribe()
    }

    pub async fn control(&self) -> Control {
        *self.control.lock().await
    }

    pub async fn set_control(&self, owner: Control) {
        *self.control.lock().await = owner;
    }

    async fn ensure_started(&self) {
        if self.started.swap(true, Ordering::SeqCst) {
            return;
        }

        let mut stream = match self
            .session
            .page
            .event_listener::<EventScreencastFrame>()
            .await
        {
            Ok(stream) => stream,
            Err(err) => {
                tracing::warn!("screencast listener failed: {err}");
                return;
            }
        };
        if let Err(err) = self.session.start_screencast().await {
            tracing::warn!("starting screencast failed: {err}");
            return;
        }

        let session = self.session.clone();
        let frames = self.frames.clone();
        tokio::spawn(async move {
            while let Some(frame) = stream.next().await {
                let encoded: &str = frame.data.as_ref();
                match base64::engine::general_purpose::STANDARD.decode(encoded) {
                    Ok(bytes) => {
                        let _ = frames.send(Arc::new(bytes));
                    }
                    Err(err) => tracing::warn!("screencast frame decode failed: {err}"),
                }
                let _ = session.ack_frame(frame.session_id).await;
            }
        });
    }
}
