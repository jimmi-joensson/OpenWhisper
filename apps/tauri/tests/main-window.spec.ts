import { expect, test } from "./fixtures/tauri-shim";

test.describe("sidebar nav", () => {
  test("route sidebar (Home/Settings/Diagnostics) on home + diagnostics", async ({ page }) => {
    await page.goto("/");
    // Default route is Home.
    await expect(page.getByTestId("sidebar-item-home")).toHaveAttribute("aria-current", "page");
    await expect(page.getByRole("heading", { name: "Ready when you are" })).toBeVisible();

    // Click Diagnostics — pane visible, sidebar still shows the three routes.
    await page.getByTestId("sidebar-item-diagnostics").click();
    await expect(page.getByTestId("sidebar-item-diagnostics")).toHaveAttribute(
      "aria-current",
      "page",
    );
    await expect(
      page.getByRole("heading", { name: "Diagnostics" }),
    ).toBeVisible();
    await expect(page.getByTestId("sidebar-item-home")).toBeVisible();
    await expect(page.getByTestId("sidebar-item-settings")).toBeVisible();

    // Click Home — sidebar marks Home active and hero is back.
    await page.getByTestId("sidebar-item-home").click();
    await expect(page.getByTestId("sidebar-item-home")).toHaveAttribute("aria-current", "page");
    await expect(page.getByRole("heading", { name: "Ready when you are" })).toBeVisible();
  });

  test("entering Settings replaces the sidebar with the pane chooser; back restores it", async ({
    page,
  }) => {
    await page.goto("/");

    // Enter Settings — sidebar swaps to General/Audio/Models/Shortcuts;
    // route-level items disappear.
    await page.getByTestId("sidebar-item-settings").click();
    await expect(page.getByRole("tab", { name: "General" })).toBeVisible();
    await expect(page.getByRole("tab", { name: "Shortcuts" })).toBeVisible();
    await expect(page.getByTestId("sidebar-item-home")).toHaveCount(0);
    await expect(page.getByTestId("sidebar-item-diagnostics")).toHaveCount(0);

    // Back arrow restores the outer route sidebar.
    await page.getByRole("button", { name: "Back to main" }).click();
    await expect(page.getByRole("tab", { name: "General" })).toHaveCount(0);
    await expect(page.getByTestId("sidebar-item-home")).toHaveAttribute(
      "aria-current",
      "page",
    );
    await expect(page.getByRole("heading", { name: "Ready when you are" })).toBeVisible();
  });

  test("sidebar starts at window top; titlebar inset over content column", async ({ page }) => {
    await page.goto("/");
    const sidebarBox = await page.getByTestId("sidebar-item-home").boundingBox();
    // Sidebar's first item is within the top 60 px of the window — i.e. the
    // sidebar column starts at or near y=0.
    expect(sidebarBox && sidebarBox.y).toBeLessThan(60);

    // Settings titlebar back-arrow lives inside the content column (x>150),
    // not full-width above the sidebar.
    await page.getByTestId("sidebar-item-settings").click();
    const back = page.getByRole("button", { name: "Back to main" });
    const backBox = await back.boundingBox();
    expect(backBox && backBox.x).toBeGreaterThan(150);
  });

  test("re-entering Settings resets to General regardless of last pane", async ({ page }) => {
    await page.goto("/");

    // First visit: navigate Settings → Shortcuts.
    await page.getByTestId("sidebar-item-settings").click();
    await page.getByRole("tab", { name: "Shortcuts" }).click();
    await expect(page.getByRole("tab", { name: "Shortcuts" })).toHaveAttribute(
      "aria-selected",
      "true",
    );

    // Leave Settings via the back arrow.
    await page.getByRole("button", { name: "Back to main" }).click();
    await expect(page.getByRole("heading", { name: "Ready when you are" })).toBeVisible();

    // Re-enter Settings — General is active again, not Shortcuts.
    await page.getByTestId("sidebar-item-settings").click();
    await expect(page.getByRole("tab", { name: "General" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await expect(page.getByRole("tab", { name: "Shortcuts" })).toHaveAttribute(
      "aria-selected",
      "false",
    );
  });
});

test.describe("scroll", () => {
  test(".ow-app__body scrolls when content overflows the viewport", async ({ page }) => {
    await page.setViewportSize({ width: 600, height: 500 });
    await page.goto("/");
    // Diagnostics has the densest content; force overflow there.
    await page.getByTestId("sidebar-item-diagnostics").click();

    // Window titlebar strip stays fixed; scroll happens inside
    // `.ow-app__body` so the strip never scrolls out of view.
    const probe = await page.evaluate(() => {
      const body = document.querySelector(".ow-app__body") as HTMLElement;
      return {
        clientH: body.clientHeight,
        scrollH: body.scrollHeight,
        overflowY: getComputedStyle(body).overflowY,
      };
    });

    expect(probe.overflowY).toBe("auto");
    expect(probe.scrollH).toBeGreaterThan(probe.clientH);

    const scrolled = await page.evaluate(() => {
      const body = document.querySelector(".ow-app__body") as HTMLElement;
      body.scrollTop = body.scrollHeight;
      return body.scrollTop;
    });
    expect(scrolled).toBeGreaterThan(0);

    // Footer caveat lives at the very bottom of the diagnostics pane;
    // scrolling to the end brings it into view.
    await expect(
      page.getByText(/Per-model RAM is an RSS-delta estimate/),
    ).toBeInViewport();
  });

  test("Memory card visible without scroll at default 720x820", async ({ page }) => {
    await page.setViewportSize({ width: 720, height: 820 });
    await page.goto("/");
    await page.getByTestId("sidebar-item-diagnostics").click();
    await expect(page.getByText("Memory", { exact: true })).toBeInViewport();
  });
});

test.describe("text selection", () => {
  test("chrome (sidebar) is non-selectable", async ({ page }) => {
    await page.goto("/");

    const sidebarSelect = await page
      .getByTestId("sidebar-item-home")
      .evaluate((el) => getComputedStyle(el).userSelect);
    expect(sidebarSelect).toBe("none");
  });
});
