import {
  emitTick,
  expect,
  test,
  waitForTickListener,
} from "./fixtures/tauri-shim";

type Page = import("@playwright/test").Page;

interface Entry {
  id: string;
  at_ms: number;
  duration_ms: number;
  word_count: number;
  text: string;
  original: string | null;
  app_id: string | null;
  app_name: string | null;
}

interface Seed {
  settings?: { enabled?: boolean; retention_days?: number; folder?: string | null };
  // Built in the page so "today" and "yesterday" follow its clock.
  entries?: (Omit<Entry, "at_ms" | "duration_ms" | "word_count"> & { hoursAgo: number })[];
}

async function seed(page: Page, data: Seed) {
  await page.addInitScript((d: Seed) => {
    const w = window as unknown as {
      __owHistorySettings?: unknown;
      __owHistoryEntries?: unknown[];
    };
    w.__owHistorySettings = d.settings;
    const noon = new Date();
    noon.setHours(12, 0, 0, 0);
    w.__owHistoryEntries = (d.entries ?? []).map(({ hoursAgo, ...e }) => ({
      ...e,
      at_ms: noon.getTime() - hoursAgo * 3_600_000,
      duration_ms: 4000,
      word_count: e.text.split(/\s+/).length,
    }));
  }, data);
}

// Read a value the shim recorded on `window`.
function recorded<T>(page: Page, key: string): Promise<T | undefined> {
  return page.evaluate(
    (k) => (window as unknown as Record<string, unknown>)[k] as T | undefined,
    key,
  );
}

const THREE = [
  { id: "e3", hoursAgo: 1, text: "Vi ses på Ærø i morgen", original: null, app_id: "com.example.chat", app_name: "Chatter" },
  {
    id: "e2",
    hoursAgo: 2,
    text: "Send the report to the team before Friday.",
    original: "send the reaport to the team before friday",
    app_id: null,
    app_name: null,
  },
  { id: "e1", hoursAgo: 26, text: "Remember to book the meeting room.", original: null, app_id: "com.example.mail", app_name: "Mailbox" },
];

async function openGeneral(page: Page) {
  await page.waitForSelector('[data-testid="sidebar-item-home"]');
  await page.evaluate(() => window.__owEmit("ow_navigate", "settings"));
  await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
}

function many(count: number) {
  return Array.from({ length: count }, (_, i) => ({
    id: `e${1000 - i}`,
    hoursAgo: i,
    text: `Dictation number ${i + 1}`,
    original: null,
    app_id: null,
    app_name: null,
  }));
}

async function openHistory(page: Page) {
  await page.getByTestId("sidebar-item-history").click();
  await expect(page.getByTestId("history-pane")).toBeVisible();
}

