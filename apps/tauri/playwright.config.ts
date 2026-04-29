import { defineConfig, devices } from "@playwright/test";

// Tests run against the Vite dev server (no Tauri shell). Tauri internals
// are stubbed in tests/fixtures/tauri-shim.ts so the bundle boots in plain
// Chromium and we can synthesize backend events.
//
// `OW_PW_PORT` overrides the dev-server port. Useful when port 1420 is
// already taken by a Vite from a sibling worktree — set
// `OW_PW_PORT=1430 pnpm test:ui` and Playwright will spawn its own
// isolated server.
const PORT = process.env.OW_PW_PORT ?? "1420";

export default defineConfig({
  testDir: "./tests",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: `http://localhost:${PORT}`,
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
    command: `pnpm dev --port ${PORT} --strictPort`,
    url: `http://localhost:${PORT}`,
    reuseExistingServer: !process.env.CI,
    stdout: "ignore",
    stderr: "pipe",
  },
});
