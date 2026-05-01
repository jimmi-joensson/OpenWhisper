import { expect, test, waitForPermissionsStatusListener } from "./fixtures/tauri-shim";

test.describe("home pane", () => {
  test("renders hero with live hotkey hint", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByRole("heading", { name: "Ready when you are" })).toBeVisible();
    // Mac default toggle = RightCommand modifier-tap → "Right ⌘".
    await expect(page.getByTestId("home-hotkey-hint")).toContainText("Right ⌘");
    await expect(page.getByTestId("home-app-icon")).toBeVisible();
  });

  test("hotkey hint updates when binding changes", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByTestId("home-hotkey-hint")).toContainText("Right ⌘");

    // Simulate a rebind to Right Shift via the hotkey_captured event.
    await page.evaluate(() => {
      window.__owEmit("hotkey_captured", {
        target: "toggle",
        config: { kind: "modifier-tap", code: "RightShift", mods: [] },
      });
    });
    await expect(page.getByTestId("home-hotkey-hint")).toContainText("Right ⇧");
  });

  test("mic permission banner renders above the hero", async ({ page }) => {
    await page.goto("/");
    await waitForPermissionsStatusListener(page);

    // Push a denied snapshot via the permissions_status event the hook listens to.
    await page.evaluate(() =>
      window.__owEmit("permissions_status", {
        mic_ok: false,
        mic_state: "denied",
        error: "Microphone access denied.",
      }),
    );

    const banner = page.getByTestId("mic-banner");
    const hero = page.getByRole("heading", { name: "Ready when you are" });
    await expect(banner).toBeVisible();
    const bannerBox = await banner.boundingBox();
    const heroBox = await hero.boundingBox();
    expect(bannerBox && heroBox && bannerBox.y < heroBox.y).toBeTruthy();
  });
});
