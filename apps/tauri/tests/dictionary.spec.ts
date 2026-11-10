import { expect, test } from "./fixtures/tauri-shim";

async function openDictionary(page: import("@playwright/test").Page) {
  await page.waitForSelector('[data-testid="sidebar-item-home"]');
  await page.evaluate(() => window.__owEmit("ow_navigate", "settings"));
  await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
  await page.getByRole("tab", { name: "Dictionary" }).click();
  await expect(page.getByRole("heading", { name: "Dictionary" })).toBeVisible();
}

test.describe("dictionary pane", () => {
  test("shows the empty state and the file path", async ({ page }) => {
    await page.goto("/");
    await openDictionary(page);
    await expect(page.getByText("No words yet.")).toBeVisible();
    await expect(page.getByText(/dictionary\.json$/)).toBeVisible();
  });

  test("adds a word with what it was heard as", async ({ page }) => {
    await page.goto("/");
    await openDictionary(page);
    await page.getByRole("textbox", { name: "Word" }).fill("PayProff");
    await page.getByRole("textbox", { name: "Heard as" }).fill("pay prof, pay proof");
    await page.getByRole("button", { name: "Add" }).click();

    const list = page.getByRole("list", { name: "Dictionary words" });
    await expect(list.getByText("PayProff", { exact: true })).toBeVisible();
    await expect(list.getByText("Heard as pay prof, pay proof")).toBeVisible();
    await expect(page.getByRole("textbox", { name: "Word" })).toHaveValue("");
    await expect(page.getByRole("textbox", { name: "Heard as" })).toHaveValue("");
  });

  test("Add is disabled without a word", async ({ page }) => {
    await page.goto("/");
    await openDictionary(page);
    await expect(page.getByRole("button", { name: "Add" })).toBeDisabled();
  });

  test("lists stored words and removes one", async ({ page }) => {
    await page.addInitScript(() => {
      (window as unknown as { __owDictionary: unknown }).__owDictionary = [
        { term: "Emanoil", heard_as: ["Immanuel"] },
        { term: "Jeppe", heard_as: ["hipp"] },
      ];
    });
    await page.goto("/");
    await openDictionary(page);
    const list = page.getByRole("list", { name: "Dictionary words" });
    await expect(list.getByRole("listitem")).toHaveCount(2);

    await page.getByRole("button", { name: "Remove Emanoil" }).click();
    await expect(list.getByRole("listitem")).toHaveCount(1);
    await expect(list.getByText("Jeppe", { exact: true })).toBeVisible();
  });

  test("Show file reveals the dictionary file", async ({ page }) => {
    await page.goto("/");
    await openDictionary(page);
    await page.getByRole("button", { name: "Show file" }).click();
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owDictionaryRevealCount?: number })
              .__owDictionaryRevealCount ?? 0,
        ),
      )
      .toBe(1);
  });
});
