const fs = require("node:fs");
const { test, expect } = require("@playwright/test");

// The ratatui suite drives a real app binary through the service. `make
// ui-test` builds the trex example and passes its path here; without it the
// spec cannot launch anything, so it fails loudly rather than skipping.
const TREX = process.env.LUMEN_TREX;

function sessionName(label) {
  return `e2e-tui-${label}-${Date.now().toString(36)}-${test.info().workerIndex}`.slice(0, 32);
}

async function create(request, name) {
  const response = await request.post("/v1/sessions", {
    data: { name, kind: "ratatui", path: TREX, origin: "agent", owner: "e2e" },
  });
  expect(response.ok(), await response.text()).toBeTruthy();
  return response.json();
}

test.beforeAll(() => {
  expect(TREX, "LUMEN_TREX must point at the built trex example").toBeTruthy();
  expect(fs.existsSync(TREX), `${TREX} exists`).toBeTruthy();
});

test("renders a ratatui app as cells and reports its grid as text", async ({ request }) => {
  const name = sessionName("screen");
  try {
    const info = await create(request, name);
    expect(info.kind).toBe("ratatui");
    expect(info.path).toBe(TREX);
    expect(info.cdp_endpoint).toBe("");

    // The app renders continuously, so the grid is populated without any input.
    await expect
      .poll(
        async () => {
          const response = await request.get(`/v1/sessions/${name}/screen`);
          if (!response.ok()) return "";
          return response.text();
        },
        { timeout: 10_000, intervals: [100, 250, 500] },
      )
      .toContain("score");

    const screen = await (await request.get(`/v1/sessions/${name}/screen`)).text();
    expect(screen).toContain("🦖");
    expect(screen).toContain("─");

    // A screenshot is pixels, which a cell grid is not: the service refuses it
    // and points at the text endpoint instead.
    const shot = await request.post(`/v1/sessions/${name}/screenshot`);
    expect(shot.status()).toBe(409);
  } finally {
    await request.delete(`/v1/sessions/${name}`);
  }
});

test("streams cell snapshots and accepts human input", async ({ page, request }) => {
  const name = sessionName("input");
  try {
    await create(request, name);
    await page.goto("/");

    const snapshot = await page.evaluate(
      ({ name }) =>
        new Promise((resolve, reject) => {
          const protocol = location.protocol === "https:" ? "wss" : "ws";
          const ws = new WebSocket(`${protocol}://${location.host}/v1/sessions/${name}/stream`);
          ws.binaryType = "arraybuffer";
          const timer = setTimeout(() => {
            ws.close();
            reject(new Error("timed out waiting for a cell snapshot"));
          }, 8000);
          ws.onerror = () => reject(new Error("stream failed"));
          ws.onmessage = (event) => {
            if (typeof event.data === "string") return;
            clearTimeout(timer);
            const text = new TextDecoder().decode(event.data);
            ws.close();
            resolve(JSON.parse(text));
          };
        }),
      { name },
    );

    expect(snapshot.cols).toBeGreaterThan(0);
    expect(snapshot.rows).toBeGreaterThan(0);
    expect(Array.isArray(snapshot.cells)).toBe(true);
    expect(snapshot.cells.length).toBeGreaterThan(0);
    // Every listed cell is inside the grid and carries a symbol.
    for (const cell of snapshot.cells) {
      expect(cell.x).toBeLessThan(snapshot.cols);
      expect(cell.y).toBeLessThan(snapshot.rows);
      expect(typeof cell.s).toBe("string");
    }

    // Claim control and send the jump key; the app's score keeps advancing, so
    // the observable is that the service accepted the key without an error.
    const result = await page.evaluate(
      ({ name }) =>
        new Promise((resolve, reject) => {
          const protocol = location.protocol === "https:" ? "wss" : "ws";
          const ws = new WebSocket(`${protocol}://${location.host}/v1/sessions/${name}/stream`);
          const errors = [];
          const timer = setTimeout(() => {
            ws.close();
            resolve(errors);
          }, 1200);
          ws.onerror = () => reject(new Error("stream failed"));
          ws.onmessage = (event) => {
            if (typeof event.data !== "string") return;
            const message = JSON.parse(event.data);
            if (message.type === "error") errors.push(message.message);
          };
          ws.onopen = () => {
            ws.send(JSON.stringify({ type: "control", action: "claim" }));
            ws.send(JSON.stringify({ type: "text", text: " " }));
            ws.send(JSON.stringify({ type: "key", code: "up", mods: 0 }));
          };
        }),
      { name },
    );
    expect(result).toEqual([]);
  } finally {
    await request.delete(`/v1/sessions/${name}`);
  }
});

test("rejects a ratatui path that is missing or not executable", async ({ request }) => {
  const missing = await request.post("/v1/sessions", {
    data: { name: sessionName("missing"), kind: "ratatui", path: "/tmp/lumen-no-such-app" },
  });
  expect(missing.status()).toBe(400);

  const notExecutable = await request.post("/v1/sessions", {
    data: { name: sessionName("nonexec"), kind: "ratatui", path: `${process.cwd()}/Cargo.toml` },
  });
  expect(notExecutable.status()).toBe(400);

  const relative = await request.post("/v1/sessions", {
    data: { name: sessionName("relative"), kind: "ratatui", path: "target/debug/examples/trex" },
  });
  expect(relative.status()).toBe(400);
});
