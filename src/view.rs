use crate::cdp::CdpSession;
use crate::desktop::DesktopSession;
use base64::Engine as _;
use bytes::Bytes;
use chromiumoxide::cdp::browser_protocol::page::EventScreencastFrame;
use futures::StreamExt;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use tokio::sync::{watch, Mutex};
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
    frames: watch::Sender<Option<Bytes>>,
    closed: watch::Sender<bool>,
    started: Arc<AtomicBool>,
    subscribers: AtomicUsize,
    lifecycle: Mutex<()>,
    pump: Mutex<Option<JoinHandle<()>>>,
    control: Mutex<Control>,
}

/// One viewer's claim on the shared capture source.
///
/// The receiver stores only the newest frame. Dropping the final subscription
/// releases the source even when its WebSocket ended through cancellation.
pub struct ViewSubscription {
    frames: watch::Receiver<Option<Bytes>>,
    closed: watch::Receiver<bool>,
    hub: Weak<ViewHub>,
    initial: bool,
}

impl ViewSubscription {
    pub async fn next_frame(&mut self) -> Option<Bytes> {
        if *self.closed.borrow() {
            return None;
        }
        if self.initial {
            self.initial = false;
            if let Some(frame) = self.frames.borrow_and_update().clone() {
                return Some(frame);
            }
        }
        loop {
            tokio::select! {
                changed = self.frames.changed() => changed.ok()?,
                changed = self.closed.changed() => {
                    changed.ok()?;
                    if *self.closed.borrow_and_update() {
                        return None;
                    }
                    continue;
                }
            }
            if let Some(frame) = self.frames.borrow_and_update().clone() {
                return Some(frame);
            }
        }
    }
}

impl Drop for ViewSubscription {
    fn drop(&mut self) {
        let Some(hub) = self.hub.upgrade() else {
            return;
        };
        let previous = hub.subscribers.fetch_sub(1, Ordering::SeqCst);
        debug_assert!(previous > 0, "view subscriber count underflow");
        if previous != 1 {
            return;
        }
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                hub.stop_if_unused().await;
            });
        } else if let ViewSource::Desktop(session) = &hub.source {
            let _ = session.set_streaming(false);
        }
    }
}

#[derive(Clone)]
enum ViewSource {
    Browser(Arc<CdpSession>),
    Desktop(Arc<DesktopSession>),
    #[cfg(test)]
    Test {
        starts: Arc<AtomicUsize>,
        stops: Arc<AtomicUsize>,
    },
}

impl ViewHub {
    pub fn new(session: Arc<CdpSession>) -> Self {
        let (frames, _) = watch::channel(None);
        let (closed, _) = watch::channel(false);
        Self {
            source: ViewSource::Browser(session),
            frames,
            closed,
            started: Arc::new(AtomicBool::new(false)),
            subscribers: AtomicUsize::new(0),
            lifecycle: Mutex::new(()),
            pump: Mutex::new(None),
            control: Mutex::new(Control::Agent),
        }
    }

    pub fn new_desktop(session: Arc<DesktopSession>) -> Self {
        let (frames, _) = watch::channel(None);
        let (closed, _) = watch::channel(false);
        Self {
            source: ViewSource::Desktop(session),
            frames,
            closed,
            started: Arc::new(AtomicBool::new(false)),
            subscribers: AtomicUsize::new(0),
            lifecycle: Mutex::new(()),
            pump: Mutex::new(None),
            control: Mutex::new(Control::Agent),
        }
    }

    /// Join the frame stream, starting capture if this is the first viewer.
    pub async fn subscribe(self: &Arc<Self>) -> ViewSubscription {
        self.subscribers.fetch_add(1, Ordering::SeqCst);
        let subscription = ViewSubscription {
            frames: self.frames.subscribe(),
            closed: self.closed.subscribe(),
            hub: Arc::downgrade(self),
            initial: true,
        };
        self.ensure_started().await;
        subscription
    }

