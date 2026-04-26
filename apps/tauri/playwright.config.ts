import { defineConfig, devices } from "@playwright/test";

// Tests run against the Vite dev server (no Tauri shell). Tauri internals
// are stubbed in tests/fixtures/tauri-shim.ts so the bundle boots in plain
// Chromium and we can synthesize backend events.
export default defineConfig({
  testDir: "./tests",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: "http://localhost:1420",
    trace: "on-first-retry",
    colorScheme: "dark",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: {
    command: "pnpm dev",
    url: "http://localhost:1420",
    reuseExistingServer: !process.env.CI,
    stdout: "ignore",
    stderr: "pipe",
  },
});
