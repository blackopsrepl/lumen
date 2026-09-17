const fs = require("node:fs");
const crypto = require("node:crypto");
const path = require("node:path");
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

test("reports an invalid session name as a bad request, not a server error", async ({ request }) => {
  const tooLong = "a".repeat(33);
  const calls = [
    ["navigate", { url: "about:blank" }],
    ["tabs", { url: "about:blank" }],
    ["screenshot", {}],
    ["cdp", { method: "Browser.getVersion" }],
  ];
  for (const [path, data] of calls) {
    const response = await request.post(`/v1/sessions/${tooLong}/${path}`, { data });
    expect(response.status(), `${path} should reject the name`).toBe(400);
  }
});

test("creates and screenshots a Quickshell session", async ({ page, request }) => {
  const name = sessionName("quickshell");
  const shell = path.resolve("tests/fixtures/quickshell.qml");
  try {
    const created = await request.post("/v1/sessions", {
      data: { name, kind: "quickshell", path: shell },
    });
    const createdBody = await created.text();
    expect(created.ok(), createdBody).toBeTruthy();
    const info = JSON.parse(createdBody);
    expect(info.kind).toBe("quickshell");
    expect(info.path).toBe(shell);
    expect(info.cdp_endpoint).toBe("");

    const screenshotHash = async () => {
      const screenshot = await request.post(`/v1/sessions/${name}/screenshot`);
      expect(screenshot.ok()).toBeTruthy();
      expect(screenshot.headers()["content-type"]).toContain("image/png");
      const body = await screenshot.body();
      expect(body.subarray(0, 8)).toEqual(
        Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
      );
      return crypto.createHash("sha256").update(body).digest("hex");
    };
    const before = await screenshotHash();
    await page.goto("/");
    await page.evaluate(
      ({ name, text }) =>
        new Promise((resolve, reject) => {
          const protocol = location.protocol === "https:" ? "wss" : "ws";
          const ws = new WebSocket(`${protocol}://${location.host}/v1/sessions/${name}/stream`);
          const timeout = setTimeout(() => {
            ws.close();
            reject(new Error("timed out sending desktop text"));
          }, 5000);
          ws.onerror = () => reject(new Error("desktop WebSocket failed"));
          ws.onopen = () => {
            ws.send(JSON.stringify({ type: "control", action: "claim" }));
            setTimeout(() => {
              ws.send(JSON.stringify({ type: "text", text }));
              setTimeout(() => ws.close(), 300);
            }, 100);
          };
          ws.onclose = () => {
            clearTimeout(timeout);
            resolve();
          };
        }),
      { name, text: "native input" },
    );
    // wtype is asynchronous relative to the control socket; wait for the compositor frame.
    await expect
      .poll(screenshotHash, { timeout: 5_000, intervals: [100, 250, 500] })
      .not.toBe(before);
    const tabs = await request.get(`/v1/sessions/${name}/tabs`);
    expect(tabs.status(), await tabs.text()).toBe(409);
  } finally {
    await request.delete(`/v1/sessions/${name}`);
  }
});

test("rejects invalid Quickshell paths as bad requests", async ({ request }) => {
  const name = sessionName("quickshell-path");
  for (const pathValue of ["tests/fixtures/quickshell.qml", "/tmp/lumen-no-such-shell.qml"]) {
    const response = await request.post("/v1/sessions", {
      data: { name, kind: "quickshell", path: pathValue },
    });
    expect(response.status(), await response.text()).toBe(400);
  }
});

test("refuses requests that did not come from loopback", async ({ request }) => {
  expect(
    (await request.get("/healthz", { headers: { Origin: "http://evil.test" } })).status(),
  ).toBe(403);
  expect((await request.get("/healthz", { headers: { Host: "evil.test" } })).status()).toBe(403);
  expect((await request.get("/healthz")).ok()).toBeTruthy();
});

