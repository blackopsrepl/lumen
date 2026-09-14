use anyhow::{anyhow, Context, Result};
use base64::Engine as _;
use chromiumoxide::cdp::browser_protocol::emulation::{
    SetDeviceMetricsOverrideParams, SetPageScaleFactorParams,
};
use chromiumoxide::cdp::browser_protocol::input::{
    DispatchMouseEventParams, DispatchMouseEventType, InsertTextParams, MouseButton,
};
use chromiumoxide::cdp::browser_protocol::page::{
    CaptureScreenshotFormat, CaptureScreenshotParams, EnableParams, ScreencastFrameAckParams,
    StartScreencastFormat, StartScreencastParams, StopScreencastParams,
};
use chromiumoxide::{Browser, Page};
use futures::StreamExt;
use serde::Serialize;
use std::sync::Arc;
use tokio::task::JoinHandle;

/// One open tab in an agent's browser, addressed by its position.
#[derive(Debug, Clone, Serialize)]
pub struct TabInfo {
    pub index: usize,
    pub url: String,
    pub title: String,
}

/// A live CDP connection to one agent's Chromium, bound to a single shared page.
///
/// The page is the one the agent drives; the view plane screencasts this same
/// page, so the human and the agent never diverge onto different tabs.
pub struct CdpSession {
    pub browser: Browser,
    pub page: Page,
    handler: JoinHandle<()>,
}

impl CdpSession {
    pub async fn connect(endpoint: &str) -> Result<Arc<Self>> {
        let (browser, mut handler) = Browser::connect(endpoint.to_string())
            .await
            .with_context(|| format!("connecting to CDP at {endpoint}"))?;

        let handler = tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                if let Err(err) = event {
                    tracing::debug!("cdp handler error: {err}");
                }
            }
        });

        let page = match browser.pages().await?.into_iter().next() {
            Some(page) => page,
            None => browser
                .new_page("about:blank")
                .await
                .context("opening the agent page")?,
        };
        page.execute(EnableParams::builder().build())
            .await
            .context("Page.enable")?;

        Ok(Arc::new(Self {
            browser,
            page,
            handler,
        }))
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
            .execute(params)
            .await
            .context("Emulation.setDeviceMetricsOverride")?;
        Ok(())
    }

    pub async fn goto(&self, url: &str) -> Result<()> {
        self.page.goto(url).await.context("Page.navigate")?;
        Ok(())
    }

    /// Begin streaming the page at its native viewport size.
    pub async fn start_screencast(&self) -> Result<()> {
        let params = StartScreencastParams::builder()
            .format(StartScreencastFormat::Jpeg)
            .quality(70)
            .build();
        self.page
            .execute(params)
            .await
            .context("Page.startScreencast")?;
        Ok(())
    }

    pub async fn stop_screencast(&self) -> Result<()> {
        self.page
            .execute(StopScreencastParams {})
            .await
            .context("Page.stopScreencast")?;
        Ok(())
    }

    /// Acknowledge a frame so Chromium releases the next one (flow control).
    pub async fn ack_frame(&self, session_id: i64) -> Result<()> {
        let params = ScreencastFrameAckParams::builder()
            .session_id(session_id)
            .build()
            .map_err(|err| anyhow!(err))?;
        self.page
            .execute(params)
            .await
            .context("Page.screencastFrameAck")?;
        Ok(())
    }

    pub async fn click(&self, x: f64, y: f64) -> Result<()> {
        self.mouse(
            DispatchMouseEventType::MousePressed,
            x,
            y,
            MouseButton::Left,
        )
        .await?;
        self.mouse(
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
        let params = DispatchMouseEventParams::builder()
            .r#type(kind)
            .x(x)
            .y(y)
            .button(button)
            .click_count(1)
            .build()
            .map_err(|err| anyhow!(err))?;
        self.page
            .execute(params)
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
            .execute(params)
            .await
            .context("Page.captureScreenshot")?;
        let encoded: &str = response.result.data.as_ref();
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .context("decode screenshot")
    }

    /// Every open tab, in browser order.
    pub async fn tabs(&self) -> Result<Vec<TabInfo>> {
        let pages = self.browser.pages().await?;
        let mut tabs = Vec::with_capacity(pages.len());
        for (index, page) in pages.iter().enumerate() {
            tabs.push(TabInfo {
                index,
                url: page.url().await?.unwrap_or_default(),
                title: page.get_title().await?.unwrap_or_default(),
            });
        }
        Ok(tabs)
    }

    pub async fn open_tab(&self, url: &str) -> Result<()> {
        let page = self.browser.new_page(url).await?;
        page.bring_to_front().await?;
        Ok(())
    }

    pub async fn activate_tab(&self, index: usize) -> Result<()> {
        if let Some(page) = self.browser.pages().await?.into_iter().nth(index) {
            page.bring_to_front().await?;
        }
        Ok(())
    }

    pub async fn close_tab(&self, index: usize) -> Result<()> {
        if let Some(page) = self.browser.pages().await?.into_iter().nth(index) {
            page.close().await?;
        }
        Ok(())
    }
}

impl Drop for CdpSession {
    fn drop(&mut self) {
        self.handler.abort();
    }
}
