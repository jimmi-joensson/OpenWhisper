import {
  answerTest,
  emitAskTick,
  emitVisionChanged,
  expect,
  pillTest,
  test,
  waitForAskTickListener,
  waitForVisionListener,
  type MockVisionSettings,
} from "./fixtures/tauri-shim";

type Page = import("@playwright/test").Page;

// Vision (TASK-98.4): an ask can include a picture of the screen.
// The Screen section of the Ask pane, what the answer window and the
// pill say while and after the screen is looked at, and the marker on
// asks in the history.
//
// Nothing here takes a screenshot: the shim has no command for one,
// like Rust. The built-in lists of the shim are short stand-ins:
// six phrases, and the made-up apps Vault and Keyring.

const DEFAULT_PHRASES = [
  "look at my screen",
  "on my screen",
  "this error",
  "se på min skærm",
  "på min skærm",
  "denne fejl",
];

interface Seed {
  // Ask is on unless a test says otherwise: the section is only in
  // use then.
  askEnabled?: boolean;
  settings?: Partial<MockVisionSettings>;
  permission?: "granted" | "denied" | "unknown";
  permissionAfterRequest?: "granted" | "denied";
  platform?: "macos" | "windows";
  running?: { id: string; name: string }[];
}

async function seed(page: Page, given: Seed = {}) {
  await page.addInitScript((s: Seed) => {
    const w = window as unknown as Record<string, unknown>;
    w.__owAskSettings = { enabled: s.askEnabled ?? true };
    if (s.settings) w.__owVisionSettings = s.settings;
    if (s.permission) w.__owVisionPermission = s.permission;
    if (s.permissionAfterRequest) w.__owVisionPermissionAfterRequest = s.permissionAfterRequest;
    if (s.platform) w.__owVisionPlatform = s.platform;
    if (s.running) w.__owRunningApps = s.running;
  }, given);
}