test("serializes concurrent navigation and tab activation", async ({ request }) => {
  const name = sessionName("race");
  try {
    expect((await request.post("/v1/sessions", { data: { name } })).ok()).toBeTruthy();
    expect(
      (
        await request.post(`/v1/sessions/${name}/tabs`, {
          data: { url: "data:text/html,second" },
        })
      ).ok(),
    ).toBeTruthy();

    const navigate = () =>
      request.post(`/v1/sessions/${name}/navigate`, { data: { url: "data:text/html,raced" } });
    const activate = () => request.post(`/v1/sessions/${name}/tabs/0/activate`);

    const results = await Promise.all([navigate(), activate(), navigate(), activate(), navigate()]);
    for (const response of results) expect(response.ok()).toBeTruthy();

    const tabs = await (await request.get(`/v1/sessions/${name}/tabs`)).json();
    expect(tabs).toHaveLength(2);
    expect(tabs.filter((tab) => tab.active)).toHaveLength(1);

    // Whichever order the calls resolved in, the active tab is the one the
    // viewer and every later command must target.
    const active = tabs.find((tab) => tab.active);
    expect((await request.post(`/v1/sessions/${name}/tabs/1/activate`)).ok()).toBeTruthy();
    expect((await request.get(`/v1/sessions/${name}/tabs`)).ok()).toBeTruthy();
    const after = await (await request.get(`/v1/sessions/${name}/tabs`)).json();
    expect(after.filter((tab) => tab.active)).toHaveLength(1);
    expect(after.find((tab) => tab.active).index).not.toBe(active.index);
  } finally {
    await request.delete(`/v1/sessions/${name}`);
  }
});

test("recovers when the managed tab is closed behind Lumen's back", async ({ request }) => {
  const name = sessionName("destroy");
  try {
    const info = await (await request.post("/v1/sessions", { data: { name } })).json();
    expect(
      (
        await request.post(`/v1/sessions/${name}/tabs`, {
          data: { url: "data:text/html,keep-me" },
        })
      ).ok(),
    ).toBeTruthy();

    // Close the managed target directly, the way an agent driving CDP could.
    const targets = await (await request.get(`${info.cdp_endpoint}/json/list`)).json();
    const managed = targets.find((target) => target.type === "page" && target.url.includes("keep-me"));
    expect(managed).toBeTruthy();
    expect(
      (
        await request.post(`/v1/sessions/${name}/cdp`, {
          data: { method: "Target.closeTarget", params: { targetId: managed.id } },
        })
      ).ok(),
    ).toBeTruthy();

    await expect
      .poll(async () => {
        const tabs = await (await request.get(`/v1/sessions/${name}/tabs`)).json();
        return tabs.filter((tab) => tab.active).length;
      })
      .toBe(1);

    // The session must still be usable rather than stranded on a dead page.
    expect(
      (
        await request.post(`/v1/sessions/${name}/navigate`, {
          data: { url: "data:text/html,recovered" },
        })
      ).ok(),
    ).toBeTruthy();
    await expect
      .poll(async () => {
        const tabs = await (await request.get(`/v1/sessions/${name}/tabs`)).json();
        return tabs.find((tab) => tab.active)?.url;
      })
      .toContain("recovered");
  } finally {
    await request.delete(`/v1/sessions/${name}`);
  }
});

test("enforces the navigation policy through the API and over CDP", async ({ request }) => {
  const name = sessionName("policy");
  try {
    expect((await request.post("/v1/sessions", { data: { name } })).ok()).toBeTruthy();

    expect(
      (
        await request.post(`/v1/sessions/${name}/navigate`, {
          data: { url: "http://blocked.test/" },
        })
      ).status(),
      "API navigation to a blocked host",
    ).toBe(403);

    expect(
      (
        await request.post(`/v1/sessions/${name}/navigate`, {
          data: { url: "data:text/html,allowed" },
        })
      ).ok(),
    ).toBeTruthy();

    // The same block must hold for a navigation Lumen did not issue.
    const result = await (
      await request.post(`/v1/sessions/${name}/cdp`, {
        data: { method: "Page.navigate", params: { url: "http://blocked.test/" } },
      })
    ).json();
    expect(JSON.stringify(result)).toContain("ERR_BLOCKED_BY_CLIENT");
  } finally {
    await request.delete(`/v1/sessions/${name}`);
  }
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
