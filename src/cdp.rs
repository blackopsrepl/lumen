use anyhow::{anyhow, Context, Result};
use base64::Engine as _;
use chromiumoxide::cdp::browser_protocol::emulation::{
    SetDeviceMetricsOverrideParams, SetPageScaleFactorParams,
};
use chromiumoxide::cdp::browser_protocol::fetch::{
    self, ContinueRequestParams, EventRequestPaused, FailRequestParams,
};
use chromiumoxide::cdp::browser_protocol::input::{
    DispatchMouseEventParams, DispatchMouseEventType, InsertTextParams, MouseButton,
};
use chromiumoxide::cdp::browser_protocol::network::{ErrorReason, ResourceType};
use chromiumoxide::cdp::browser_protocol::page::{
    CaptureScreenshotFormat, CaptureScreenshotParams, EnableParams, ScreencastFrameAckParams,
    StartScreencastFormat, StartScreencastParams, StopScreencastParams,
};
use chromiumoxide::{Browser, Page};
use futures::{SinkExt, StreamExt};
use serde::Serialize;
use std::collections::hash_map::{Entry, HashMap};
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

/// One open tab in an agent's browser, addressed by its position.
#[derive(Debug, Clone, Serialize)]
pub struct TabInfo {
    pub index: usize,
    pub url: String,
    pub title: String,
    /// Whether this is the page Lumen drives and screencasts.
    pub active: bool,
}

/// A live CDP connection to one agent's Chromium, bound to one managed page.
///
/// The managed page is the one the view plane screencasts and the control plane
/// drives. Tab activation replaces it so those planes stay together.
pub struct CdpSession {
    pub browser: Browser,
    /// The one managed page. Held for the whole of every operation that
    /// resolves "the current page" and then acts on it, so a tab switch can
    /// never slip in between and leave a command driving a hidden page.
    page: Mutex<Page>,
    handler: JoinHandle<()>,
    handler_alive: Arc<AtomicBool>,
    policy: crate::config::Policy,
    policy_guards: Mutex<HashMap<String, JoinHandle<()>>>,
    navigation: Mutex<()>,
}

#[derive(Debug)]
pub struct NavigationBlocked {
    pub url: String,
}

impl fmt::Display for NavigationBlocked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "navigation blocked by policy: {}", self.url)
    }
}

impl std::error::Error for NavigationBlocked {}

#[derive(Debug)]
pub struct OnlyManagedTab;

impl fmt::Display for OnlyManagedTab {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("cannot close the only managed tab")
    }
}

impl std::error::Error for OnlyManagedTab {}

impl CdpSession {
    pub async fn connect(endpoint: &str) -> Result<Arc<Self>> {
        Self::connect_with_policy(endpoint, crate::config::Policy::default()).await
    }

