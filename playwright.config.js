const fs = require("node:fs");
const { defineConfig, devices } = require("@playwright/test");

const port = process.env.LUMEN_PORT || "8899";
const baseURL = process.env.LUMEN_URL || `http://127.0.0.1:${port}`;
const systemChromium = "/usr/bin/chromium";

module.exports = defineConfig({
  testDir: "./tests/e2e",
  timeout: 30_000,
  expect: { timeout: 10_000 },
  fullyParallel: false,
  workers: 1,
  reporter: process.env.CI ? "line" : "list",
  use: {
    baseURL,
    ...devices["Desktop Chrome"],
    headless: true,
    launchOptions: {
      executablePath:
        process.env.PLAYWRIGHT_EXECUTABLE_PATH ||
        (fs.existsSync(systemChromium) ? systemChromium : undefined),
    },
    trace: "retain-on-failure",
  },
  webServer: process.env.LUMEN_URL
    ? undefined
    : {
        command: `LUMEN_CONFIG=${process.env.LUMEN_TEST_CONFIG || "tests/e2e/lumen.toml"} cargo run --quiet -- serve`,
        url: `${baseURL}/healthz`,
        timeout: 120_000,
        // Always start our own server. Reusing whatever answers on the port is
        // how the suite ends up testing a leftover or the production service;
        // make ui-test picks a free port so this can never collide.
        reuseExistingServer: false,
      },
});