test.describe("home: history", () => {
  test("is off by default and shows no list", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByTestId("home-history-switch")).not.toBeChecked();
    await expect(page.getByTestId("home-history")).toHaveCount(0);
  });

  test("the quick switch turns history on and shows the empty list", async ({ page }) => {
    await page.goto("/");
    await page.getByTestId("home-history-switch").click();
    await expect(page.getByTestId("home-history-switch")).toBeChecked();
    await expect
      .poll(async () => (await recorded<{ enabled: boolean }[]>(page, "__owHistorySaves"))?.at(-1)?.enabled)
      .toBe(true);
    await expect(page.getByTestId("history-empty")).toContainText("Nothing saved yet");
    await expect(page.getByTestId("history-see-all")).toHaveCount(0);
  });

  test("shows only the newest five and links to the rest", async ({ page }) => {
    await seed(page, { settings: { enabled: true }, entries: many(8) });
    await page.goto("/");

    const rows = page.getByTestId("history-row");
    await expect(rows).toHaveCount(5);
    await expect(rows.nth(0)).toContainText("Dictation number 1");
    await expect(rows.nth(4)).toContainText("Dictation number 5");

    await expect(page.getByTestId("history-see-all")).toHaveText("See all 8");
    await page.getByTestId("history-see-all").click();
    await expect(page.getByTestId("history-pane")).toBeVisible();
    await expect(page.getByTestId("history-row")).toHaveCount(8);
  });

  test("groups by day and shows time and app", async ({ page }) => {
    await seed(page, { settings: { enabled: true }, entries: THREE });
    await page.goto("/");

    const rows = page.getByTestId("history-row");
    await expect(rows).toHaveCount(3);
    await expect(rows.nth(0).getByTestId("history-row-app")).toHaveText("Chatter");
    await expect(rows.nth(1).getByTestId("history-row-app")).toHaveCount(0);
    const history = page.getByTestId("home-history");
    await expect(history.getByRole("heading", { name: "Today" })).toBeVisible();
    await expect(history.getByRole("heading", { name: "Yesterday" })).toBeVisible();
    // Everything fits on Home, so the link just opens the screen.
    await expect(page.getByTestId("history-see-all")).toHaveText("Open history");
  });

  test("deleting an entry on Home removes it", async ({ page }) => {
    await seed(page, { settings: { enabled: true }, entries: THREE });
    await page.goto("/");
    const row = page.getByTestId("history-row").nth(1);
    await row.hover();
    await row.getByTestId("history-row-delete").click();

    await expect(page.getByTestId("history-row")).toHaveCount(2);
    await expect(page.getByTestId("home-history")).not.toContainText("Send the report");
    expect(await recorded<string[]>(page, "__owHistoryDeleted")).toEqual(["e2"]);
  });

  test("with history off, stored text stays visible with a note", async ({ page }) => {
    await seed(page, { settings: { enabled: false }, entries: THREE });
    await page.goto("/");
    await expect(page.getByTestId("history-off-note")).toContainText("New dictations are not saved");
    await expect(page.getByTestId("history-row")).toHaveCount(3);
  });

  test("the dictation that just finished is the first entry, with its notes", async ({ page }) => {
    await seed(page, { settings: { enabled: true }, entries: THREE });
    await page.goto("/");
    await waitForTickListener(page);
    await expect(page.getByTestId("history-row")).toHaveCount(3);

    // The dictation that is the newest stored entry finishes.
    await emitTick(page, { phase: 3, status: "transcribing" });
    await emitTick(page, {
      phase: 4,
      status: "idle",
      transcript: "Vi ses på Ærø i morgen ",
      replacements: [{ from: "aero", to: "Ærø" }],
      refine: {
        raw_transcript: "vi ses på aero i morgen ",
        status: "refined",
        detail: "",
        backend: "claude-cli",
        elapsed_ms: 800,
      },
    });

    // Shown once: in the list, not in a row of its own above it.
    const first = page.getByTestId("history-row").nth(0);
    await expect(first).toHaveAttribute("data-latest", "true");
    await expect(first).toContainText("Vi ses på Ærø i morgen");
    await expect(first.getByTestId("home-latest-fixes")).toContainText("Ærø");
    await expect(first.getByTestId("home-latest-refine")).toContainText("Refined");
    await expect(page.getByTestId("history-row")).toHaveCount(3);
    await expect(page.getByTestId("home-latest-row")).toHaveCount(0);

    // The original is one click away, like on the row of its own.
    await first.getByRole("button", { name: "Show original" }).click();
    await expect(first.getByTestId("home-latest-original")).toHaveText("vi ses på aero i morgen");
  });

  test("with history off, the latest transcript keeps its own row", async ({ page }) => {
    await page.goto("/");
    await waitForTickListener(page);
    await emitTick(page, { phase: 3, status: "transcribing" });
    await emitTick(page, { phase: 4, status: "idle", transcript: "Not stored anywhere " });
    await expect(page.getByTestId("home-latest-row")).toContainText("Not stored anywhere");
    await expect(page.getByTestId("home-history")).toHaveCount(0);
  });

  test("a finished dictation that was not stored still shows, in its own row", async ({ page }) => {
    await seed(page, { settings: { enabled: true }, entries: THREE });
    await page.goto("/");
    await waitForTickListener(page);
    await emitTick(page, { phase: 3, status: "transcribing" });
    await emitTick(page, { phase: 4, status: "idle", transcript: "This one never reached the store " });
    // After the short wait for the list, Home falls back to the row.
    await expect(page.getByTestId("home-latest-row")).toContainText("This one never reached the store");
    await expect(page.getByTestId("history-row")).toHaveCount(3);
  });

  test("row buttons show on hover and hide again, also after a click", async ({ page }) => {
    await seed(page, { settings: { enabled: true }, entries: THREE });
    await page.goto("/");
    const rows = page.getByTestId("history-row");
    const copy = (i: number) => rows.nth(i).getByTestId("history-row-copy");

    await expect(copy(0)).toBeHidden();
    await rows.nth(0).hover();
    await expect(copy(0)).toBeVisible();
    await expect(copy(1)).toBeHidden();

    await rows.nth(1).hover();
    await expect(copy(0)).toBeHidden();
    await expect(copy(1)).toBeVisible();

    // A clicked button keeps focus in Chromium (the Windows webview);
    // that must not keep the buttons on screen.
    await copy(1).click();
    await page.mouse.move(2, 2);
    await expect(copy(1)).toBeHidden();
    await expect(copy(0)).toBeHidden();
  });
});