    pub async fn connect_with_policy(
        endpoint: &str,
        policy: crate::config::Policy,
    ) -> Result<Arc<Self>> {
        let (browser, mut handler) = Browser::connect(endpoint.to_string())
            .await
            .with_context(|| format!("connecting to CDP at {endpoint}"))?;

        let handler_alive = Arc::new(AtomicBool::new(true));
        let handler_status = handler_alive.clone();
        let handler = tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                if let Err(err) = event {
                    tracing::debug!("cdp handler error: {err}");
                }
            }
            handler_status.store(false, Ordering::SeqCst);
        });

        // Keep exactly one page so the viewer, the agent's `playwright-cli`,
        // and Lumen's own CDP calls all target the same tab. Chromium opens
        // startup pages asynchronously, so reap extras until the set is stable.
        let mut pages = browser.pages().await?;
        if pages.is_empty() {
            pages.push(
                browser
                    .new_page("about:blank")
                    .await
                    .context("opening the agent page")?,
            );
        }
        let page = pages.remove(0);
        let keep = page.target_id().inner().clone();
        for other in pages {
            let _ = other.close().await;
        }
        for _ in 0..12 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let extras: Vec<Page> = browser
                .pages()
                .await?
                .into_iter()
                .filter(|candidate| candidate.target_id().inner() != &keep)
                .collect();
            if extras.is_empty() {
                break;
            }
            for extra in extras {
                let _ = extra.close().await;
            }
        }

        page.execute(EnableParams::builder().build())
            .await
            .context("Page.enable")?;

        let mut policy_guards = HashMap::new();
        if let Some(guard) = install_navigation_policy(&page, &policy).await? {
            policy_guards.insert(page.target_id().inner().clone(), guard);
        }

        Ok(Arc::new(Self {
            browser,
            page: Mutex::new(page),
            handler,
            handler_alive,
            policy,
            policy_guards: Mutex::new(policy_guards),
            navigation: Mutex::new(()),
        }))
    }

    /// A snapshot of the managed page. Only for callers that need an owned
    /// `Page` outside a serialized operation (the view plane); control-plane
    /// operations must act under the lock instead.
    pub async fn current_page(&self) -> Page {
        self.page.lock().await.clone()
    }

    pub fn is_alive(&self) -> bool {
        self.handler_alive.load(Ordering::SeqCst)
    }

    /// Pin the layout viewport to an exact CSS-pixel size, independent of any
    /// window chrome. This is the codex-style viewport override primitive.
    pub async fn set_viewport(&self, width: u32, height: u32) -> Result<()> {
        let params = SetDeviceMetricsOverrideParams::builder()
            .width(width as i64)
            .height(height as i64)
            .device_scale_factor(1.0)
            .mobile(false)
            .build()
            .map_err(|err| anyhow!(err))?;
        self.page
            .lock()
            .await
            .execute(params)
            .await
            .context("Emulation.setDeviceMetricsOverride")?;
        Ok(())
    }

    pub async fn goto(&self, url: &str) -> Result<()> {
        let page = self.page.lock().await;
        self.navigate_page(&page, url).await
    }

    /// Begin streaming the page at its native viewport size.
    pub async fn start_screencast(&self) -> Result<()> {
        let page = self.page.lock().await;
        self.start_screencast_on(&page).await
    }

    pub async fn start_screencast_on(&self, page: &Page) -> Result<()> {
        let params = StartScreencastParams::builder()
            .format(StartScreencastFormat::Jpeg)
            .quality(70)
            .build();
        page.execute(params).await.context("Page.startScreencast")?;
        Ok(())
    }

    pub async fn stop_screencast(&self) -> Result<()> {
        let page = self.page.lock().await;
        self.stop_screencast_on(&page).await
    }

    pub async fn stop_screencast_on(&self, page: &Page) -> Result<()> {
        page.execute(StopScreencastParams {})
            .await
            .context("Page.stopScreencast")?;
        Ok(())
    }

    /// Acknowledge a frame so Chromium releases the next one (flow control).
    pub async fn ack_frame(&self, session_id: i64) -> Result<()> {
        let page = self.page.lock().await;
        self.ack_frame_on(&page, session_id).await
    }

    pub async fn ack_frame_on(&self, page: &Page, session_id: i64) -> Result<()> {
        let params = ScreencastFrameAckParams::builder()
            .session_id(session_id)
            .build()
            .map_err(|err| anyhow!(err))?;
        page.execute(params)
            .await
            .context("Page.screencastFrameAck")?;
        Ok(())
    }

    pub async fn click(&self, x: f64, y: f64) -> Result<()> {
        let page = self.page.lock().await;
        self.mouse_on(
            &page,
            DispatchMouseEventType::MousePressed,
            x,
            y,
            MouseButton::Left,
        )
        .await?;
        self.mouse_on(
            &page,
            DispatchMouseEventType::MouseReleased,
            x,
            y,
            MouseButton::Left,
        )
        .await?;
        Ok(())
    }

    /// Dispatch one mouse event in CSS pixels relative to the viewport.
    pub async fn mouse(
        &self,
        kind: DispatchMouseEventType,
        x: f64,
        y: f64,
        button: MouseButton,
    ) -> Result<()> {
        let page = self.page.lock().await;
        self.mouse_on(&page, kind, x, y, button).await
    }

    async fn mouse_on(
        &self,
        page: &Page,
        kind: DispatchMouseEventType,
        x: f64,
        y: f64,
        button: MouseButton,
    ) -> Result<()> {
        let params = DispatchMouseEventParams::builder()
            .r#type(kind)
            .x(x)
            .y(y)
            .button(button)
            .click_count(1)
            .build()
            .map_err(|err| anyhow!(err))?;
        page.execute(params)
            .await
            .context("Input.dispatchMouseEvent")?;
        Ok(())
    }

    /// Dispatch a wheel event in CSS pixels relative to the viewport.
    pub async fn wheel(&self, x: f64, y: f64, delta_x: f64, delta_y: f64) -> Result<()> {
        let params = DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MouseWheel)
            .x(x)
            .y(y)
            .delta_x(delta_x)
            .delta_y(delta_y)
            .build()
            .map_err(|err| anyhow!(err))?;
        self.page
            .lock()
            .await
            .execute(params)
            .await
            .context("Input.dispatchMouseEvent (wheel)")?;
        Ok(())
    }

    pub async fn insert_text(&self, text: &str) -> Result<()> {
        let params = InsertTextParams::builder()
            .text(text)
            .build()
            .map_err(|err| anyhow!(err))?;
        self.page
            .lock()
            .await
            .execute(params)
            .await
            .context("Input.insertText")?;
        Ok(())
    }

    /// Multiply the visual page scale (pinch-zoom semantics; layout untouched).
    pub async fn set_page_scale(&self, scale: f64) -> Result<()> {
        let params = SetPageScaleFactorParams::builder()
            .page_scale_factor(scale)
            .build()
            .map_err(|err| anyhow!(err))?;
        self.page
            .lock()
            .await
            .execute(params)
            .await
            .context("Emulation.setPageScaleFactor")?;
        Ok(())
    }

    /// Capture the page as PNG, optionally beyond the visible viewport.
    pub async fn screenshot(&self, full_page: bool) -> Result<Vec<u8>> {
        let mut builder = CaptureScreenshotParams::builder()
            .format(CaptureScreenshotFormat::Png)
            .from_surface(true);
        if full_page {
            builder = builder.capture_beyond_viewport(true);
        }
        let params = builder.build();
        let response = self
            .page
            .lock()
            .await
            .execute(params)
            .await
            .context("Page.captureScreenshot")?;
        let encoded: &str = response.result.data.as_ref();
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .context("decode screenshot")
    }

    /// Every open tab, in browser order.
    ///
    /// A target that has just been destroyed can still appear in the browser's
    /// page list for a moment, and asking it for its URL or title fails. That
    /// must not fail the whole enumeration: the index has to stay aligned with
    /// the list tab activation and closing operate on, so a vanishing target
    /// contributes empty fields instead of taking the request down with it.
    pub async fn tabs(&self) -> Result<Vec<TabInfo>> {
        let pages = self.browser.pages().await?;
        let active = self.target_id().await;
        let mut tabs = Vec::with_capacity(pages.len());
        for (index, page) in pages.iter().enumerate() {
            let url = page.url().await.ok().flatten().unwrap_or_default();
            let title = page.get_title().await.ok().flatten().unwrap_or_default();
            tabs.push(TabInfo {
                index,
                url,
                title,
                active: page.target_id().inner() == &active,
            });
        }
        Ok(tabs)
    }

    pub async fn open_tab(&self, url: &str) -> Result<()> {
        let page = self.browser.new_page("about:blank").await?;
        let target_id = page.target_id().inner().clone();
        page.execute(EnableParams::builder().build()).await?;
        self.ensure_policy_guard(&page).await?;
        if let Err(err) = self.navigate_page(&page, url).await {
            if page.close().await.is_ok() {
                self.remove_policy_guard(&target_id).await;
            }
            return Err(err);
        }
        page.bring_to_front().await?;
        let mut current = self.page.lock().await;
        if current.target_id().inner() == &target_id {
            return Ok(());
        }
        let old = std::mem::replace(&mut *current, page);
        let _ = self.stop_screencast_on(&old).await;
        Ok(())
    }

    /// Make a tab the browser opened on its own (a `target=_blank` link, a
    /// popup) the managed tab, so the view and control planes follow what the
    /// browser actually shows instead of stranding on the tab the human left.
    /// Returns whether the managed tab changed.
    pub async fn adopt_opened_page(&self, target_id: &str) -> Result<bool> {
        let Some(page) = self
            .browser
            .pages()
            .await?
            .into_iter()
            .find(|candidate| candidate.target_id().inner() == target_id)
        else {
            return Ok(false);
        };
        self.ensure_policy_guard(&page).await?;
        page.bring_to_front().await?;
        let mut current = self.page.lock().await;
        if current.target_id().inner() == target_id {
            return Ok(false);
        }
        let old = std::mem::replace(&mut *current, page);
        let _ = self.stop_screencast_on(&old).await;
        Ok(true)
    }

    pub async fn activate_tab(&self, index: usize) -> Result<bool> {
        let Some(page) = self.browser.pages().await?.into_iter().nth(index) else {
            return Ok(false);
        };
        let target_id = page.target_id().inner().clone();
        self.ensure_policy_guard(&page).await?;
        page.bring_to_front().await?;
        let mut current = self.page.lock().await;
        if current.target_id().inner() == &target_id {
            return Ok(true);
        }
        let old = std::mem::replace(&mut *current, page);
        let _ = self.stop_screencast_on(&old).await;
        Ok(true)
    }

    pub async fn close_tab(&self, index: usize) -> Result<Option<bool>> {
        let pages = self.browser.pages().await?;
        let Some(page) = pages.into_iter().nth(index) else {
            return Ok(None);
        };
        let target_id = page.target_id().inner().clone();
        let mut current = self.page.lock().await;
        if current.target_id().inner() != &target_id {
            drop(current);
            page.close().await?;
            self.remove_policy_guard(&target_id).await;
            return Ok(Some(false));
        }
        let pages = self.browser.pages().await?;
        let Some(replacement) = pages
            .into_iter()
            .find(|candidate| candidate.target_id().inner() != &target_id)
        else {
            return Err(OnlyManagedTab.into());
        };
        self.ensure_policy_guard(&replacement).await?;
        replacement.bring_to_front().await?;
        let old = std::mem::replace(&mut *current, replacement);
        let _ = self.stop_screencast_on(&old).await;
        page.close().await?;
        self.remove_policy_guard(&target_id).await;
        Ok(Some(true))
    }

    /// The DevTools target id of the shared page.
    pub async fn target_id(&self) -> String {
        self.current_page().await.target_id().inner().clone()
    }

    /// Replace the managed page after its target disappears — an agent closed
    /// it over CDP, or it crashed. Returns whether recovery happened.
    ///
    /// Lumen's own `close_tab` swaps the managed page *before* closing the old
    /// target, so when the destruction event for that target arrives the dead
    /// target is no longer managed and this is a no-op.
    pub async fn recover_managed_page(&self, dead_target: &str) -> Result<bool> {
        if !self.is_alive() {
            return Ok(false);
        }
        let mut current = self.page.lock().await;
        if current.target_id().inner() != dead_target {
            return Ok(false);
        }
        let mut candidates = Vec::new();
        for candidate in self.browser.pages().await? {
            if candidate.target_id().inner() == dead_target {
                continue;
            }
            // Skip anything that cannot answer for itself: a target on its way
            // out is worse than no candidate, since adopting it would strand
            // the session again.
            if candidate.url().await.is_ok() {
                candidates.push(candidate);
            }
        }
        let replacement = match candidates.into_iter().next() {
            Some(page) => page,
            None => self.browser.new_page("about:blank").await?,
        };
        let _ = replacement.execute(EnableParams::builder().build()).await;
        self.ensure_policy_guard(&replacement).await?;
        let _ = replacement.bring_to_front().await;
        let old = std::mem::replace(&mut *current, replacement);
        let _ = self.stop_screencast_on(&old).await;
        Ok(true)
    }

    /// Send a raw CDP command to the shared page and return its result.
    ///
    /// This is the escape hatch for protocol features Lumen has not wrapped;
    /// it opens a short-lived DevTools socket to the page target. Every step is
    /// bounded, because an unresponsive browser or half-open socket would
    /// otherwise leave the caller's request pending forever.
    pub async fn raw_cdp(
        &self,
        endpoint: &str,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let ws_url = page_ws_url(endpoint, &self.target_id().await).await?;
        let (mut socket, _) = tokio::time::timeout(
            CDP_CONNECT_TIMEOUT,
            tokio_tungstenite::connect_async(&ws_url),
        )
        .await
        .with_context(|| format!("connecting to {ws_url} timed out"))?
        .with_context(|| format!("connecting to {ws_url}"))?;

        let request = serde_json::json!({ "id": 1, "method": method, "params": params });
        tokio::time::timeout(CDP_COMMAND_TIMEOUT, async {
            socket
                .send(Message::Text(request.to_string().into()))
                .await
                .context("sending the CDP command")?;

            while let Some(frame) = socket.next().await {
                let frame = frame.context("reading the CDP response")?;
                if let Message::Text(text) = frame {
                    let value: serde_json::Value =
                        serde_json::from_str(text.as_str()).context("parsing the CDP response")?;
                    if value.get("id").and_then(serde_json::Value::as_i64) == Some(1) {
                        if let Some(error) = value.get("error") {
                            anyhow::bail!("CDP error: {error}");
                        }
                        return Ok(value
                            .get("result")
                            .cloned()
                            .unwrap_or(serde_json::Value::Null));
                    }
                }
            }
            anyhow::bail!("CDP socket closed before a response")
        })
        .await
        .with_context(|| format!("CDP command '{method}' timed out"))?
    }

    /// Give a tab Lumen is about to mediate a navigation-policy guard, so
    /// every document navigation on it is checked regardless of which CDP
    /// session issues it. Idempotent per target: if a guard already exists,
    /// the duplicate task is discarded.
    async fn ensure_policy_guard(&self, page: &Page) -> Result<()> {
        let Some(guard) = install_navigation_policy(page, &self.policy).await? else {
            return Ok(());
        };
        let target_id = page.target_id().inner().clone();
        let mut guards = self.policy_guards.lock().await;
        match guards.entry(target_id) {
            Entry::Occupied(_) => guard.abort(),
            Entry::Vacant(entry) => {
                entry.insert(guard);
            }
        }
        Ok(())
    }

    /// Stop a closed tab's guard so its interception task does not leak.
    async fn remove_policy_guard(&self, target_id: &str) {
        if let Some(guard) = self.policy_guards.lock().await.remove(target_id) {
            guard.abort();
        }
    }

    async fn navigate_page(&self, page: &Page, url: &str) -> Result<()> {
        let _navigation = self.navigation.lock().await;
        match page.goto(url).await {
            Ok(_) => Ok(()),
            Err(err) => {
                // The per-tab Fetch guard fails a blocked document request in
                // the browser; report that as a policy decision, not a CDP
                // fault. Every other failure is a real navigation error.
                if format!("{err}").contains("ERR_BLOCKED_BY_CLIENT") {
                    return Err(NavigationBlocked {
                        url: url.to_string(),
                    }
                    .into());
                }
                Err(err).context("Page.navigate")
            }
        }
    }
}

