// End-to-end coverage for Qt sessions and their accessibility tree.
//
// The suite only runs when LUMEN_QT_APP names a real Qt application, which
// bin/ui-test.sh sets after it builds tests/fixtures/qt_app. Without it the
// spec skips, so a host with no Qt development toolchain still gets a green
// run instead of a false failure.
const { test, expect } = require("@playwright/test");

const qtApp = process.env.LUMEN_QT_APP;

function findByName(node, wanted) {
  if (!node) return null;
  if (node.name === wanted) return node;
  for (const child of node.children || []) {
    const found = findByName(child, wanted);
    if (found) return found;
  }
  return null;
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
    } finally {
      await request.delete(`/v1/sessions/${name}`);
    }
  });
});
