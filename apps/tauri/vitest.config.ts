import { defineConfig } from "vitest/config";

// Vitest scope = unit tests under `src/`. The Playwright spec files in
// `tests/*.spec.ts` import `@playwright/test` and live in their own
// runner — exclude them from vitest discovery.
export default defineConfig({
  test: {
    include: ["src/**/*.test.{ts,tsx}"],
    exclude: ["tests/**", "node_modules/**", "dist/**"],
    environment: "node",
    globals: false,
  },
});