    /// Suspend or resume capture while at least one viewer is connected.
    pub async fn set_visible(&self, visible: bool) {
        if visible && self.subscribers.load(Ordering::SeqCst) > 0 {
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

    /// Permanently close this hub and release every connected viewer.
    pub async fn shutdown(&self) {
        self.closed.send_replace(true);
        let _lifecycle = self.lifecycle.lock().await;
        self.stop_locked().await;
    }

    pub async fn control(&self) -> Control {
        *self.control.lock().await
    }

    pub async fn set_control(&self, owner: Control) {
        *self.control.lock().await = owner;
    }

    async fn ensure_started(&self) {
        let _lifecycle = self.lifecycle.lock().await;
        if *self.closed.borrow() || self.started.load(Ordering::SeqCst) {
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
            #[cfg(test)]
            ViewSource::Test { starts, .. } => {
                starts.fetch_add(1, Ordering::SeqCst);
                (None, None)
            }
        };

        let desktop_stream = match &self.source {
            ViewSource::Desktop(session) => Some(session.frames()),
            ViewSource::Browser(_) => None,
            #[cfg(test)]
            ViewSource::Test { .. } => None,
        };
        #[cfg(test)]
        let test_source = matches!(&self.source, ViewSource::Test { .. });
        let frames = self.frames.clone();
        let started = self.started.clone();
        let pump = tokio::spawn(async move {
            if let Some(mut stream) = stream {
                while let Some(frame) = stream.next().await {
                    let encoded: &str = frame.data.as_ref();
                    match base64::engine::general_purpose::STANDARD.decode(encoded) {
                        Ok(bytes) => {
                            frames.send_replace(Some(Bytes::from(bytes)));
                        }
                        Err(err) => tracing::warn!("screencast frame decode failed: {err}"),
                    }
                    if let Some((session, page)) = &page {
                        let _ = session.ack_frame_on(page, frame.session_id).await;
                    }
                }
            } else if let Some(mut stream) = desktop_stream {
                while stream.changed().await.is_ok() {
                    if let Some(frame) = stream.borrow_and_update().clone() {
                        frames.send_replace(Some(frame));
                    }
                }
            }
            #[cfg(test)]
            if test_source {
                std::future::pending::<()>().await;
            }
            started.store(false, Ordering::SeqCst);
        });
        self.started.store(true, Ordering::SeqCst);
        *self.pump.lock().await = Some(pump);
    }

    async fn stop_locked(&self) {
        if !self.started.swap(false, Ordering::SeqCst) {
            return;
        }
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
            #[cfg(test)]
            ViewSource::Test { stops, .. } => {
                stops.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    async fn stop_if_unused(&self) {
        let _lifecycle = self.lifecycle.lock().await;
        if self.subscribers.load(Ordering::SeqCst) == 0 {
            self.stop_locked().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn test_hub() -> (Arc<ViewHub>, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let starts = Arc::new(AtomicUsize::new(0));
        let stops = Arc::new(AtomicUsize::new(0));
        let (frames, _) = watch::channel(None);
        let (closed, _) = watch::channel(false);
        let hub = Arc::new(ViewHub {
            source: ViewSource::Test {
                starts: starts.clone(),
                stops: stops.clone(),
            },
            frames,
            closed,
            started: Arc::new(AtomicBool::new(false)),
            subscribers: AtomicUsize::new(0),
            lifecycle: Mutex::new(()),
            pump: Mutex::new(None),
            control: Mutex::new(Control::Agent),
        });
        (hub, starts, stops)
    }

    async fn wait_for(counter: &AtomicUsize, expected: usize) {
        tokio::time::timeout(Duration::from_secs(1), async {
            while counter.load(Ordering::SeqCst) != expected {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("counter did not reach expected value");
    }

    #[tokio::test]
    async fn capture_lives_from_first_until_last_subscriber() {
        let (hub, starts, stops) = test_hub();
        let first = hub.subscribe().await;
        let second = hub.subscribe().await;

        assert_eq!(starts.load(Ordering::SeqCst), 1);
        drop(first);
        tokio::task::yield_now().await;
        assert_eq!(stops.load(Ordering::SeqCst), 0);

        drop(second);
        wait_for(&stops, 1).await;

        let third = hub.subscribe().await;
        assert_eq!(starts.load(Ordering::SeqCst), 2);
        drop(third);
        wait_for(&stops, 2).await;
    }

    #[tokio::test]
    async fn subscription_keeps_only_the_latest_frame() {
        let (hub, _, _) = test_hub();
        let mut subscription = hub.subscribe().await;
        hub.frames.send_replace(Some(Bytes::from_static(b"one")));
        hub.frames.send_replace(Some(Bytes::from_static(b"two")));
        hub.frames.send_replace(Some(Bytes::from_static(b"three")));

        assert_eq!(subscription.next_frame().await.unwrap(), "three");
    }

    #[tokio::test]
    async fn shutdown_releases_subscribers() {
        let (hub, _, stops) = test_hub();
        let mut subscription = hub.subscribe().await;

        hub.shutdown().await;

        assert!(subscription.next_frame().await.is_none());
        assert_eq!(stops.load(Ordering::SeqCst), 1);
    }
}
