import { expect, test } from "./fixtures/tauri-shim";

test.describe("window controls (Windows-only)", () => {
  test("renders min/max/close on Windows", async ({ page }) => {
    await page.addInitScript(() => {
      Object.defineProperty(navigator, "platform", { value: "Win32" });
    });
    await page.goto("/");
    await expect(page.getByTestId("window-control-minimize")).toBeVisible();
    await expect(page.getByTestId("window-control-maximize")).toBeVisible();
    await expect(page.getByTestId("window-control-close")).toBeVisible();
  });

  test("does not render on Mac", async ({ page }) => {
    // Default tauri-shim runs as Mac platform; no init override.
    await page.goto("/");
    await expect(page.getByTestId("window-control-close")).toHaveCount(0);
  });

  test("clicking min/max/close invokes the matching window IPC", async ({ page }) => {
    await page.addInitScript(() => {
      Object.defineProperty(navigator, "platform", { value: "Win32" });
    });
    await page.goto("/");

    await page.getByTestId("window-control-minimize").click();
    await page.getByTestId("window-control-maximize").click();
    await page.getByTestId("window-control-close").click();

    const calls = await page.evaluate(
      () => (window as unknown as { __owWindowCalls?: string[] }).__owWindowCalls ?? [],
    );
    expect(calls).toEqual(
      expect.arrayContaining(["minimize", "toggle_maximize", "close"]),
    );
  });
});