/// Install the per-tab enforcement of the navigation policy for one target.
///
/// A `Fetch.enable` interception (document requests, request stage) pauses
/// every document navigation on this target — including navigations issued by
/// other CDP sessions, such as an agent's own playwright connection — and
/// continues or fails each one according to `policy.check`. Blocked
/// navigations surface in the browser as `net::ERR_BLOCKED_BY_CLIENT`, which
/// Lumen's own navigate call translates into a `403`.
///
/// The returned task must stay alive for the interception to be answered;
/// callers keep it in `policy_guards` keyed by target id and abort it when
/// the tab closes. Returns `None` for an unrestricted policy, leaving the
/// tab unintercepted.
async fn install_navigation_policy(
    page: &Page,
    policy: &crate::config::Policy,
) -> Result<Option<JoinHandle<()>>> {
    if !policy.is_restricted() {
        return Ok(None);
    }

    let mut requests = page
        .event_listener::<EventRequestPaused>()
        .await
        .context("Fetch.requestPaused listener")?;
    let pattern = fetch::RequestPattern::builder()
        .resource_type(ResourceType::Document)
        .request_stage(fetch::RequestStage::Request)
        .build();
    page.execute(fetch::EnableParams::builder().pattern(pattern).build())
        .await
        .context("Fetch.enable")?;

    let page = page.clone();
    let policy = policy.clone();
    Ok(Some(tokio::spawn(async move {
        while let Some(event) = requests.next().await {
            if policy.check(&event.request.url).is_ok() {
                if let Err(err) = page
                    .execute(ContinueRequestParams::new(event.request_id.clone()))
                    .await
                {
                    tracing::debug!("continuing intercepted navigation failed: {err}");
                }
            } else {
                tracing::warn!(url = %event.request.url, "blocked browser navigation by policy");
                if let Err(err) = page
                    .execute(FailRequestParams::new(
                        event.request_id.clone(),
                        ErrorReason::BlockedByClient,
                    ))
                    .await
                {
                    tracing::debug!("failing intercepted navigation failed: {err}");
                }
            }
        }
    })))
}

