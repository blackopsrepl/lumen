const fs = require("node:fs");
const { test, expect } = require("@playwright/test");

const PROFILES = process.env.LUMEN_PROFILES_DIR || "/tmp/lumen-e2e-agents/run";

function profileDirs(name) {
  try {
    return fs.readdirSync(PROFILES).filter((entry) => entry.startsWith(`${name}-`));
  } catch {
    return [];
  }
}

function sessionName(label) {
  return `e2e-api-${label}-${Date.now().toString(36)}-${test.info().workerIndex}`.slice(0, 32);
}

test("rejects profile path components as session names", async ({ request }) => {
  for (const name of [".", ".."])
    expect((await request.post("/v1/sessions", { data: { name } })).status()).toBe(400);
});

test("keeps the managed tab active and protects the last tab", async ({ request }) => {
  const name = sessionName("tabs");
  try {
    expect((await request.post("/v1/sessions", { data: { name } })).ok()).toBeTruthy();
    expect(
      (
        await request.post(`/v1/sessions/${name}/tabs`, {
          data: { url: "data:text/html,new-tab" },
        })
      ).ok(),
    ).toBeTruthy();

    let tabs = await (await request.get(`/v1/sessions/${name}/tabs`)).json();
    expect(tabs.filter((tab) => tab.active)).toHaveLength(1);
    expect(tabs.find((tab) => tab.active).url).toContain("new-tab");

    const otherIndex = tabs.findIndex((tab) => !tab.active);
    expect((await request.post(`/v1/sessions/${name}/tabs/${otherIndex}/activate`)).ok()).toBeTruthy();
    tabs = await (await request.get(`/v1/sessions/${name}/tabs`)).json();
    expect(tabs.find((tab) => tab.active).url).not.toContain("new-tab");

    const activeIndex = tabs.findIndex((tab) => tab.active);
    expect((await request.delete(`/v1/sessions/${name}/tabs/${activeIndex}`)).ok()).toBeTruthy();
    tabs = await (await request.get(`/v1/sessions/${name}/tabs`)).json();
    expect(tabs).toHaveLength(1);
    expect(tabs[0].active).toBeTruthy();
    expect((await request.delete(`/v1/sessions/${name}/tabs/0`)).status()).toBe(409);
  } finally {
    await request.delete(`/v1/sessions/${name}`);
  }
});

test("adopts a tab the browser opened itself", async ({ request }) => {
  const name = sessionName("adopt");
  // Chromium refuses to open popups at data: URLs, so aim the link at the
  // service's own health endpoint.
  const popup = process.env.LUMEN_URL || `http://127.0.0.1:${process.env.LUMEN_PORT || 18899}/healthz`;
  const link = `data:text/html,<a%20href="${popup}"%20target="_blank"%20style="position:fixed;left:10px;top:10px;width:120px;height:40px;display:block">open</a>`;
  try {
    expect((await request.post("/v1/sessions", { data: { name } })).ok()).toBeTruthy();
    expect((await request.post(`/v1/sessions/${name}/navigate`, { data: { url: link } })).ok()).toBeTruthy();
    await expect
      .poll(async () => {
        const tabs = await (await request.get(`/v1/sessions/${name}/tabs`)).json();
        return tabs.find((tab) => tab.active)?.url;
      })
      .toContain("target=");

    // A trusted click (viewer Take control, or an agent over CDP) on a
    // target=_blank link makes the browser open and foreground a tab without
    // any Lumen API call. The managed tab must follow it.
    const click = (type) =>
      request.post(`/v1/sessions/${name}/cdp`, {
        data: {
          method: "Input.dispatchMouseEvent",
          params: { type, x: 60, y: 20, button: "left", clickCount: 1 },
        },
      });
    expect((await click("mousePressed")).ok()).toBeTruthy();
    expect((await click("mouseReleased")).ok()).toBeTruthy();

    await expect
      .poll(async () => {
        const tabs = await (await request.get(`/v1/sessions/${name}/tabs`)).json();
        return tabs.filter((tab) => tab.active).map((tab) => tab.url);
      })
      .toEqual([popup]);
  } finally {
    await request.delete(`/v1/sessions/${name}`);
  }
});

test("deleting a session removes read and stream targets", async ({ request }) => {
  const name = sessionName("delete");
  expect((await request.post("/v1/sessions", { data: { name } })).ok()).toBeTruthy();
  try {
    expect((await request.delete(`/v1/sessions/${name}`)).status()).toBe(204);
    expect((await request.get(`/v1/sessions/${name}`)).status()).toBe(404);
    expect((await request.get(`/v1/sessions/${name}/tabs`)).status()).toBe(404);
  } finally {
    await request.delete(`/v1/sessions/${name}`);
  }
});

test("recreates a session after its Chromium exits", async ({ request }) => {
  const name = sessionName("recover");
  try {
    const first = await (await request.post("/v1/sessions", { data: { name } })).json();
    expect(
      (await request.post(`/v1/sessions/${name}/cdp`, {
        data: { method: "Browser.close", params: {} },
      })).ok(),
    ).toBeTruthy();
    await expect
      .poll(async () => (await request.post("/v1/sessions", { data: { name } })).json())
      .not.toHaveProperty("cdp_endpoint", first.cdp_endpoint);
  } finally {
    await request.delete(`/v1/sessions/${name}`);
  }
});

test("removes the browser profile when the session is deleted", async ({ request }) => {
  test.skip(Boolean(process.env.LUMEN_URL), "profiles are only inspectable for the disposable service");
  const name = sessionName("profile");
  expect((await request.post("/v1/sessions", { data: { name } })).ok()).toBeTruthy();
  await expect.poll(() => profileDirs(name).length).toBe(1);
  expect((await request.delete(`/v1/sessions/${name}`)).status()).toBe(204);
  await expect.poll(() => profileDirs(name).length).toBe(0);
});
