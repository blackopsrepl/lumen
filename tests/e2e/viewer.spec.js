const http = require("node:http");
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
    await page.locator("#go").click();
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
    await page.mouse.move(end.x, end.y);
    await page.mouse.up();

    await expect(page.locator("#composer")).toBeVisible();
    await page.locator("#comment-text").fill("Please keep this area aligned");
    await page.locator("#comment-send").click();
    await expect(page.locator("#feedback-list")).toContainText("Please keep this area aligned");
    await expect(page.locator("#feedback-count")).toHaveText("1");
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
    await page.locator("#go").click();
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
