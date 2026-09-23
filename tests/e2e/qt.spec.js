// End-to-end coverage for Qt sessions and their accessibility tree.
//
// The suite only runs when LUMEN_QT_APP names a real Qt application, which
// bin/ui-test.sh sets after it builds tests/fixtures/qt_app. Without it the
// spec skips, so a host with no Qt development toolchain still gets a green
// run instead of a false failure. LUMEN_QT_SPARSE_APP names the same binary
// loading a QML that publishes nothing at all, and LUMEN_QT_UNNAMED_APP one
// that publishes an unnamed control; together they cover the two ways a tree
// comes back sparse.
const { test, expect } = require("@playwright/test");

const qtApp = process.env.LUMEN_QT_APP;
const sparseApp = process.env.LUMEN_QT_SPARSE_APP;
const unnamedApp = process.env.LUMEN_QT_UNNAMED_APP;

function findByName(node, wanted) {
  if (!node) return null;
  if (node.name === wanted) return node;
  for (const child of node.children || []) {
    const found = findByName(child, wanted);
    if (found) return found;
  }
  return null;
}

// True when any object inside the application's windows carries a name. The
// desktop root, the application entries, and the window titles are registry
// and window identity, not the identity of a target, so they must not count.
function hasNamedInsideWindows(node) {
  if (!node || !node.children) return false;
  for (const app of node.children) {
    for (const window of app.children || []) {
      const stack = [...(window.children || [])];
      while (stack.length > 0) {
        const current = stack.pop();
        if (current.name) return true;
        stack.push(...(current.children || []));
      }
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
      expect(stats.interior).toBeGreaterThan(0);
      expect(stats.max_depth).toBeGreaterThan(0);
    } finally {
      await request.delete(`/v1/sessions/${name}`);
    }
  });
});

test.describe("qt empty tree", () => {
  test.skip(!sparseApp, "LUMEN_QT_SPARSE_APP is not set (no empty Qt fixture)");

  test("refuses a tree when the application published nothing", async ({ request }) => {
    test.setTimeout(90_000);
    const name = `qt-empty-${Date.now()}`;

    const created = await request.post("/v1/sessions", {
      data: { name, kind: "qt", path: sparseApp },
    });
    expect(created.ok(), await created.text()).toBeTruthy();

    try {
      // Before the application registers, the endpoint reports that no
      // application is publishing; once it has registered, an application
      // with nothing addressable must be reported as such, never served as a
      // bare registry skeleton.
      let refused = false;
      const deadline = Date.now() + 25_000;
      while (Date.now() < deadline) {
        const candidate = await request.get(`/v1/sessions/${name}/accessibility`);
        expect(
          candidate.ok(),
          "an application that published nothing was served a tree",
        ).toBe(false);
        const body = await candidate.json();
        if ((body.error || "").includes("no accessible objects inside its windows")) {
          refused = true;
          break;
        }
        await new Promise((resolve) => setTimeout(resolve, 500));
      }
      expect(refused, "the empty application is refused").toBe(true);
    } finally {
      await request.delete(`/v1/sessions/${name}`);
    }
  });
});

test.describe("qt unnamed tree", () => {
  test.skip(!unnamedApp, "LUMEN_QT_UNNAMED_APP is not set (no unnamed Qt fixture)");

  test("serves an unnamed control with a warning", async ({ request }) => {
    test.setTimeout(90_000);
    const name = `qt-unnamed-${Date.now()}`;

    const created = await request.post("/v1/sessions", {
      data: { name, kind: "qt", path: unnamedApp },
    });
    expect(created.ok(), await created.text()).toBeTruthy();

    try {
      // Same transient 409 while the application registers.
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
      expect(response, "the unnamed tree is served").not.toBeNull();
      expect(
        response.headers()["x-lumen-tree-warning"],
        "a tree without named objects carries a warning header",
      ).toBeTruthy();

      const tree = await response.json();
      expect(hasNamedInsideWindows(tree), "nothing inside the windows is named").toBe(false);
      // The control is real: it carries its own rectangle, so the tree stays
      // addressable even though nothing names it.
      expect(tree.stats.named).toBe(0);
      expect(tree.stats.interior).toBeGreaterThan(0);
      expect(typeof tree.stats.max_depth).toBe("number");
    } finally {
      await request.delete(`/v1/sessions/${name}`);
    }
  });
});
