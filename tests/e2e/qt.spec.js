// End-to-end coverage for Qt sessions and their accessibility tree.
//
// The suite only runs when LUMEN_QT_APP names a real Qt application, which
// bin/ui-test.sh sets after it builds tests/fixtures/qt_app. Without it the
// spec skips, so a host with no Qt development toolchain still gets a green
// run instead of a false failure. LUMEN_QT_SPARSE_APP names the same binary
// loading a QML that publishes no named accessible object; it covers the
// sparse-tree warning.
const { test, expect } = require("@playwright/test");

const qtApp = process.env.LUMEN_QT_APP;
const sparseApp = process.env.LUMEN_QT_SPARSE_APP;

function findByName(node, wanted) {
  if (!node) return null;
  if (node.name === wanted) return node;
  for (const child of node.children || []) {
    const found = findByName(child, wanted);
    if (found) return found;
  }
  return null;
}

// True when any object below the application entries carries a name. The
// desktop root and the application entries themselves are registry chrome
// that always carry names, so they must not count.
function hasNamedBelowApplication(node) {
  if (!node || !node.children) return false;
  for (const app of node.children) {
    const stack = [...(app.children || [])];
    while (stack.length > 0) {
      const current = stack.pop();
      if (current.name) return true;
      stack.push(...(current.children || []));
    }
  }
  return false;
}

async function fetchTree(request, name) {
  const response = await request.get(`/v1/sessions/${name}/accessibility`);
  if (!response.ok()) return null;
  return response.json();
}

async function waitForTree(request, name, predicate, timeout = 25_000) {
  const deadline = Date.now() + timeout;
  let tree = null;
  while (Date.now() < deadline) {
    tree = await fetchTree(request, name);
    if (tree && predicate(tree)) return tree;
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  return tree;
}

test.describe("qt accessibility", () => {
  test.skip(!qtApp, "LUMEN_QT_APP is not set (no Qt application to drive)");

  test("reads the tree and clicks an element by reference", async ({ request }) => {
    test.setTimeout(90_000);
    const name = `qt-${Date.now()}`;

    const created = await request.post("/v1/sessions", {
      data: { name, kind: "qt", path: qtApp },
    });
    expect(created.ok(), await created.text()).toBeTruthy();
    expect((await created.json()).kind).toBe("qt");

    try {
      const tree = await waitForTree(request, name, (candidate) =>
        findByName(candidate, "Increment"),
      );
      const button = findByName(tree, "Increment");
      expect(button, "the Increment button is published").not.toBeNull();
      expect(button.role).toBe("push button");
      expect(button.bounds, "the button carries screen bounds").toBeTruthy();
      expect(button.bounds.width).toBeGreaterThan(0);

      expect(findByName(tree, "Name field"), "the text field is published").not.toBeNull();
      expect(findByName(tree, "0"), "the counter starts at zero").not.toBeNull();

      const clicked = await request.post(`/v1/sessions/${name}/accessibility/click`, {
        data: { ref: button.ref },
      });
      expect(clicked.ok(), await clicked.text()).toBeTruthy();

      const after = await waitForTree(request, name, (candidate) => findByName(candidate, "1"));
      expect(findByName(after, "1"), "clicking the button advances the counter").not.toBeNull();

      // A tree with named objects carries no warning, and its body stats
      // report the named objects the agent can address.
      const healthy = await request.get(`/v1/sessions/${name}/accessibility`);
      expect(healthy.ok()).toBeTruthy();
      expect(healthy.headers()["x-lumen-tree-warning"]).toBeUndefined();
      const stats = (await healthy.json()).stats;
      expect(stats.applications).toBeGreaterThanOrEqual(1);
      expect(stats.named).toBeGreaterThan(0);
      expect(stats.max_depth).toBeGreaterThan(0);
    } finally {
      await request.delete(`/v1/sessions/${name}`);
    }
  });
});

test.describe("qt sparse tree", () => {
  test.skip(!sparseApp, "LUMEN_QT_SPARSE_APP is not set (no sparse Qt fixture)");

  test("warns when the application publishes no named object", async ({ request }) => {
    test.setTimeout(90_000);
    const name = `qt-sparse-${Date.now()}`;

    const created = await request.post("/v1/sessions", {
      data: { name, kind: "qt", path: sparseApp },
    });
    expect(created.ok(), await created.text()).toBeTruthy();

    try {
      // The application may not have registered on the bus yet; the endpoint
      // answers 409 until then, so poll for the first served tree.
      let response = null;
      const deadline = Date.now() + 25_000;
      while (Date.now() < deadline) {
        const candidate = await request.get(`/v1/sessions/${name}/accessibility`);
        if (candidate.ok()) {
          response = candidate;
          break;
        }
        await new Promise((resolve) => setTimeout(resolve, 500));
      }
      expect(response, "the sparse tree is served").not.toBeNull();
      expect(
        response.headers()["x-lumen-tree-warning"],
        "a tree without named objects carries a warning header",
      ).toBeTruthy();

      const tree = await response.json();
      expect(hasNamedBelowApplication(tree), "nothing below the application is named").toBe(false);
      // The body itself must carry the signal, for a caller that reads no
      // headers: measured emptiness, not just a bare node skeleton.
      expect(tree.stats.applications).toBeGreaterThanOrEqual(1);
      expect(tree.stats.nodes).toBeGreaterThan(0);
      expect(tree.stats.named).toBe(0);
      expect(typeof tree.stats.max_depth).toBe("number");
    } finally {
      await request.delete(`/v1/sessions/${name}`);
    }
  });
});