async function openAsk(page: Page) {
  await page.waitForSelector('[data-testid="sidebar-item-home"]');
  await page.evaluate(() => window.__owEmit("ow_navigate", "settings"));
  await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
  await page.getByRole("tab", { name: "Ask", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Screen", exact: true })).toBeVisible();
}

async function openScreen(page: Page, given: Seed = {}) {
  await seed(page, given);
  await page.goto("/");
  await openAsk(page);
  // The first read has answered.
  await expect(section(page).getByTestId("vision-never")).toBeVisible();
}

// Read a value the shim recorded on `window`.
function recorded<T>(page: Page, key: string): Promise<T | undefined> {
  return page.evaluate(
    (k) => (window as unknown as Record<string, unknown>)[k] as T | undefined,
    key,
  );
}

async function lastSave(page: Page): Promise<MockVisionSettings | undefined> {
  return (await recorded<MockVisionSettings[]>(page, "__owVisionSaves"))?.at(-1);
}

async function saves(page: Page): Promise<number> {
  return (await recorded<MockVisionSettings[]>(page, "__owVisionSaves"))?.length ?? 0;
}

// How often the window called a vision command.
async function calls(page: Page, cmd: string): Promise<number> {
  const all = (await recorded<{ cmd: string }[]>(page, "__owVisionCalls")) ?? [];
  return all.filter((c) => c.cmd === cmd).length;
}

// Pick an option of an open select and wait for its list to go away.
async function choose(page: Page, option: string) {
  await page.getByRole("option", { name: option, exact: true }).click();
  await expect(page.getByRole("option")).toHaveCount(0);
}

const section = (page: Page) => page.getByTestId("vision-section");
const triggerSelect = (page: Page) =>
  page.getByRole("combobox", { name: "When to include the screen" });
const phraseInput = (page: Page) => page.getByRole("textbox", { name: "Phrases" });
const phraseRows = (page: Page) => page.getByTestId("vision-phrase-row");
const appRows = (page: Page) => page.getByTestId("vision-never-row");
const appIdInput = (page: Page) => page.getByRole("textbox", { name: "App id" });
const openApps = (page: Page) => page.getByRole("combobox", { name: "Add an open app" });

test.describe("screen section: what it says", () => {
  test("opens with what a picture of the screen is, when it is taken and where it goes", async ({
    page,
  }) => {
    await openScreen(page);
    const intro = page.getByTestId("vision-intro");
    await expect(intro).toBeVisible();
    await expect(intro).toContainText(
      "An ask can include a picture of your whole screen (the display with the window you are working in), so you can refer to what you see.",
    );
    await expect(intro).toContainText(
      "The picture is only ever taken during an ask. It is never taken while you dictate and never in the background.",
    );
    await expect(intro).toContainText(
      "It is sent with your question to the same place as the question, kept in memory only and not saved.",
    );
  });

  test("says what the never-capture list does and, in plain sight, what it cannot cover", async ({
    page,
  }) => {
    await openScreen(page);
    await expect(page.getByTestId("vision-never-note")).toHaveText(
      "Apps on this list are left out of the picture, and the place where their windows are is painted over as well. When one of them is the app in front, no picture is taken at all and the answer says so.",
    );

    // On the page itself: no tooltip, nothing to open first.
    const cannot = page.getByTestId("vision-cannot-cover");
    await expect(cannot).toBeVisible();
    await expect(cannot).toContainText("What a list of apps cannot cover");
    await expect(cannot).toContainText(
      "A password manager's popup inside your browser belongs to the browser, not to the password manager, so it is in the picture.",
    );
    await expect(cannot).toContainText(
      "A password shown on a web page or in a terminal is part of that window, so it is in the picture too.",
    );
    await expect(cannot).toContainText("Close or hide these before you ask about your screen.");
    // A standing statement, not an alert that fires.
    await expect(cannot).toHaveAttribute("role", "note");
  });

  test("nothing in the section has a long dash in it", async ({ page }) => {
    await openScreen(page, { permission: "denied" });
    const text = await section(page).innerText();
    expect(text).not.toMatch(/[–—]/);
  });
});

test.describe("screen section: when to include the screen", () => {
  test("starts on When I say so and explains all four choices", async ({ page }) => {
    await openScreen(page);
    await expect(triggerSelect(page)).toContainText("When I say so");
    const choices = page.getByTestId("vision-trigger-choices").locator("li");
    await expect(choices).toHaveText([
      "Off: Never. No ask includes the screen.",
      "When I say so: Only when your question contains one of your phrases. The phrases are matched on this computer, before anything is sent.",
      "When needed: When your question contains one of your phrases, or when the model says it needs to see the screen to answer. The question is then sent first, without the picture.",
      "Always: Every ask includes the screen.",
    ]);
    await expect(
      page.getByTestId("vision-trigger-choices").locator('li[data-selected="true"]'),
    ).toHaveAttribute("data-choice", "phrases");
    // Reading the section saves nothing and asks for nothing.
    expect(await saves(page)).toBe(0);
    expect(await calls(page, "vision_request_permission")).toBe(0);
    expect(await calls(page, "vision_open_settings")).toBe(0);
  });

  test("each of the four choices saves", async ({ page }) => {
    await openScreen(page);
    await triggerSelect(page).click();
    await expect(page.getByRole("option")).toHaveText([
      "Off",
      "When I say so",
      "When needed",
      "Always",
    ]);
    await page.keyboard.press("Escape");
    await expect(page.getByRole("option")).toHaveCount(0);

    for (const [label, id] of [
      ["Off", "off"],
      ["When needed", "smart"],
      ["Always", "always"],
      ["When I say so", "phrases"],
    ] as const) {
      await triggerSelect(page).click();
      await choose(page, label);
      await expect(triggerSelect(page)).toContainText(label);
      await expect.poll(async () => (await lastSave(page))?.trigger).toBe(id);
      await expect(
        page.getByTestId("vision-trigger-choices").locator('li[data-selected="true"]'),
      ).toHaveAttribute("data-choice", id);
    }
    // The lists were never touched: they stay the built-in ones.
    const saved = await lastSave(page);
    expect(saved?.phrases).toBeNull();
    expect(saved?.never_capture).toBeNull();
  });

  test("the phrases are shown for When I say so and When needed only", async ({ page }) => {
    await openScreen(page);
    await expect(page.getByTestId("vision-phrases")).toBeVisible();

    await triggerSelect(page).click();
    await choose(page, "When needed");
    await expect(page.getByTestId("vision-phrases")).toBeVisible();

    await triggerSelect(page).click();
    await choose(page, "Always");
    await expect(page.getByTestId("vision-phrases")).toHaveCount(0);

    await triggerSelect(page).click();
    await choose(page, "Off");
    await expect(page.getByTestId("vision-phrases")).toHaveCount(0);
    // The never-capture list stays: it also holds for a later choice.
    await expect(page.getByTestId("vision-never")).toBeVisible();
  });

  test("a save that fails is undone and reported", async ({ page }) => {
    await openScreen(page);
    await page.evaluate(() => {
      (window as unknown as { __owVisionSetError?: string }).__owVisionSetError =
        "The settings file could not be written.";
    });
    await triggerSelect(page).click();
    await choose(page, "Always");
    await expect(section(page).getByRole("alert")).toContainText(
      "The settings file could not be written.",
    );
    await expect(triggerSelect(page)).toContainText("When I say so");
    await expect(page.getByTestId("vision-phrases")).toBeVisible();
  });
});

test.describe("screen section: phrases", () => {
  test("lists the built-in phrases and says how they are matched", async ({ page }) => {
    await openScreen(page);
    await expect(phraseRows(page)).toHaveText(DEFAULT_PHRASES);
    await expect(page.getByTestId("vision-phrases")).toContainText(
      "Matching ignores upper and lower case and punctuation.",
    );
    // Nothing to restore while the list is the built-in one.
    await expect(page.getByTestId("vision-phrases-restore")).toHaveCount(0);
  });

  test("Enter and the button add a phrase; one that is on the list is not added twice", async ({
    page,
  }) => {
    await openScreen(page);
    const add = page.getByRole("button", { name: "Add phrase" });
    await expect(add).toBeDisabled();

    await phraseInput(page).fill("  check   this window ");
    await phraseInput(page).press("Enter");
    // Tidied by Rust, and first on the list.
    await expect(phraseRows(page).first()).toHaveText("check this window");
    await expect(phraseInput(page)).toHaveValue("");
    await expect
      .poll(async () => (await lastSave(page))?.phrases)
      .toEqual(["check this window", ...DEFAULT_PHRASES]);

    await phraseInput(page).fill("what do you see");
    await add.click();
    await expect(phraseRows(page).first()).toHaveText("what do you see");
    await expect(phraseRows(page)).toHaveCount(DEFAULT_PHRASES.length + 2);

    // Already there, in other letters: the list stays as long.
    await phraseInput(page).fill("ON MY SCREEN");
    await phraseInput(page).press("Enter");
    await expect.poll(() => saves(page)).toBe(3);
    await expect(phraseRows(page)).toHaveCount(DEFAULT_PHRASES.length + 2);
    await expect(page.getByTestId("vision-phrases-restore")).toBeVisible();
  });

  test("a phrase is removed with the mouse from the button its row shows on hover", async ({
    page,
  }) => {
    await openScreen(page);
    const row = phraseRows(page).filter({ hasText: "this error" });
    const remove = row.getByRole("button", { name: "Remove this error" });
    await expect(remove).toBeHidden();
    await row.hover();
    await expect(remove).toBeVisible();
    await remove.click();

    await expect(phraseRows(page)).toHaveText(DEFAULT_PHRASES.filter((p) => p !== "this error"));
    await expect
      .poll(async () => (await lastSave(page))?.phrases)
      .toEqual(DEFAULT_PHRASES.filter((p) => p !== "this error"));
  });

  test("a phrase is removed with the keyboard alone, and focus stays in the list", async ({
    page,
  }) => {
    await openScreen(page);
    const first = phraseRows(page).first();
    const remove = first.getByRole("button", { name: "Remove look at my screen" });
    await expect(remove).toBeHidden();

    // Tab from the input: the Add button is disabled (nothing typed),
    // so focus lands on the first row, which shows its button.
    await phraseInput(page).focus();
    await page.keyboard.press("Tab");
    await expect(first).toBeFocused();
    await expect(remove).toBeVisible();
    await page.keyboard.press("Tab");
    await expect(remove).toBeFocused();
    await page.keyboard.press("Enter");

    await expect(phraseRows(page)).toHaveText(DEFAULT_PHRASES.slice(1));
    // The row that moved up has the focus: the next Tab reaches its
    // button, and Enter removes that one too.
    await expect(phraseRows(page).first()).toBeFocused();
    await page.keyboard.press("Tab");
    await expect(
      phraseRows(page).first().getByRole("button", { name: "Remove on my screen" }),
    ).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(phraseRows(page)).toHaveText(DEFAULT_PHRASES.slice(2));
    await expect.poll(async () => (await lastSave(page))?.phrases).toEqual(DEFAULT_PHRASES.slice(2));
  });

  test("Restore defaults brings the built-in phrases back", async ({ page }) => {
    await openScreen(page, { settings: { phrases: ["check this window"] } });
    await expect(phraseRows(page)).toHaveText(["check this window"]);
    const restore = page.getByTestId("vision-phrases-restore");
    await expect(restore).toHaveText("Restore defaults");
    await restore.click();

    await expect(phraseRows(page)).toHaveText(DEFAULT_PHRASES);
    // Stored as "the built-in list", not as a copy of it.
    await expect.poll(async () => (await lastSave(page))?.phrases).toBeNull();
    await expect(restore).toHaveCount(0);
  });

  test("a list with nothing left says what that means", async ({ page }) => {
    await openScreen(page, { settings: { phrases: ["check this window"] } });
    await phraseRows(page).first().hover();
    await page.getByRole("button", { name: "Remove check this window" }).click();
    await expect(phraseRows(page)).toHaveCount(0);
    await expect(page.getByTestId("vision-phrases-empty")).toHaveText(
      "No phrases. The screen is never included.",
    );
    // An emptied list is the person's and is stored as empty.
    await expect.poll(async () => (await lastSave(page))?.phrases).toEqual([]);

    await triggerSelect(page).click();
    await choose(page, "When needed");
    await expect(page.getByTestId("vision-phrases-empty")).toHaveText(
      "No phrases. The screen is only included when the model says it needs it.",
    );
  });
});

const RUNNING = [
  { id: "com.example.Notes", name: "Notes" },
  { id: "com.example.vault", name: "Vault" },
  { id: "com.example.ledger", name: "Ledger" },
];

test.describe("screen section: never in a screenshot", () => {
  test("lists the built-in apps with name and id", async ({ page }) => {
    await openScreen(page);
    await expect(appRows(page)).toHaveCount(2);
    await expect(appRows(page).nth(0)).toContainText("Vault");
    await expect(appRows(page).nth(0)).toContainText("com.example.vault");
    await expect(appRows(page).nth(1)).toContainText("Keyring");
    await expect(appRows(page).nth(1)).toContainText("com.example.keyring");
    await expect(page.getByTestId("vision-never-restore")).toHaveCount(0);
  });

  test("an app that is open now is added from the picker", async ({ page }) => {
    await openScreen(page, { running: RUNNING });
    await openApps(page).click();
    // Vault is on the list already and is not offered again.
    await expect(page.getByRole("option")).toHaveText(["Notes", "Ledger"]);
    await choose(page, "Ledger");

    await expect(appRows(page)).toHaveCount(3);
    await expect(appRows(page).first()).toContainText("Ledger");
    await expect(appRows(page).first()).toContainText("com.example.ledger");
    await expect
      .poll(async () => (await lastSave(page))?.never_capture)
      .toEqual([
        { id: "com.example.ledger", name: "Ledger" },
        { id: "com.example.vault", name: "Vault" },
        { id: "com.example.keyring", name: "Keyring" },
      ]);

    // The picker is ready for the next one, without Ledger.
    await expect(openApps(page)).toContainText("Add an open app");
    await openApps(page).click();
    await expect(page.getByRole("option")).toHaveText(["Notes"]);
    await choose(page, "Notes");
    await expect(appRows(page).first()).toContainText("com.example.notes");
    await expect(openApps(page)).toBeDisabled();
    await expect(openApps(page)).toContainText("No other open apps");
  });

  test("an app is added by typing its id, with Enter and with the button", async ({ page }) => {
    await openScreen(page, { running: RUNNING });
    const add = page.getByRole("button", { name: "Add app" });
    await expect(add).toBeDisabled();
    await expect(appIdInput(page)).toHaveAttribute("placeholder", "com.example.vault");

    await appIdInput(page).fill(" Com.Example.Safe ");
    await appIdInput(page).press("Enter");
    // Stored in lower case; without a name the id is what is shown.
    await expect(appRows(page).first()).toHaveText("com.example.safe");
    await expect(appIdInput(page)).toHaveValue("");
    await expect
      .poll(async () => (await lastSave(page))?.never_capture?.[0])
      .toEqual({ id: "com.example.safe", name: "" });

    // An id of an app that is open gets that app's name.
    await appIdInput(page).fill("com.example.ledger");
    await add.click();
    await expect(appRows(page).first()).toContainText("Ledger");
    await expect
      .poll(async () => (await lastSave(page))?.never_capture?.[0])
      .toEqual({ id: "com.example.ledger", name: "Ledger" });

    // An id that is on the list is not added twice.
    await appIdInput(page).fill("COM.EXAMPLE.VAULT");
    await appIdInput(page).press("Enter");
    await expect.poll(() => saves(page)).toBe(3);
    await expect(appRows(page)).toHaveCount(4);
  });

  test("an app is removed with the mouse and with the keyboard", async ({ page }) => {
    await openScreen(page);
    const vault = appRows(page).filter({ hasText: "Vault" });
    const removeVault = vault.getByRole("button", { name: "Remove Vault" });
    await expect(removeVault).toBeHidden();
    await vault.hover();
    await expect(removeVault).toBeVisible();
    await removeVault.click();
    await expect(appRows(page)).toHaveCount(1);
    await expect
      .poll(async () => (await lastSave(page))?.never_capture)
      .toEqual([{ id: "com.example.keyring", name: "Keyring" }]);

    // The keyboard: Tab from the id input lands on the row (the Add
    // button is disabled), the next Tab on its button.
    const keyring = appRows(page).first();
    await appIdInput(page).focus();
    await page.keyboard.press("Tab");
    await expect(keyring).toBeFocused();
    await page.keyboard.press("Tab");
    await expect(keyring.getByRole("button", { name: "Remove Keyring" })).toBeFocused();
    await page.keyboard.press("Enter");

    await expect(appRows(page)).toHaveCount(0);
    await expect(page.getByTestId("vision-never-empty")).toHaveText(
      "No apps. Every app can end up in the picture.",
    );
    await expect.poll(async () => (await lastSave(page))?.never_capture).toEqual([]);
  });

  test("Restore defaults brings the built-in apps back", async ({ page }) => {
    await openScreen(page, {
      settings: { never_capture: [{ id: "com.example.safe", name: "Safe" }] },
    });
    await expect(appRows(page)).toHaveCount(1);
    const restore = page.getByTestId("vision-never-restore");
    await expect(restore).toHaveText("Restore defaults");
    await restore.click();

    await expect(appRows(page)).toHaveCount(2);
    await expect(appRows(page).first()).toContainText("Vault");
    await expect.poll(async () => (await lastSave(page))?.never_capture).toBeNull();
    await expect(restore).toHaveCount(0);
  });
});

test.describe("screen section: the macOS permission", () => {
  test("allowed: says so, with no notice and nothing to ask for", async ({ page }) => {
    await openScreen(page, { permission: "granted" });
    const row = page.getByTestId("vision-permission");
    await expect(row).toHaveAttribute("data-state", "allowed");
    await expect(page.getByTestId("vision-permission-state")).toHaveText(
      "Allowed. macOS lets OpenWhisper record the screen for an ask.",
    );
    await expect(page.getByTestId("vision-permission-notice")).toHaveCount(0);
    await expect(row.getByRole("button", { name: "Ask for permission" })).toHaveCount(0);
    await expect(row.getByRole("button", { name: "Open System Settings" })).toBeVisible();
  });

  test("not allowed yet: a notice on top, the state, both buttons and the restart sentence", async ({
    page,
  }) => {
    await openScreen(page, { permission: "denied" });
    const notice = page.getByTestId("vision-permission-notice");
    await expect(notice).toBeVisible();
    await expect(notice).toContainText("The screen cannot be included yet");
    await expect(notice).toContainText(
      "macOS has not allowed OpenWhisper to record the screen. Until it does, an ask goes out without the picture and the answer says so.",
    );
    // On top of the section: above the choice of when to include it.
    const noticeBox = await notice.boundingBox();
    const selectBox = await triggerSelect(page).boundingBox();
    expect(noticeBox && selectBox && noticeBox.y < selectBox.y).toBe(true);

    const row = page.getByTestId("vision-permission");
    await expect(row).toHaveAttribute("data-state", "not-allowed");
    await expect(page.getByTestId("vision-permission-state")).toHaveText(
      "Not allowed yet. macOS has to allow OpenWhisper to record the screen before an ask can include it.",
    );
    await expect(page.getByTestId("vision-permission-restart")).toHaveText(
      "After you allow it in System Settings, quit OpenWhisper and open it again. The permission only counts after a restart.",
    );
    await expect(row.getByRole("button", { name: "Ask for permission" })).toBeEnabled();
    await expect(row.getByRole("button", { name: "Open System Settings" })).toBeEnabled();
    // Showing the row asked the system for nothing.
    expect(await calls(page, "vision_request_permission")).toBe(0);
    expect(await calls(page, "vision_open_settings")).toBe(0);
  });

  test("a state the system could not tell counts as not allowed", async ({ page }) => {
    await openScreen(page, { permission: "unknown" });
    await expect(page.getByTestId("vision-permission")).toHaveAttribute(
      "data-state",
      "not-allowed",
    );
    await expect(page.getByTestId("vision-permission-notice")).toBeVisible();
  });

  test("Ask for permission requests it and shows the answer", async ({ page }) => {
    await openScreen(page, { permission: "denied", permissionAfterRequest: "granted" });
    const row = page.getByTestId("vision-permission");
    await row.getByRole("button", { name: "Ask for permission" }).click();

    await expect.poll(() => calls(page, "vision_request_permission")).toBe(1);
    await expect(row).toHaveAttribute("data-state", "allowed");
    await expect(row.getByRole("button", { name: "Ask for permission" })).toHaveCount(0);
    await expect(page.getByTestId("vision-permission-notice")).toHaveCount(0);
    // Asking opened no settings pane and saved nothing.
    expect(await calls(page, "vision_open_settings")).toBe(0);
    expect(await saves(page)).toBe(0);
  });

  test("a request that is not granted leaves the row as it was", async ({ page }) => {
    await openScreen(page, { permission: "denied", permissionAfterRequest: "denied" });
    const row = page.getByTestId("vision-permission");
    await row.getByRole("button", { name: "Ask for permission" }).click();
    await expect.poll(() => calls(page, "vision_request_permission")).toBe(1);
    await expect(row).toHaveAttribute("data-state", "not-allowed");
    await expect(row.getByRole("button", { name: "Ask for permission" })).toBeEnabled();
    await expect(page.getByTestId("vision-permission-restart")).toBeVisible();
  });

  test("Open System Settings opens the pane and requests nothing", async ({ page }) => {
    await openScreen(page, { permission: "denied" });
    await page
      .getByTestId("vision-permission")
      .getByRole("button", { name: "Open System Settings" })
      .click();
    await expect.poll(() => calls(page, "vision_open_settings")).toBe(1);
    expect(await calls(page, "vision_request_permission")).toBe(0);
  });

  test("with the screen switched off there is no notice, only the row", async ({ page }) => {
    await openScreen(page, { permission: "denied", settings: { trigger: "off" } });
    await expect(page.getByTestId("vision-permission-notice")).toHaveCount(0);
    await expect(page.getByTestId("vision-permission")).toHaveAttribute(
      "data-state",
      "not-allowed",
    );

    // Choosing to include the screen brings the notice.
    await triggerSelect(page).click();
    await choose(page, "Always");
    await expect(page.getByTestId("vision-permission-notice")).toBeVisible();
  });
});

test.describe("screen section: Windows", () => {
  test("has no permission row or notice, and says what happens there", async ({ page }) => {
    // The shim's macOS permission is ignored on Windows, as in core.
    await openScreen(page, { platform: "windows", permission: "denied" });
    await expect(page.getByTestId("vision-permission")).toHaveCount(0);
    await expect(page.getByTestId("vision-permission-notice")).toHaveCount(0);
    await expect(section(page)).not.toContainText("System Settings");

    await expect(page.getByTestId("vision-never-note")).toHaveText(
      "The windows of the apps on this list are painted over before the picture is sent anywhere, so they are not in it. When one of them is the app in front, no picture is taken at all and the answer says so.",
    );
    await expect(appRows(page).first()).toContainText("vault.exe");
    await expect(appIdInput(page)).toHaveAttribute("placeholder", "vault.exe");
    // What a list cannot cover holds there too.
    await expect(page.getByTestId("vision-cannot-cover")).toBeVisible();
  });
});

test.describe("screen section: while Ask is off", () => {
  test("is disabled, says why, and comes alive when Ask is switched on", async ({ page }) => {
    await openScreen(page, {
      askEnabled: false,
      permission: "denied",
      running: RUNNING,
    });
    await expect(section(page)).toHaveAttribute("data-disabled", "true");
    await expect(page.getByTestId("vision-ask-off")).toHaveText(
      "Ask is off, so no picture is ever taken and these settings are not in use. Switch Ask on at the top of this page to change them.",
    );
    // What it is about can still be read.
    await expect(page.getByTestId("vision-intro")).toBeVisible();
    await expect(page.getByTestId("vision-cannot-cover")).toBeVisible();
    // No warning about a permission nothing needs right now.
    await expect(page.getByTestId("vision-permission-notice")).toHaveCount(0);

    await expect(triggerSelect(page)).toBeDisabled();
    await expect(phraseInput(page)).toBeDisabled();
    await expect(page.getByRole("button", { name: "Add phrase" })).toBeDisabled();
    await expect(appIdInput(page)).toBeDisabled();
    await expect(page.getByRole("button", { name: "Add app" })).toBeDisabled();
    await expect(openApps(page)).toBeDisabled();
    const permission = page.getByTestId("vision-permission");
    await expect(permission.getByRole("button", { name: "Ask for permission" })).toBeDisabled();
    await expect(permission.getByRole("button", { name: "Open System Settings" })).toBeDisabled();
    await phraseRows(page).first().hover();
    await expect(
      phraseRows(page).first().getByRole("button", { name: "Remove look at my screen" }),
    ).toBeDisabled();
    // The open apps were not even asked for.
    expect(await calls(page, "vision_running_apps")).toBe(0);

    await page.getByRole("switch", { name: "Ask", exact: true }).click();
    await expect(section(page)).not.toHaveAttribute("data-disabled", "true");
    await expect(page.getByTestId("vision-ask-off")).toHaveCount(0);
    await expect(triggerSelect(page)).toBeEnabled();
    await expect(phraseInput(page)).toBeEnabled();
    await expect(page.getByTestId("vision-permission-notice")).toBeVisible();
    expect(await saves(page)).toBe(0);
  });
});

test.describe("screen section: changes made elsewhere", () => {
  test("vision_changed from outside updates the section", async ({ page }) => {
    await openScreen(page);
    await waitForVisionListener(page);
    await emitVisionChanged(page, {
      trigger: "always",
      never_capture: [{ id: "com.example.safe", name: "Safe" }],
    });
    await expect(triggerSelect(page)).toContainText("Always");
    await expect(page.getByTestId("vision-phrases")).toHaveCount(0);
    await expect(appRows(page)).toHaveCount(1);
    await expect(appRows(page).first()).toContainText("Safe");
    await expect(page.getByTestId("vision-never-restore")).toBeVisible();

    await emitVisionChanged(page, { trigger: "smart", phrases: ["check this window"] });
    await expect(triggerSelect(page)).toContainText("When needed");
    await expect(phraseRows(page)).toHaveText(["check this window"]);
    await expect(appRows(page)).toHaveCount(2);
    // Showing a change from outside saves nothing back.
    expect(await saves(page)).toBe(0);
  });

  test("a change made while another pane is open is there when Ask opens", async ({ page }) => {
    await openScreen(page);
    await page.getByRole("tab", { name: "General" }).click();
    await emitVisionChanged(page, { trigger: "off" });
    await page.getByRole("tab", { name: "Ask", exact: true }).click();
    await expect(triggerSelect(page)).toContainText("Off");
  });
});

// --- the answer window ------------------------------------------------

answerTest.describe("answer window: the screen", () => {
  answerTest.use({ viewport: { width: 420, height: 360 } });

  async function open(page: Page) {
    await page.goto("/");
    await waitForAskTickListener(page);
  }
  const QUESTION = "what does this error on my screen mean";

  answerTest("says so while the screen is being looked at", async ({ page }) => {
    await open(page);
    await emitAskTick(page, { phase: "thinking", question: QUESTION, screen: "capturing" });
    const status = page.getByTestId("answer-status");
    await expect(status).toHaveText("Looking at your screen");
    await expect(status.locator("svg")).toBeVisible();
    await expect(page.getByTestId("answer-screen-included")).toHaveCount(0);
    await expect(page.getByTestId("answer-screen-notice")).toHaveCount(0);

    // The capture is over: back to the usual line.
    await emitAskTick(page, { phase: "thinking", question: QUESTION, screen: "included" });
    await expect(status).toHaveText("Thinking");
  });

  answerTest("marks an answer whose question went out with the screen", async ({ page }) => {
    await open(page);
    await emitAskTick(page, {
      phase: "answering",
      question: QUESTION,
      answer: "The build",
      screen: "included",
    });
    const marker = page.getByTestId("answer-screen-included");
    await expect(marker).toHaveText("Screen included");

    await emitAskTick(page, {
      phase: "done",
      question: QUESTION,
      answer: "The build failed because a file is missing.",
      screen: "included",
    });
    await expect(marker).toBeVisible();
    await expect(page.getByTestId("answer-source")).toHaveText("Sent to Claude (Sonnet)");
    await expect(page.getByTestId("answer-screen-notice")).toHaveCount(0);
    // Inside the card, with the footer, and no picture anywhere.
    await expect(page.getByTestId("answer-card").locator("img, canvas")).toHaveCount(0);
  });

  answerTest("an ask without the screen has no marker and no notice", async ({ page }) => {
    await open(page);
    await emitAskTick(page, { phase: "done", question: "what is a tap", answer: "A short press." });
    await expect(page.getByTestId("answer-source")).toBeVisible();
    await expect(page.getByTestId("answer-screen-included")).toHaveCount(0);
    await expect(page.getByTestId("answer-screen-notice")).toHaveCount(0);
  });

  answerTest("says why there is no screenshot when an app that is never captured is in front", async ({
    page,
  }) => {
    await open(page);
    const why = "No screenshot was taken: Vault is in front, and it is on your never-capture list.";
    await emitAskTick(page, {
      phase: "done",
      question: QUESTION,
      answer: "I could not see your screen, so I cannot say what the error is.",
      screen: "blocked",
      screen_notice: why,
    });
    const notice = page.getByTestId("answer-screen-notice");
    await expect(notice).toHaveText(why);
    await expect(notice).toHaveAttribute("data-screen", "blocked");
    await expect(page.getByTestId("answer-screen-included")).toHaveCount(0);
    await expect(page.getByTestId("answer-text")).toContainText("I could not see your screen");
  });

  answerTest("says why when the screen could not be captured", async ({ page }) => {
    await open(page);
    const why =
      "No screenshot was taken: OpenWhisper is not allowed to record the screen. Allow it in the Ask settings.";
    await emitAskTick(page, {
      phase: "answering",
      question: QUESTION,
      answer: "I cannot",
      screen: "unavailable",
      screen_notice: why,
    });
    const notice = page.getByTestId("answer-screen-notice");
    await expect(notice).toHaveText(why);
    await expect(notice).toHaveAttribute("data-screen", "unavailable");
    await expect(page.getByTestId("answer-screen-included")).toHaveCount(0);
  });
});

// --- the pill -----------------------------------------------------------

pillTest.describe("pill: the screen label", () => {
  const opacity = (page: Page) =>
    page.getByTestId("pill-mode-label").evaluate((el) => Number(getComputedStyle(el).opacity));

  async function mount(page: Page) {
    // The size of the real pill window.
    await page.setViewportSize({ width: 180, height: 110 });
    await page.goto("/");
    await page.locator(".pill-capsule").waitFor({ state: "visible" });
    await waitForAskTickListener(page);
  }

  pillTest("says Screen while an ask captures, and for about a second after", async ({ page }) => {
    await mount(page);
    const label = page.getByTestId("pill-mode-label");
    await expect(label).toHaveAttribute("data-visible", "false");

    await emitAskTick(page, { phase: "thinking", screen: "capturing" });
    await expect(label).toHaveText("Screen");
    await expect(label).toHaveAttribute("data-visible", "true");
    await expect(label).toHaveAttribute("data-kind", "screen");
    await expect.poll(() => opacity(page)).toBe(1);

    // For as long as the capture runs, however long that is (the
    // ticks repeat while the ask is busy).
    await page.waitForTimeout(1300);
    await emitAskTick(page, { phase: "thinking", screen: "capturing" });
    await expect(label).toHaveAttribute("data-visible", "true");

    // The screenshot went out: about a second more, then gone.
    await emitAskTick(page, { phase: "thinking", screen: "included" });
    await page.waitForTimeout(500);
    await expect(label).toHaveAttribute("data-visible", "true");
    await expect(label).toHaveAttribute("data-visible", "false", { timeout: 2500 });
    await expect.poll(() => opacity(page)).toBe(0);

    // Later ticks of the same ask do not bring it back.
    await emitAskTick(page, { phase: "answering", answer: "The build", screen: "included" });
    await emitAskTick(page, { phase: "done", answer: "The build failed.", screen: "included" });
    await page.waitForTimeout(300);
    await expect(label).toHaveAttribute("data-visible", "false");
  });

  pillTest("shows a capture that was over before a tick could show it", async ({ page }) => {
    await mount(page);
    const label = page.getByTestId("pill-mode-label");
    await emitAskTick(page, { phase: "thinking", screen: "included" });
    await expect(label).toHaveText("Screen");
    await expect(label).toHaveAttribute("data-visible", "true");
    await expect(label).toHaveAttribute("data-visible", "false", { timeout: 2500 });
  });

  pillTest("goes away at once when no screenshot is taken after all", async ({ page }) => {
    await mount(page);
    const label = page.getByTestId("pill-mode-label");
    await emitAskTick(page, { phase: "thinking", screen: "capturing" });
    await expect(label).toHaveAttribute("data-visible", "true");
    await emitAskTick(page, {
      phase: "thinking",
      screen: "blocked",
      screen_notice: "No screenshot was taken: Vault is in front.",
    });
    await expect(label).toHaveAttribute("data-visible", "false");
  });

  pillTest("an ask that does not involve the screen shows nothing", async ({ page }) => {
    await mount(page);
    await emitAskTick(page, { phase: "listening" });
    await emitAskTick(page, { phase: "thinking" });
    await emitAskTick(page, { phase: "done", answer: "A short press." });
    await page.waitForTimeout(300);
    const label = page.getByTestId("pill-mode-label");
    await expect(label).toHaveAttribute("data-visible", "false");
    await expect(label).toHaveText("");
  });

  pillTest("sits above the capsule, changes nothing about it and takes no clicks", async ({
    page,
  }) => {
    await mount(page);
    await emitAskTick(page, { phase: "thinking", screen: "capturing" });
    const label = page.getByTestId("pill-mode-label");
    await expect(label).toHaveAttribute("data-visible", "true");

    const capsule = page.locator(".pill-capsule");
    const box = await label.boundingBox();
    const pill = await capsule.boundingBox();
    expect(box).not.toBeNull();
    expect(pill).not.toBeNull();
    if (!box || !pill) return;
    expect(box.y + box.height).toBeLessThanOrEqual(pill.y - 4);
    expect(box.y).toBeGreaterThanOrEqual(0);
    // The idle capsule is the size it always is.
    expect(Math.round(pill.width)).toBe(38);
    expect(Math.round(pill.height)).toBe(22);
    expect(await label.evaluate((el) => getComputedStyle(el).pointerEvents)).toBe("none");
  });
});

// --- the history ----------------------------------------------------------

test.describe("history: asks that included the screen", () => {
  async function seedHistory(page: Page) {
    await page.addInitScript(() => {
      const w = window as unknown as {
        __owHistorySettings?: unknown;
        __owHistoryEntries?: unknown[];
      };
      w.__owHistorySettings = { enabled: true };
      const noon = new Date();
      noon.setHours(12, 0, 0, 0);
      const at = (hoursAgo: number) => noon.getTime() - hoursAgo * 3_600_000;
      const base = {
        duration_ms: 4000,
        word_count: 5,
        original: null,
        app_id: null,
        app_name: null,
      };
      w.__owHistoryEntries = [
        {
          ...base,
          id: "a3",
          at_ms: at(1),
          text: "what does this error mean",
          kind: "ask",
          answer: "A file is missing.",
          screen: true,
        },
        {
          ...base,
          id: "a2",
          at_ms: at(2),
          text: "what is a tap",
          kind: "ask",
          answer: "A short press of a key.",
        },
        { ...base, id: "e1", at_ms: at(3), text: "Remember to book the meeting room." },
      ];
    });
  }

  test("Home marks the ask that was sent with the screen, next to the Ask marker", async ({
    page,
  }) => {
    await seedHistory(page);
    await page.goto("/");
    const rows = page.getByTestId("history-row");
    await expect(rows).toHaveCount(3);

    const withScreen = rows.nth(0);
    await expect(withScreen.getByTestId("history-row-ask")).toHaveText("Ask");
    await expect(withScreen.getByTestId("history-row-screen")).toHaveText("Screen");
    // Right after the Ask marker, on the same line.
    const ask = await withScreen.getByTestId("history-row-ask").boundingBox();
    const screen = await withScreen.getByTestId("history-row-screen").boundingBox();
    expect(ask && screen && screen.x > ask.x && Math.abs(screen.y - ask.y) < 2).toBe(true);

    // An ask without the screen and a dictation have none.
    await expect(rows.nth(1).getByTestId("history-row-ask")).toHaveText("Ask");
    await expect(rows.nth(1).getByTestId("history-row-screen")).toHaveCount(0);
    await expect(rows.nth(2).getByTestId("history-row-screen")).toHaveCount(0);
  });

  test("the History screen marks it too", async ({ page }) => {
    await seedHistory(page);
    await page.goto("/");
    await page.getByTestId("sidebar-item-history").click();
    await expect(page.getByTestId("history-pane")).toBeVisible();
    const rows = page.getByTestId("history-row");
    await expect(rows).toHaveCount(3);
    await expect(page.getByTestId("history-row-screen")).toHaveCount(1);
    await expect(rows.nth(0).getByTestId("history-row-screen")).toHaveText("Screen");
  });
});