/// How long the DevTools HTTP target list and websocket handshake may take.
const CDP_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// How long one raw CDP command may take before it is abandoned.
const CDP_COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Resolve the DevTools websocket URL for one page target.
async fn page_ws_url(endpoint: &str, target_id: &str) -> Result<String> {
    page_ws_url_within(endpoint, target_id, CDP_CONNECT_TIMEOUT).await
}

async fn page_ws_url_within(
    endpoint: &str,
    target_id: &str,
    timeout: std::time::Duration,
) -> Result<String> {
    let targets: Vec<serde_json::Value> = tokio::time::timeout(timeout, async {
        let response = reqwest::get(format!("{endpoint}/json/list"))
            .await
            .context("fetching /json/list")?;
        response
            .json::<Vec<serde_json::Value>>()
            .await
            .context("parsing /json/list")
    })
    .await
    .context("timed out fetching /json/list")??;

    for target in targets {
        if target.get("id").and_then(serde_json::Value::as_str) == Some(target_id) {
            if let Some(url) = target
                .get("webSocketDebuggerUrl")
                .and_then(serde_json::Value::as_str)
            {
                return Ok(url.to_string());
            }
        }
    }
    anyhow::bail!("no DevTools websocket for target {target_id}")
}

impl Drop for CdpSession {
    fn drop(&mut self) {
        self.handler.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn target_list_fetch_gives_up_on_a_stalled_connection() {
        // A socket that accepts and then never answers, like a half-open
        // DevTools endpoint.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((socket, _)) = listener.accept().await {
                held.push(socket);
            }
        });

        let started = std::time::Instant::now();
        let result = page_ws_url_within(
            &format!("http://{addr}"),
            "target",
            Duration::from_millis(200),
        )
        .await;
        assert!(result.is_err());
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "should abandon the stalled fetch, took {:?}",
            started.elapsed()
        );
    }
}
