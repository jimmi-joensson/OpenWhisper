import { expect, test } from "./fixtures/tauri-shim";

test.describe("sidebar nav", () => {
  test("clicking sidebar items switches the visible pane", async ({ page }) => {
    await page.goto("/");
    // Default route is Home.
    await expect(page.getByTestId("sidebar-item-home")).toHaveAttribute("aria-current", "page");
    await expect(page.getByRole("heading", { name: "Ready when you are" })).toBeVisible();

    // Click Diagnostics — debug content visible.
    await page.getByTestId("sidebar-item-diagnostics").click();
    await expect(page.getByTestId("sidebar-item-diagnostics")).toHaveAttribute(
      "aria-current",
      "page",
    );
    await expect(page.getByText("Rust ↔ React FFI")).toBeVisible();

    // Click Settings — existing settings shell renders.
    await page.getByTestId("sidebar-item-settings").click();
    await expect(page.getByTestId("sidebar-item-settings")).toHaveAttribute(
      "aria-current",
      "page",
    );
    await expect(page.getByRole("tab", { name: "General" })).toBeVisible();

    // Click Home — sidebar marks Home active and hero is back.
    await page.getByTestId("sidebar-item-home").click();
    await expect(page.getByTestId("sidebar-item-home")).toHaveAttribute("aria-current", "page");
    await expect(page.getByRole("heading", { name: "Ready when you are" })).toBeVisible();
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

    await expect(page.getByText("transcript", { exact: true })).toBeInViewport();
  });

  test("transcript Card visible without scroll at default 720x820", async ({ page }) => {
    await page.setViewportSize({ width: 720, height: 820 });
    await page.goto("/");
    await page.getByTestId("sidebar-item-diagnostics").click();
    await expect(page.getByText("transcript", { exact: true })).toBeInViewport();
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
