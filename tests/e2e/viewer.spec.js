const http = require("node:http");
const path = require("node:path");
const { test, expect } = require("@playwright/test");

function sessionName(label) {
  return `e2e-${label}-${Date.now().toString(36)}-${test.info().workerIndex}`.slice(0, 32);
}

async function deleteSession(request, name) {
  await request.delete(`/v1/sessions/${encodeURIComponent(name)}`);
}

async function createFromViewer(page, name) {
  await page.goto("/");
  await page.locator("#new-name").fill(name);
  await page.locator("#new-session button").click();
  await expect(page.locator(`[data-name="${name}"]`)).toBeVisible();
  await expect(page.locator("#conn-status")).toHaveText(`live · ${name}`);
  await expect(page.locator("#screen")).toHaveAttribute("data-frame-ready", "true");
}

test("streams a session, navigates it, and hands control back", async ({ page, request }) => {
  const name = sessionName("control");
  try {
    await createFromViewer(page, name);

    await page.locator("#url").fill("data:text/html,<title>E2E page</title><h1>viewer e2e</h1>");
    await page.locator("#url").press("Enter");
    await expect.poll(() => page.locator("#url").inputValue()).toContain("data:text/html");

    await page.locator("#control").click();
    await expect(page.locator("body")).toHaveClass(/controlling/);
    await expect(page.locator("#banner")).toBeVisible();

    await page.keyboard.press("Escape");
    await expect(page.locator("body")).not.toHaveClass(/controlling/);
    await expect(page.locator("#control")).toHaveText("Take control");
  } finally {
    await deleteSession(request, name);
  }
});

test("pans the streamed frame while control is not held", async ({ page, request }) => {
  const name = sessionName("pan");
  const offset = () =>
    page.locator("#screen").evaluate((canvas) => ({
      x: Number(canvas.dataset.offsetX),
      y: Number(canvas.dataset.offsetY),
    }));
  try {
    await createFromViewer(page, name);
    const before = await offset();

    const stage = await page.locator("#stage").boundingBox();
    const center = { x: stage.x + stage.width / 2, y: stage.y + stage.height / 2 };
    await page.mouse.move(center.x, center.y);
    await page.mouse.down();
    await page.mouse.move(center.x + 90, center.y + 60, { steps: 8 });
    await page.mouse.up();

    const after = await offset();
    expect(after.x).toBe(before.x + 90);
    expect(after.y).toBe(before.y + 60);
    await expect(page.locator("body")).not.toHaveClass(/panning/);
  } finally {
    await deleteSession(request, name);
  }
});

test("streams a Quickshell desktop session", async ({ page, request }) => {
  const name = sessionName("quickshell");
  try {
    await page.goto("/");
    await page.locator("#new-name").fill(name);
    await page.locator("#new-kind").selectOption("quickshell");
    await page.locator("#new-path").fill(path.resolve("tests/fixtures/quickshell.qml"));
    await page.locator("#new-session button").click();
    await expect(page.locator(`[data-name="${name}"]`)).toBeVisible();
    await expect(page.locator("#conn-status")).toHaveText(`live · ${name}`);
    await expect(page.locator("#screen")).toHaveAttribute("data-frame-ready", "true");
    await expect(page.locator("#url")).toBeDisabled();
    await expect(page.locator("#page-title")).toHaveText("Quickshell desktop");

    await page.locator("#control").click();
    await expect(page.locator("body")).toHaveClass(/controlling/);
    const screen = await page.locator("#screen").boundingBox();
    expect(screen).not.toBeNull();
    await page.mouse.click(screen.x + screen.width / 2, screen.y + screen.height / 2);
    await page.keyboard.press("Escape");
    await expect(page.locator("body")).not.toHaveClass(/controlling/);
  } finally {
    await deleteSession(request, name);
  }
});

test("coalesces frames while browser decoding is busy", async ({ page, request }) => {
  const name = sessionName("decode");
  await page.addInitScript(() => {
    const decode = window.createImageBitmap.bind(window);
    window.decodeStats = { active: 0, maxActive: 0, calls: 0 };
    window.createImageBitmap = async (...args) => {
      window.decodeStats.active += 1;
      window.decodeStats.calls += 1;
      window.decodeStats.maxActive = Math.max(
        window.decodeStats.maxActive,
        window.decodeStats.active,
      );
      await new Promise((resolve) => setTimeout(resolve, 50));
      try {
        return await decode(...args);
      } finally {
        window.decodeStats.active -= 1;
      }
    };
  });
  try {
    await createFromViewer(page, name);
    const before = await page.evaluate(() => window.decodeStats.calls);
    const response = await request.post(`/v1/sessions/${name}/navigate`, {
      data: {
        url: "data:text/html,<script>let n=0;let t=setInterval(()=>{document.body.textContent=String(++n);document.body.style.background='hsl('+n*19+' 80% 50%)';if(n===20)clearInterval(t)},10)</script>",
      },
    });
    expect(response.ok()).toBeTruthy();
    await expect.poll(() => page.evaluate(() => window.decodeStats.calls)).toBeGreaterThan(before);
    await expect.poll(() => page.evaluate(() => window.decodeStats.active)).toBe(0);
    const stats = await page.evaluate(() => window.decodeStats);
    expect(stats.maxActive).toBe(1);
  } finally {
    await deleteSession(request, name);
  }
});