test.describe("history screen", () => {
  test("opens from the sidebar and lists everything by day", async ({ page }) => {
    await seed(page, { settings: { enabled: true }, entries: THREE });
    await page.goto("/");
    await openHistory(page);

    const pane = page.getByTestId("history-pane");
    await expect(pane.getByRole("heading", { name: "History", exact: true })).toBeVisible();
    await expect(page.getByTestId("history-row")).toHaveCount(3);
    await expect(pane.getByRole("heading", { name: "Today" })).toBeVisible();
    await expect(pane.getByRole("heading", { name: "Yesterday" })).toBeVisible();
    await expect(page.getByTestId("history-count")).toHaveText("3 saved");
    // One page: no pager.
    await expect(page.getByTestId("history-next")).toHaveCount(0);
  });

  test("shows where the files are and opens the folder", async ({ page }) => {
    await page.goto("/");
    await openHistory(page);
    await expect(page.getByTestId("history-pane-folder")).toHaveText(
      "/Users/test/Library/Application Support/com.openwhisper.app/history",
    );
    await page.getByTestId("history-pane-reveal").click();
    await expect.poll(() => recorded<number>(page, "__owHistoryRevealCount")).toBe(1);
  });

  test("shows the folder the user chose", async ({ page }) => {
    await seed(page, { settings: { enabled: true, folder: "/Users/test/Notes/Dictations" } });
    await page.goto("/");
    await openHistory(page);
    await expect(page.getByTestId("history-pane-folder")).toHaveText("/Users/test/Notes/Dictations");
  });

  test("search narrows the list and says when nothing matches", async ({ page }) => {
    await seed(page, { settings: { enabled: true }, entries: THREE });
    await page.goto("/");
    await openHistory(page);
    const search = page.getByTestId("history-search");

    // Case-insensitive, Danish letters included.
    await search.fill("ÆRØ");
    await expect(page.getByTestId("history-row")).toHaveCount(1);
    await expect(page.getByTestId("history-row")).toContainText("Vi ses på Ærø");
    await expect(page.getByTestId("history-count")).toHaveText("1 of 3 saved");

    // The text before Refine is searched too.
    await search.fill("reaport");
    await expect(page.getByTestId("history-row")).toContainText("Send the report");

    await search.fill("no such words");
    await expect(page.getByTestId("history-no-match")).toContainText("no such words");
    await expect(page.getByTestId("history-row")).toHaveCount(0);

    await search.fill("");
    await expect(page.getByTestId("history-row")).toHaveCount(3);
  });

  test("an entry Refine changed offers the original", async ({ page }) => {
    await seed(page, { settings: { enabled: true }, entries: THREE });
    await page.goto("/");
    await openHistory(page);
    const row = page.getByTestId("history-row").nth(1);

    await expect(row.getByTestId("history-row-original")).toHaveCount(0);
    await row.getByRole("button", { name: "Show original" }).click();
    await expect(row.getByTestId("history-row-original")).toHaveText(
      "send the reaport to the team before friday",
    );
    await row.getByRole("button", { name: "Hide original" }).click();
    await expect(row.getByTestId("history-row-original")).toHaveCount(0);

    // An entry Refine did not change has no such button.
    await expect(
      page.getByTestId("history-row").nth(0).getByRole("button", { name: "Show original" }),
    ).toHaveCount(0);
  });

  test("pages through the history", async ({ page }) => {
    await seed(page, { settings: { enabled: true }, entries: many(60) });
    await page.goto("/");
    await openHistory(page);

    const rows = page.getByTestId("history-row");
    const label = page.getByTestId("history-page-label");
    await expect(rows).toHaveCount(25);
    await expect(label).toHaveText("Page 1 of 3");
    await expect(page.getByTestId("history-prev")).toBeDisabled();
    await expect(rows.nth(0)).toContainText("Dictation number 1");

    await page.getByTestId("history-next").click();
    await expect(label).toHaveText("Page 2 of 3");
    await expect(rows.nth(0)).toContainText("Dictation number 26");

    await page.getByTestId("history-next").click();
    await expect(label).toHaveText("Page 3 of 3");
    await expect(rows).toHaveCount(10);
    await expect(page.getByTestId("history-next")).toBeDisabled();

    await page.getByTestId("history-prev").click();
    await expect(label).toHaveText("Page 2 of 3");
  });

  test("a search starts again at the first page", async ({ page }) => {
    await seed(page, { settings: { enabled: true }, entries: many(60) });
    await page.goto("/");
    await openHistory(page);
    await page.getByTestId("history-next").click();
    await expect(page.getByTestId("history-page-label")).toHaveText("Page 2 of 3");

    // "number 1", "number 10" to "number 19": eleven matches, one page.
    await page.getByTestId("history-search").fill("number 1");
    await expect(page.getByTestId("history-count")).toHaveText("11 of 60 saved");
    await expect(page.getByTestId("history-row")).toHaveCount(11);
    await expect(page.getByTestId("history-next")).toHaveCount(0);
  });

  test("deleting the last entry of the last page steps back a page", async ({ page }) => {
    await seed(page, { settings: { enabled: true }, entries: many(26) });
    await page.goto("/");
    await openHistory(page);
    await page.getByTestId("history-next").click();
    const rows = page.getByTestId("history-row");
    await expect(rows).toHaveCount(1);

    await rows.nth(0).hover();
    await rows.nth(0).getByTestId("history-row-delete").click();
    await expect(rows).toHaveCount(25);
    await expect(page.getByTestId("history-count")).toHaveText("25 saved");
    await expect(page.getByTestId("history-next")).toHaveCount(0);
  });

  test("clear history asks first, then removes everything", async ({ page }) => {
    await seed(page, { settings: { enabled: true }, entries: THREE });
    await page.goto("/");
    await openHistory(page);

    await page.getByTestId("history-clear-trigger").click();
    await expect(page.getByRole("alertdialog")).toContainText("all 3 saved dictations");
    await page.getByRole("button", { name: "Cancel" }).click();
    await expect(page.getByTestId("history-row")).toHaveCount(3);
    expect(await recorded<number>(page, "__owHistoryClearCount")).toBeUndefined();

    await page.getByTestId("history-clear-trigger").click();
    await page.getByTestId("history-clear-confirm").click();
    await expect(page.getByTestId("history-row")).toHaveCount(0);
    await expect(page.getByTestId("history-empty")).toBeVisible();
    expect(await recorded<number>(page, "__owHistoryClearCount")).toBe(1);
  });

  test("a long entry is folded until asked for", async ({ page }) => {
    const long = `${"This sentence makes the entry long. ".repeat(12)}The end.`;
    await seed(page, {
      settings: { enabled: true },
      entries: [{ id: "e1", hoursAgo: 1, text: long, original: null, app_id: null, app_name: null }],
    });
    await page.goto("/");
    await openHistory(page);
    const row = page.getByTestId("history-row");
    const text = row.locator(".ow-history-row__text");
    await expect(text).toHaveAttribute("data-folded", "true");
    await row.getByRole("button", { name: "Show all" }).click();
    await expect(text).not.toHaveAttribute("data-folded", "true");
    await expect(row.getByRole("button", { name: "Show less" })).toBeVisible();
  });

  test("with history off it says so and links to the setting", async ({ page }) => {
    await page.goto("/");
    await openHistory(page);
    await expect(page.getByTestId("history-off-note")).toContainText("History is off");
    await expect(page.getByTestId("history-empty")).toContainText("Switch history on");
    await page.getByTestId("history-open-settings").click();
    await expect(page.getByTestId("history-enabled-switch")).toBeVisible();
  });

  test("still lists and clears stored text while history is off", async ({ page }) => {
    await seed(page, { settings: { enabled: false }, entries: THREE });
    await page.goto("/");
    await openHistory(page);
    await expect(page.getByTestId("history-row")).toHaveCount(3);
    await expect(page.getByTestId("history-clear-trigger")).toBeVisible();
  });
});

