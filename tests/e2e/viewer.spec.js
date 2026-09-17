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

test("draws and sends a feedback annotation", async ({ page, request }) => {
  const name = sessionName("feedback");
  try {
    await createFromViewer(page, name);
    await page.locator("#comment").click();

    const box = await page.locator("#overlay").boundingBox();
    expect(box).not.toBeNull();
    const start = { x: box.x + 32, y: box.y + 32 };
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
    const region = items[0].region;
    // Region is in page CSS pixels; screen px times scale round-trips to the
    // rectangle actually drawn, not wherever the pointer ended up.
    expect(region.scale * region.width).toBeCloseTo(140, 0);
    expect(region.scale * region.height).toBeCloseTo(90, 0);
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
