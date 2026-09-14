use anyhow::{anyhow, Context, Result};
use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
use chromiumoxide::cdp::browser_protocol::input::{
    DispatchMouseEventParams, DispatchMouseEventType, InsertTextParams, MouseButton,
};
use chromiumoxide::cdp::browser_protocol::page::{
    EnableParams, ScreencastFrameAckParams, StartScreencastFormat, StartScreencastParams,
    StopScreencastParams,
};
use chromiumoxide::{Browser, Page};
use futures::StreamExt;
use std::sync::Arc;
use tokio::task::JoinHandle;

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

    pub async fn start_screencast(&self, width: u32, height: u32) -> Result<()> {
        let params = StartScreencastParams::builder()
            .format(StartScreencastFormat::Jpeg)
            .quality(70)
            .max_width(width as i64)
            .max_height(height as i64)
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
        self.mouse(DispatchMouseEventType::MousePressed, x, y)
            .await?;
        self.mouse(DispatchMouseEventType::MouseReleased, x, y)
            .await?;
        Ok(())
    }

    async fn mouse(&self, kind: DispatchMouseEventType, x: f64, y: f64) -> Result<()> {
        let params = DispatchMouseEventParams::builder()
            .r#type(kind)
            .x(x)
            .y(y)
            .button(MouseButton::Left)
            .click_count(1)
            .build()
            .map_err(|err| anyhow!(err))?;
        self.page
            .execute(params)
            .await
            .context("Input.dispatchMouseEvent")?;
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
}

impl Drop for CdpSession {
    fn drop(&mut self) {
        self.handler.abort();
    }
}