test.describe("home: quick switches", () => {
  test("the Refine switch follows the saved setting and saves a change", async ({ page }) => {
    await page.addInitScript(() => {
      (window as unknown as { __owRefineSettings: unknown }).__owRefineSettings = { enabled: true };
    });
    await page.goto("/");
    const refine = page.getByTestId("home-refine-switch");
    await expect(refine).toBeChecked();

    await refine.click();
    await expect(refine).not.toBeChecked();
    await expect
      .poll(async () => (await recorded<{ enabled: boolean }[]>(page, "__owRefineSaves"))?.at(-1)?.enabled)
      .toBe(false);
    // Home does not start the backend probe the Refine pane runs.
    expect(await recorded<number>(page, "__owRefineStatusCount")).toBeUndefined();
  });

  test("both switches say where the text goes", async ({ page }) => {
    await page.goto("/");
    const switches = page.getByTestId("home-switches");
    await expect(switches).toContainText("The text leaves this computer");
    await expect(switches).toContainText("on this computer only");
  });

  test("the Refine card opens the Refine settings", async ({ page }) => {
    await page.goto("/");
    await page.getByTestId("home-refine-settings").click();
    await expect(page.getByRole("heading", { name: "Refine", exact: true })).toBeVisible();
    await expect(page.getByRole("switch", { name: "Refine dictations" })).toBeVisible();
  });

  test("the History card opens the History settings, scrolled into view", async ({ page }) => {
    // A short window, so the section is below the fold in General.
    await page.setViewportSize({ width: 900, height: 420 });
    await page.goto("/");
    await page.getByTestId("home-history-settings").click();
    await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
    await expect(page.getByTestId("history-enabled-switch")).toBeInViewport();
  });

  test("the Refine pane shows what Home saved", async ({ page }) => {
    await page.goto("/");
    await page.getByTestId("home-refine-switch").click();
    await expect(page.getByTestId("home-refine-switch")).toBeChecked();
    await openGeneral(page);
    await page.getByRole("tab", { name: "Refine" }).click();
    await expect(page.getByRole("switch", { name: "Refine dictations" })).toBeChecked();
  });
});