test("draws and sends a feedback annotation", async ({ page, request }) => {
  const name = sessionName("feedback");
  try {
    await createFromViewer(page, name);
    await page.locator("#comment").click();

    const box = await page.locator("#overlay").boundingBox();
    expect(box).not.toBeNull();
    // Center the rectangle on the stage so it lies wholly inside the displayed
    // frame; a corner would be clipped by the letterbox margin, which is not
    // what this test is about.
    const start = { x: box.x + box.width / 2 - 70, y: box.y + box.height / 2 - 45 };
    const end = { x: start.x + 140, y: start.y + 90 };
    await page.mouse.move(start.x, start.y);
    await page.mouse.down();
    await page.mouse.move(end.x, end.y, { steps: 8 });
    await page.mouse.up();

    // Releasing must freeze the rectangle: moving the pointer on afterwards
    // (as a human does on the way to the composer) must not grow the region.
    await page.mouse.move(end.x + 320, end.y + 260, { steps: 20 });

    await expect(page.locator("#composer")).toBeVisible();
    await page.locator("#comment-text").fill("Please keep this area aligned");
    await page.locator("#comment-send").click();
    await expect(page.locator("#feedback-list")).toContainText("Please keep this area aligned");
    await expect(page.locator("#feedback-count")).toHaveText("1");

    const stored = await request.get(
      `/v1/sessions/${encodeURIComponent(name)}/feedback?pending=true`,
    );
    const items = await stored.json();
    expect(items[0].screenshot).toBe(true);

    // The note carries the pixels of the drawn rectangle, not coordinates that
    // the page could invalidate. The screenshot's aspect ratio round-trips to
    // the rectangle actually drawn, not wherever the pointer ended up.
    const shot = await request.get(
      `/v1/sessions/${encodeURIComponent(name)}/feedback/${items[0].id}/screenshot`,
    );
    expect(shot.ok()).toBeTruthy();
    expect(shot.headers()["content-type"]).toContain("image/png");
    const png = await shot.body();
    expect([...png.subarray(0, 8)]).toEqual([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
    expect(png.readUInt32BE(16) / png.readUInt32BE(20)).toBeCloseTo(140 / 90, 1);
  } finally {
    await deleteSession(request, name);
  }
});

test("reaches a host-loopback development server", async ({ page, request }) => {
  const name = sessionName("host");
  const server = http.createServer((_, response) => {
    response.end("<title>host e2e</title><h1>host development server</h1>");
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  try {
    await createFromViewer(page, name);
    await page.locator("#url").fill(`http://127.0.0.1:${address.port}`);
    await page.locator("#url").press("Enter");
    await expect(page.locator("#url")).toHaveValue(`http://127.0.0.1:${address.port}`);
    await expect
      .poll(async () => {
        const response = await request.get(`/v1/sessions/${encodeURIComponent(name)}/tabs`);
        const tabs = await response.json();
        return tabs[0]?.title;
      })
      .toBe("host e2e");
  } finally {
    await deleteSession(request, name);
    await new Promise((resolve) => server.close(resolve));
  }
});

test("shows agent and manual provenance separately", async ({ page, request }) => {
  const agent = sessionName("agent");
  const manual = sessionName("manual");
  try {
    const response = await request.post("/v1/sessions", {
      data: { name: agent, origin: "agent", owner: "e2e" },
    });
    expect(response.ok()).toBeTruthy();

    await page.goto("/");
    await page.locator("#new-name").fill(manual);
    await page.locator("#new-session button").click();
    await expect(page.locator(`[data-name="${agent}"]`)).toBeVisible();
    await expect(page.locator(`[data-name="${manual}"]`)).toBeVisible();
    await expect(page.locator("#sessions")).toContainText("Agent sessions");
    await expect(page.locator("#sessions")).toContainText("Manual · no agent");
  } finally {
    await deleteSession(request, agent);
    await deleteSession(request, manual);
  }
});