test.describe("settings: history section", () => {
  test("shows the switch, the retention and where the text is stored", async ({ page }) => {
    await page.goto("/");
    await openGeneral(page);
    await expect(page.getByTestId("history-enabled-switch")).not.toBeChecked();
    await expect(page.getByTestId("history-retention")).toContainText("30 days");
    await expect(page.getByTestId("history-location")).toHaveText(
      "/Users/test/Library/Application Support/com.openwhisper.app/history",
    );
    // The default folder needs no sync warning and no way back.
    await expect(page.getByTestId("history-folder-note")).toHaveCount(0);
    await expect(page.getByTestId("history-default-folder")).toHaveCount(0);
  });

  test("the switch and the retention save", async ({ page }) => {
    await page.goto("/");
    await openGeneral(page);

    await page.getByTestId("history-enabled-switch").click();
    await expect(page.getByTestId("history-enabled-switch")).toBeChecked();

    await page.getByTestId("history-retention").click();
    await page.getByRole("option", { name: "90 days" }).click();
    await expect(page.getByTestId("history-retention")).toContainText("90 days");

    await expect
      .poll(async () => (await recorded<unknown[]>(page, "__owHistorySaves"))?.at(-1))
      .toEqual({ enabled: true, retention_days: 90, folder: null });
  });

  test("the folder can be changed, opened and put back", async ({ page }) => {
    await page.addInitScript(() => {
      (window as unknown as { __owDialogPick: string }).__owDialogPick = "/Users/test/Notes/Dictations";
    });
    await page.goto("/");
    await openGeneral(page);

    await page.getByTestId("history-choose-folder").click();
    await expect(page.getByTestId("history-location")).toHaveText("/Users/test/Notes/Dictations");
    await expect(page.getByTestId("history-folder-note")).toContainText("synced");
    await expect(page.getByTestId("history-folder-note")).toContainText("not moved");
    await expect
      .poll(async () => (await recorded<{ folder: string | null }[]>(page, "__owHistorySaves"))?.at(-1)?.folder)
      .toBe("/Users/test/Notes/Dictations");
    // The picker was asked for a folder, not a file.
    expect(await recorded<{ directory?: boolean }>(page, "__owDialogOptions")).toMatchObject({
      directory: true,
    });

    await page.getByTestId("history-reveal").click();
    await expect.poll(() => recorded<number>(page, "__owHistoryRevealCount")).toBe(1);

    await page.getByTestId("history-default-folder").click();
    await expect(page.getByTestId("history-location")).toHaveText(
      "/Users/test/Library/Application Support/com.openwhisper.app/history",
    );
    await expect(page.getByTestId("history-folder-note")).toHaveCount(0);
  });

  test("a dismissed folder picker changes nothing", async ({ page }) => {
    await page.goto("/");
    await openGeneral(page);
    await page.getByTestId("history-choose-folder").click();
    await expect(page.getByTestId("history-location")).toContainText("com.openwhisper.app/history");
    expect(await recorded<unknown[]>(page, "__owHistorySaves")).toBeUndefined();
  });

  test("Home shows what Settings saved", async ({ page }) => {
    await page.goto("/");
    await openGeneral(page);
    await page.getByTestId("history-enabled-switch").click();
    await expect(page.getByTestId("history-enabled-switch")).toBeChecked();
    await page.evaluate(() => window.__owEmit("ow_navigate", "main"));
    await expect(page.getByTestId("home-history-switch")).toBeChecked();
    await expect(page.getByTestId("history-empty")).toBeVisible();
  });

  test("a retention set from the CLI is shown as it is", async ({ page }) => {
    await seed(page, { settings: { enabled: true, retention_days: 14 } });
    await page.goto("/");
    await openGeneral(page);
    await expect(page.getByTestId("history-retention")).toContainText("14 days");
  });

  test("resetting stats says the history is not touched", async ({ page }) => {
    await page.goto("/");
    await openGeneral(page);
    await page.getByTestId("stats-reset-trigger").click();
    await expect(page.getByRole("alertdialog")).toContainText("Saved transcript history is not touched");
  });
});
