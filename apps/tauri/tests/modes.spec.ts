import {
  emitModesChanged,
  emitTick,
  expect,
  pillTest,
  test,
  waitForModesListener,
  waitForTickListener,
  type MockModesChanged,
} from "./fixtures/tauri-shim";

// Modes (TASK-96): the Modes settings pane, the Mode card on Home, the
// mode marker on dictations and the mode name on the pill. The shim
// keeps an in-memory modes block that behaves like core, so these
// tests cover the screens and what they send, not core's rules.

type Page = import("@playwright/test").Page;

interface Mode {
  id: string;
  name: string;
  instructions: string;
  model: string | null;
  effort: string | null;
  // The Refine backend the model and effort were picked for.
  backend?: "claude-cli" | "openai-compatible" | null;
}

interface Seed {
  refine?: Record<string, unknown>;
  modes?: {
    active?: string;
    custom?: Mode[];
    app_defaults?: { app_id: string; app_name: string; mode: string }[];
  };
  history?: { enabled?: boolean };
  // The apps that are open (what `focus::running_apps` reports).
  running?: { id: string; name: string }[];
  // Stored dictations, newest first. Built in the page so "today"
  // follows its clock.
  entries?: {
    id: string;
    hoursAgo: number;
    text: string;
    app_id: string | null;
    app_name: string | null;
    mode?: string | null;
  }[];
}

async function seed(page: Page, data: Seed) {
  await page.addInitScript((d: Seed) => {
    const w = window as unknown as Record<string, unknown>;
    if (d.refine) w.__owRefineSettings = d.refine;
    if (d.modes) w.__owModesSettings = d.modes;
    if (d.history) w.__owHistorySettings = d.history;
    if (d.running) w.__owRunningApps = d.running;
    const noon = new Date();
    noon.setHours(12, 0, 0, 0);
    w.__owHistoryEntries = (d.entries ?? []).map(({ hoursAgo, ...e }) => ({
      original: null,
      mode: null,
      ...e,
      at_ms: noon.getTime() - hoursAgo * 3_600_000,
      duration_ms: 4000,
      word_count: e.text.split(/\s+/).length,
    }));
  }, data);
}

const FORMAL: Mode = {
  id: "formal",
  name: "Formal",
  instructions: "Rewrite what I say in a formal tone.",
  model: null,
  effort: null,
};
const ENGLISH: Mode = {
  id: "write-in-english",
  name: "Write in English",
  instructions: "Write what I say in English, whatever language I speak.",
  model: null,
  effort: null,
};

// Two dictations, into two made-up apps.
const ENTRIES = [
  { id: "e2", hoursAgo: 1, text: "The agenda is ready for Thursday.", app_id: "com.example.chat", app_name: "Chatter" },
  { id: "e1", hoursAgo: 2, text: "Please find the notes attached.", app_id: "com.example.mail", app_name: "Mailbox" },
];

// Calls the shim recorded, oldest first.
async function calls(page: Page): Promise<{ cmd: string; args: Record<string, unknown> }[]> {
  return page.evaluate(
    () =>
      (window as unknown as { __owModesCalls?: { cmd: string; args: Record<string, unknown> }[] })
        .__owModesCalls ?? [],
  );
}

async function openModes(page: Page) {
  await page.waitForSelector('[data-testid="sidebar-item-home"]');
  await page.evaluate(() => window.__owEmit("ow_navigate", "settings"));
  await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
  await page.getByRole("tab", { name: "Modes" }).click();
  await expect(page.getByRole("heading", { name: "Modes", exact: true })).toBeVisible();
}

function row(page: Page, id: string) {
  return page.locator(`[data-testid="mode-row"][data-mode="${id}"]`);
}

async function pick(page: Page, combobox: string, option: string) {
  await page.getByRole("combobox", { name: combobox }).click();
  await page.getByRole("option", { name: option, exact: true }).click();
}

test.describe("modes pane: the list", () => {
  test("says what a mode is and that modes need Refine", async ({ page }) => {
    await page.goto("/");
    await openModes(page);
    await expect(page.getByText("A mode is a set of instructions Refine follows")).toBeVisible();
    await expect(page.getByText("Modes only apply while Refine is on.")).toBeVisible();

    // Refine is off by default: the pane says so and leads to the switch.
    const off = page.getByTestId("modes-refine-off");
    await expect(off).toContainText("Refine is off");
    await off.getByRole("button", { name: "Open Refine settings" }).click();
    await expect(page.getByRole("switch", { name: "Refine dictations" })).toBeVisible();
  });

  test("with Refine on there is no warning", async ({ page }) => {
    await seed(page, { refine: { enabled: true } });
    await page.goto("/");
    await openModes(page);
    await expect(row(page, "dictation")).toBeVisible();
    await expect(page.getByTestId("modes-refine-off")).toHaveCount(0);
  });

  test("Dictation is built in: no delete, and its edit opens the Refine pane", async ({ page }) => {
    await seed(page, { refine: { instructions: "Use British spelling." } });
    await page.goto("/");
    await openModes(page);

    const dictation = row(page, "dictation");
    await expect(dictation).toContainText("Dictation");
    await expect(dictation).toContainText("Built in");
    await expect(dictation.getByTestId("mode-row-selected")).toBeVisible();
    // Its instructions are the Refine instructions.
    await expect(dictation.getByTestId("mode-row-excerpt")).toHaveText("Use British spelling.");

    await dictation.hover();
    await expect(dictation.getByRole("button", { name: /^Delete/ })).toHaveCount(0);
    await dictation.getByRole("button", { name: "Edit the Dictation instructions in Refine" }).click();
    await expect(page.getByRole("heading", { name: "Refine", exact: true })).toBeVisible();
    await expect(page.getByRole("textbox", { name: "What Refine should always do" })).toHaveValue(
      "Use British spelling.",
    );
  });

  test("Dictation without instructions of the user's own shows the standard ones", async ({ page }) => {
    await page.goto("/");
    await openModes(page);
    await expect(row(page, "dictation").getByTestId("mode-row-excerpt")).toContainText(
      "Fix punctuation and capitalization.",
    );
  });

  test("a new mode needs a name before it can be saved", async ({ page }) => {
    await page.goto("/");
    await openModes(page);
    await page.getByTestId("mode-new").click();

    const dialog = page.getByRole("dialog", { name: "New mode" });
    await expect(dialog).toBeVisible();
    const save = dialog.getByRole("button", { name: "Save" });
    await expect(save).toBeDisabled();

    // Spaces are not a name.
    await dialog.getByRole("textbox", { name: "Name" }).fill("   ");
    await expect(save).toBeDisabled();
    await dialog.getByRole("textbox", { name: "Instructions" }).fill("Rewrite it.");
    await expect(save).toBeDisabled();

    await dialog.getByRole("textbox", { name: "Name" }).fill("Formal");
    await expect(save).toBeEnabled();

    // Cancel saves nothing.
    await dialog.getByRole("button", { name: "Cancel" }).click();
    await expect(dialog).toHaveCount(0);
    await expect(page.getByTestId("mode-row")).toHaveCount(1);
    expect(await calls(page)).toHaveLength(0);
  });

  test("a name another mode has is refused, in the words Rust uses, and nothing is saved", async ({ page }) => {
    await seed(page, { modes: { custom: [FORMAL, ENGLISH] } });
    await page.goto("/");
    await openModes(page);

    // A new mode with a taken name, in another case and with spaces.
    await page.getByTestId("mode-new").click();
    const dialog = page.getByRole("dialog", { name: "New mode" });
    await dialog.getByRole("textbox", { name: "Name" }).fill("  FORMAL ");
    await dialog.getByRole("button", { name: "Save" }).click();
    await expect(dialog.getByRole("alert")).toHaveText(
      'There is already a mode called "Formal". Give this one another name.',
    );
    // The dialog stays open with what was typed, and nothing was added.
    await expect(dialog.getByRole("textbox", { name: "Name" })).toHaveValue("  FORMAL ");
    await expect(page.getByTestId("mode-row")).toHaveCount(3);

    // The built-in mode's name is taken too.
    await dialog.getByRole("textbox", { name: "Name" }).fill("dictation");
    await dialog.getByRole("button", { name: "Save" }).click();
    await expect(dialog.getByRole("alert")).toHaveText(
      'There is already a mode called "Dictation". Give this one another name.',
    );

    // Another name goes through, and the error is gone with the dialog.
    await dialog.getByRole("textbox", { name: "Name" }).fill("Casual");
    await dialog.getByRole("button", { name: "Save" }).click();
    await expect(dialog).toHaveCount(0);
    await expect(row(page, "casual")).toBeVisible();

    // Renaming onto another mode's name is refused the same way.
    const english = row(page, "write-in-english");
    await english.hover();
    await english.getByRole("button", { name: "Edit Write in English" }).click();
    const edit = page.getByRole("dialog", { name: "Edit mode" });
    await edit.getByRole("textbox", { name: "Name" }).fill("formal");
    await edit.getByRole("button", { name: "Save" }).click();
    await expect(edit.getByRole("alert")).toContainText('already a mode called "Formal"');
    // Its own name in another case is fine.
    await edit.getByRole("textbox", { name: "Name" }).fill("WRITE IN ENGLISH");
    await edit.getByRole("button", { name: "Save" }).click();
    await expect(edit).toHaveCount(0);
    await expect(english).toContainText("WRITE IN ENGLISH");

    // Only the two that went through were saved.
    expect((await calls(page)).map((c) => c.cmd)).toEqual(["modes_add", "modes_edit"]);
  });

  test("creates a mode; it follows the Refine model and effort unless told otherwise", async ({ page }) => {
    await page.goto("/");
    await openModes(page);
    await page.getByTestId("mode-new").click();

    const dialog = page.getByRole("dialog", { name: "New mode" });
    await expect(dialog.getByRole("combobox", { name: "Model" })).toContainText("Same as Refine");
    await expect(dialog.getByRole("combobox", { name: "Effort" })).toContainText("Same as Refine");
    await dialog.getByRole("textbox", { name: "Name" }).fill("Formal");
    await dialog
      .getByRole("textbox", { name: "Instructions" })
      .fill("Rewrite what I say in a formal tone.");
    await dialog.getByRole("button", { name: "Save" }).click();
    await expect(dialog).toHaveCount(0);

    // Core makes the id from the name.
    const formal = row(page, "formal");
    await expect(formal).toContainText("Formal");
    await expect(formal.getByTestId("mode-row-excerpt")).toHaveText(
      "Rewrite what I say in a formal tone.",
    );
    await expect(formal).not.toContainText("Built in");
    await expect(formal.getByTestId("mode-row-own")).toHaveCount(0);
    // Making a mode does not select it.
    await expect(row(page, "dictation").getByTestId("mode-row-selected")).toBeVisible();

    expect(await calls(page)).toEqual([
      {
        cmd: "modes_add",
        args: {
          name: "Formal",
          instructions: "Rewrite what I say in a formal tone.",
          model: null,
          effort: null,
        },
      },
    ]);
  });

  test("a mode can have a model and effort of its own, from the Refine choices", async ({ page }) => {
    await page.goto("/");
    await openModes(page);
    await page.getByTestId("mode-new").click();
    const dialog = page.getByRole("dialog", { name: "New mode" });
    await dialog.getByRole("textbox", { name: "Name" }).fill("Quick notes");

    // The same models the Refine pane offers, after "Same as Refine".
    await dialog.getByRole("combobox", { name: "Model" }).click();
    const options = page.getByRole("option");
    await expect(options).toHaveText(["Same as Refine", "Fable", "Opus", "Sonnet", "Haiku", "Other…"]);
    await page.getByRole("option", { name: "Haiku", exact: true }).click();

    await dialog.getByRole("combobox", { name: "Effort" }).click();
    await expect(page.getByRole("option")).toHaveText([
      "Same as Refine",
      "Low",
      "Medium",
      "High",
      "Extra high",
      "Max",
    ]);
    await page.getByRole("option", { name: "High", exact: true }).click();
    await dialog.getByRole("button", { name: "Save" }).click();

    const notes = row(page, "quick-notes");
    await expect(notes.getByTestId("mode-row-own")).toHaveText("Model: Haiku · Effort: High");
    // Without instructions it cleans up like the built-in mode.
    await expect(notes.getByTestId("mode-row-excerpt")).toHaveText("Cleans up like Dictation.");
    const added = (await calls(page)).at(-1);
    expect(added?.args.model).toBe("haiku");
    expect(added?.args.effort).toBe("high");
  });

  test("any other model name can be typed", async ({ page }) => {
    await page.goto("/");
    await openModes(page);
    await page.getByTestId("mode-new").click();
    const dialog = page.getByRole("dialog", { name: "New mode" });
    await dialog.getByRole("textbox", { name: "Name" }).fill("Careful");
    await expect(dialog.getByRole("textbox", { name: "Model name" })).toHaveCount(0);
    await pick(page, "Model", "Other…");
    await dialog.getByRole("textbox", { name: "Model name" }).fill("example-model-7");
    await dialog.getByRole("button", { name: "Save" }).click();

    await expect(row(page, "careful").getByTestId("mode-row-own")).toHaveText(
      "Model: example-model-7",
    );
    expect((await calls(page)).at(-1)?.args.model).toBe("example-model-7");
  });

  test("with an OpenAI-compatible server there are no named models, only a name to type", async ({ page }) => {
    await seed(page, { refine: { backend: "openai-compatible" } });
    await page.goto("/");
    await openModes(page);
    await page.getByTestId("mode-new").click();
    const dialog = page.getByRole("dialog", { name: "New mode" });
    await dialog.getByRole("combobox", { name: "Model" }).click();
    await expect(page.getByRole("option")).toHaveText(["Same as Refine", "Other…"]);
    await page.keyboard.press("Escape");
    await dialog.getByRole("combobox", { name: "Effort" }).click();
    await expect(page.getByRole("option")).toHaveText([
      "Same as Refine",
      "Minimal",
      "Low",
      "Medium",
      "High",
      "Extra high",
    ]);
  });

  test("a model picked for the other Refine backend is shown as not in use, and survives a rename", async ({ page }) => {
    // Made with a Claude model; Refine has since been switched to an
    // OpenAI-compatible server, which does not know that model.
    await seed(page, {
      refine: { backend: "openai-compatible" },
      modes: {
        custom: [{ ...FORMAL, model: "opus", effort: "high", backend: "claude-cli" }],
      },
    });
    await page.goto("/");
    await openModes(page);
    const formal = row(page, "formal");
    await expect(formal.getByTestId("mode-row-own")).toHaveText(
      "Model and effort: Same as Refine. Its own are for the Claude backend.",
    );

    await formal.hover();
    await formal.getByRole("button", { name: "Edit Formal" }).click();
    const dialog = page.getByRole("dialog", { name: "Edit mode" });
    await expect(dialog.getByRole("combobox", { name: "Model" })).toContainText("Same as Refine");
    await expect(dialog.getByRole("combobox", { name: "Effort" })).toContainText("Same as Refine");
    await expect(dialog.getByTestId("mode-other-backend")).toContainText(
      "a model and effort of its own for the Claude backend",
    );
    await expect(dialog.getByTestId("mode-other-backend")).toContainText(
      "Refine uses the OpenAI-compatible backend now",
    );

    // A rename, with nothing picked: the Claude model stays stored.
    await dialog.getByRole("textbox", { name: "Name" }).fill("Business tone");
    await dialog.getByRole("button", { name: "Save" }).click();
    await expect(dialog).toHaveCount(0);
    await expect(formal).toContainText("Business tone");
    await expect(formal.getByTestId("mode-row-own")).toContainText("Same as Refine");
    const stored = await page.evaluate(
      () =>
        (window as unknown as { __owModesStored: { custom: Mode[] } }).__owModesStored.custom[0],
    );
    expect(stored).toMatchObject({ model: "opus", effort: "high", backend: "claude-cli" });

    // Picking an effort here replaces them with one for this backend.
    await formal.hover();
    await formal.getByRole("button", { name: "Edit Business tone" }).click();
    await pick(page, "Effort", "Minimal");
    await page.getByRole("dialog", { name: "Edit mode" }).getByRole("button", { name: "Save" }).click();
    await expect(formal.getByTestId("mode-row-own")).toHaveText("Effort: Minimal");
    const replaced = await page.evaluate(
      () =>
        (window as unknown as { __owModesStored: { custom: Mode[] } }).__owModesStored.custom[0],
    );
    expect(replaced).toMatchObject({ model: null, effort: "minimal", backend: "openai-compatible" });
  });

  test("a model of the backend in use is shown as the mode's own", async ({ page }) => {
    await seed(page, {
      modes: { custom: [{ ...FORMAL, model: "opus", effort: "high", backend: "claude-cli" }] },
    });
    await page.goto("/");
    await openModes(page);
    const formal = row(page, "formal");
    await expect(formal.getByTestId("mode-row-own")).toHaveText("Model: Opus · Effort: High");
    await formal.hover();
    await formal.getByRole("button", { name: "Edit Formal" }).click();
    const dialog = page.getByRole("dialog", { name: "Edit mode" });
    await expect(dialog.getByRole("combobox", { name: "Model" })).toContainText("Opus");
    await expect(dialog.getByRole("combobox", { name: "Effort" })).toContainText("High");
    await expect(dialog.getByTestId("mode-other-backend")).toHaveCount(0);
  });

  test("edits and renames a mode; its id stays", async ({ page }) => {
    await seed(page, { modes: { custom: [{ ...FORMAL, model: "opus", effort: "medium" }] } });
    await page.goto("/");
    await openModes(page);

    const formal = row(page, "formal");
    await formal.hover();
    await formal.getByRole("button", { name: "Edit Formal" }).click();

    // The dialog opens with the mode as it is stored.
    const dialog = page.getByRole("dialog", { name: "Edit mode" });
    await expect(dialog.getByRole("textbox", { name: "Name" })).toHaveValue("Formal");
    await expect(dialog.getByRole("textbox", { name: "Instructions" })).toHaveValue(
      "Rewrite what I say in a formal tone.",
    );
    await expect(dialog.getByRole("combobox", { name: "Model" })).toContainText("Opus");
    await expect(dialog.getByRole("combobox", { name: "Effort" })).toContainText("Medium");

    await dialog.getByRole("textbox", { name: "Name" }).fill("Very formal");
    await dialog
      .getByRole("textbox", { name: "Instructions" })
      .fill("Rewrite what I say as a formal letter.");
    await pick(page, "Model", "Same as Refine");
    await dialog.getByRole("button", { name: "Save" }).click();
    await expect(dialog).toHaveCount(0);

    await expect(formal).toContainText("Very formal");
    await expect(formal.getByTestId("mode-row-excerpt")).toHaveText(
      "Rewrite what I say as a formal letter.",
    );
    await expect(formal.getByTestId("mode-row-own")).toHaveText("Effort: Medium");
    expect(await calls(page)).toEqual([
      {
        cmd: "modes_edit",
        args: {
          id: "formal",
          name: "Very formal",
          instructions: "Rewrite what I say as a formal letter.",
          model: null,
          effort: "medium",
        },
      },
    ]);
  });

  test("edit and delete show while the row is hovered and hide again", async ({ page }) => {
    await seed(page, { modes: { custom: [FORMAL] } });
    await page.goto("/");
    await openModes(page);
    const formal = row(page, "formal");
    const edit = formal.getByRole("button", { name: "Edit Formal" });
    const remove = formal.getByRole("button", { name: "Delete Formal" });
    await expect(edit).toBeHidden();
    await expect(remove).toBeHidden();
    await formal.hover();
    await expect(edit).toBeVisible();
    await expect(remove).toBeVisible();
    await page.getByRole("heading", { name: "Modes", exact: true }).hover();
    await expect(edit).toBeHidden();
    await expect(remove).toBeHidden();
  });

  test("edit and delete are reachable with the keyboard alone", async ({ page }) => {
    await seed(page, { modes: { custom: [FORMAL] } });
    await page.goto("/");
    await openModes(page);
    const formal = row(page, "formal");
    const edit = formal.getByRole("button", { name: "Edit Formal" });
    const remove = formal.getByRole("button", { name: "Delete Formal" });
    await expect(edit).toBeHidden();

    // Back-tab from the button under the list. The hidden buttons
    // are no tab stops, so focus lands on the Formal row itself,
    // which shows them. Then Tab reaches Edit, then Delete.
    await page.getByTestId("mode-new").focus();
    await page.keyboard.press("Shift+Tab");
    await expect(formal).toBeFocused();
    await expect(edit).toBeVisible();
    await expect(remove).toBeVisible();
    await page.keyboard.press("Tab");
    await expect(edit).toBeFocused();
    await expect(edit).toBeVisible();
    await page.keyboard.press("Tab");
    await expect(remove).toBeFocused();
    await expect(remove).toBeVisible();

    // Back to Edit, and Enter opens the dialog for this mode.
    await page.keyboard.press("Shift+Tab");
    await expect(edit).toBeFocused();
    await page.keyboard.press("Enter");
    const dialog = page.getByRole("dialog", { name: "Edit mode" });
    await expect(dialog).toBeVisible();
    await expect(dialog.getByRole("textbox", { name: "Name" })).toHaveValue("Formal");
    await page.keyboard.press("Escape");
    await expect(dialog).toHaveCount(0);

    // Focus comes back to the row, so the keyboard is not lost, and
    // Delete is two Tabs away. Enter asks first; Escape backs out.
    await expect(formal).toBeFocused();
    await expect(remove).toBeVisible();
    await page.keyboard.press("Tab");
    await page.keyboard.press("Tab");
    await expect(remove).toBeFocused();
    await page.keyboard.press("Enter");
    const confirm = page.getByRole("alertdialog");
    await expect(confirm).toContainText("Delete the mode “Formal”?");
    await page.keyboard.press("Escape");
    await expect(confirm).toHaveCount(0);
    await expect(formal).toBeFocused();

    // Deleting for real: the row is gone, focus moves to New mode.
    await page.keyboard.press("Tab");
    await page.keyboard.press("Tab");
    await page.keyboard.press("Enter");
    await page.getByTestId("mode-delete-confirm").focus();
    await page.keyboard.press("Enter");
    await expect(row(page, "formal")).toHaveCount(0);
    await expect(page.getByTestId("mode-new")).toBeFocused();
  });

  test("delete asks first; apps that used the mode and the selection fall back", async ({ page }) => {
    await seed(page, {
      modes: {
        active: "formal",
        custom: [FORMAL, ENGLISH],
        app_defaults: [
          { app_id: "com.example.chat", app_name: "Chatter", mode: "formal" },
          { app_id: "com.example.mail", app_name: "Mailbox", mode: "write-in-english" },
        ],
      },
      entries: ENTRIES,
    });
    await page.goto("/");
    await openModes(page);

    const formal = row(page, "formal");
    await expect(formal.getByTestId("mode-row-selected")).toBeVisible();
    const chatter = page.getByRole("combobox", { name: "Default mode for Chatter" });
    await expect(chatter).toContainText("Formal");

    await formal.hover();
    await formal.getByRole("button", { name: "Delete Formal" }).click();
    const dialog = page.getByRole("alertdialog");
    await expect(dialog).toContainText("Delete the mode “Formal”?");
    await expect(dialog).toContainText("Apps that use it by default go back to the selected mode");

    // Cancel keeps it.
    await dialog.getByRole("button", { name: "Cancel" }).click();
    await expect(dialog).toHaveCount(0);
    await expect(formal).toBeVisible();
    expect(await calls(page)).toHaveLength(0);

    await formal.hover();
    await formal.getByRole("button", { name: "Delete Formal" }).click();
    await page.getByTestId("mode-delete-confirm").click();
    await expect(formal).toHaveCount(0);
    await expect(page.getByTestId("mode-row")).toHaveCount(2);

    // Dictation is selected again, Chatter has no default any more,
    // and the app that used another mode keeps it.
    await expect(row(page, "dictation").getByTestId("mode-row-selected")).toBeVisible();
    await expect(chatter).toContainText("No default");
    await expect(page.getByRole("combobox", { name: "Default mode for Mailbox" })).toContainText(
      "Write in English",
    );
    expect(await calls(page)).toEqual([{ cmd: "modes_delete", args: { id: "formal" } }]);
  });
});

test.describe("modes pane: default mode per app", () => {
  test("says where the apps come from, and what to do when there are none", async ({ page }) => {
    await page.goto("/");
    await openModes(page);
    await expect(
      page.getByText("The list shows the apps that are open now and the apps in your history."),
    ).toBeVisible();
    const empty = page.getByTestId("modes-apps-empty");
    await expect(empty).toContainText("No apps to show");
    await expect(empty).toContainText("Open the app you want a default for");
    await expect(empty).toContainText("openwhisper modes app set");
    // History being off is no longer the reason there are no apps.
    await expect(empty).not.toContainText("History");
    await expect(page.getByTestId("mode-app-row")).toHaveCount(0);
  });

  test("with History off the open apps can be given a default", async ({ page }) => {
    // The default install: no history, so no stored dictations.
    await seed(page, {
      modes: { custom: [FORMAL] },
      history: { enabled: false },
      running: [
        { id: "com.example.Browser", name: "Browser" },
        { id: "com.example.chat", name: "Chatter" },
      ],
    });
    await page.goto("/");
    await openModes(page);
    await expect(page.getByTestId("modes-apps-empty")).toHaveCount(0);
    const rows = page.getByTestId("mode-app-row");
    await expect(rows).toHaveCount(2);
    await expect(rows.nth(0)).toContainText("Browser");
    // The id as core stores it: lower case.
    await expect(rows.nth(0)).toContainText("com.example.browser");
    await expect(rows.nth(1)).toContainText("Chatter");

    await pick(page, "Default mode for Browser", "Formal");
    await expect(page.getByRole("combobox", { name: "Default mode for Browser" })).toContainText(
      "Formal",
    );
    expect((await calls(page)).at(-1)).toEqual({
      cmd: "modes_set_app_default",
      args: { appId: "com.example.browser", appName: "Browser", modeId: "formal" },
    });
    // And it holds after a fresh read of the pane.
    await page.getByRole("tab", { name: "General" }).click();
    await page.getByRole("tab", { name: "Modes" }).click();
    await expect(page.getByRole("combobox", { name: "Default mode for Browser" })).toContainText(
      "Formal",
    );
  });

  test("apps with a default come first, then history, then open apps, each once", async ({ page }) => {
    await seed(page, {
      modes: {
        custom: [FORMAL],
        // Stored by hand with capitals: the same app as the one below.
        app_defaults: [{ app_id: "com.example.Notes", app_name: "", mode: "formal" }],
      },
      entries: ENTRIES,
      running: [
        { id: "COM.EXAMPLE.CHAT", name: "Chatter (open)" },
        { id: "com.example.notes", name: "Notes" },
        { id: "com.example.browser", name: "Browser" },
      ],
    });
    await page.goto("/");
    await openModes(page);
    const rows = page.getByTestId("mode-app-row");
    await expect(rows).toHaveCount(4);
    await expect(rows.nth(0)).toHaveAttribute("data-app", "com.example.notes");
    await expect(rows.nth(1)).toHaveAttribute("data-app", "com.example.chat");
    await expect(rows.nth(2)).toHaveAttribute("data-app", "com.example.mail");
    await expect(rows.nth(3)).toHaveAttribute("data-app", "com.example.browser");
    // The default had no name stored: the open app supplies it. The
    // default is found although the id was stored with capitals.
    await expect(page.getByRole("combobox", { name: "Default mode for Notes" })).toContainText(
      "Formal",
    );
    // The name from the history wins over the open app's.
    await expect(rows.nth(1)).toContainText("Chatter");
    await expect(rows.nth(1)).not.toContainText("(open)");
  });

  test("lists the apps from the history by name, with the id underneath", async ({ page }) => {
    await seed(page, { entries: ENTRIES });
    await page.goto("/");
    await openModes(page);
    const rows = page.getByTestId("mode-app-row");
    await expect(rows).toHaveCount(2);
    await expect(rows.nth(0)).toContainText("Chatter");
    await expect(rows.nth(0)).toContainText("com.example.chat");
    await expect(rows.nth(1)).toContainText("Mailbox");
    await expect(page.getByTestId("modes-apps-empty")).toHaveCount(0);
    await expect(page.getByRole("combobox", { name: "Default mode for Chatter" })).toContainText(
      "No default",
    );
  });

  test("an app gets a default mode and loses it again", async ({ page }) => {
    await seed(page, { modes: { custom: [FORMAL] }, entries: ENTRIES });
    await page.goto("/");
    await openModes(page);
    const chatter = page.getByRole("combobox", { name: "Default mode for Chatter" });

    await chatter.click();
    await expect(page.getByRole("option")).toHaveText(["No default", "Dictation", "Formal"]);
    await page.getByRole("option", { name: "Formal", exact: true }).click();
    await expect(chatter).toContainText("Formal");
    // The other app is untouched.
    await expect(page.getByRole("combobox", { name: "Default mode for Mailbox" })).toContainText(
      "No default",
    );

    await pick(page, "Default mode for Chatter", "No default");
    await expect(chatter).toContainText("No default");

    expect(await calls(page)).toEqual([
      {
        cmd: "modes_set_app_default",
        args: { appId: "com.example.chat", appName: "Chatter", modeId: "formal" },
      },
      { cmd: "modes_clear_app_default", args: { appId: "com.example.chat" } },
    ]);
  });

  test("an app that has a default is listed without any history", async ({ page }) => {
    // Set with `openwhisper modes app set`, which takes any app id.
    await seed(page, {
      modes: {
        custom: [FORMAL],
        app_defaults: [{ app_id: "editor.exe", app_name: "", mode: "formal" }],
      },
    });
    await page.goto("/");
    await openModes(page);
    await expect(page.getByTestId("mode-app-row")).toHaveCount(1);
    await expect(page.getByRole("combobox", { name: "Default mode for editor.exe" })).toContainText(
      "Formal",
    );
  });
});

test.describe("home: mode card", () => {
  test("is disabled while Refine is off, and says why", async ({ page }) => {
    await seed(page, { modes: { custom: [FORMAL] } });
    await page.goto("/");
    const select = page.getByTestId("home-mode-select");
    await expect(select).toBeDisabled();
    await expect(select).toContainText("Dictation");
    await expect(page.getByTestId("home-mode-note")).toHaveText("Modes apply when Refine is on.");

    // Turning Refine on, on the same screen, makes it usable.
    await page.getByTestId("home-refine-switch").click();
    await expect(select).toBeEnabled();
    await expect(page.getByTestId("home-mode-note")).toHaveText(
      "What Refine does with each dictation.",
    );
  });

  test("changes the selected mode", async ({ page }) => {
    await seed(page, { refine: { enabled: true }, modes: { custom: [FORMAL, ENGLISH] } });
    await page.goto("/");
    const select = page.getByTestId("home-mode-select");
    await expect(select).toBeEnabled();
    await expect(select).toContainText("Dictation");

    await select.click();
    await expect(page.getByRole("option")).toHaveText(["Dictation", "Formal", "Write in English"]);
    await page.getByRole("option", { name: "Formal", exact: true }).click();
    await expect(select).toContainText("Formal");
    expect(await calls(page)).toEqual([{ cmd: "modes_select", args: { id: "formal" } }]);

    // The Modes pane shows the same selection.
    await openModes(page);
    await expect(row(page, "formal").getByTestId("mode-row-selected")).toBeVisible();
    await expect(row(page, "dictation").getByTestId("mode-row-selected")).toHaveCount(0);
  });

  test("with only the built-in mode it offers that one", async ({ page }) => {
    await seed(page, { refine: { enabled: true } });
    await page.goto("/");
    const select = page.getByTestId("home-mode-select");
    await select.click();
    await expect(page.getByRole("option")).toHaveText(["Dictation"]);
  });

  test("opens the Modes settings", async ({ page }) => {
    await page.goto("/");
    await page.getByTestId("home-mode-settings").click();
    await expect(page.getByRole("heading", { name: "Modes", exact: true })).toBeVisible();
    await expect(page.getByRole("tab", { name: "Modes" })).toHaveAttribute("aria-selected", "true");
  });
});

test.describe("modes: changes made outside the window", () => {
  test("a switch by the hotkey updates Home and the Modes pane without a reload", async ({ page }) => {
    await seed(page, { refine: { enabled: true }, modes: { custom: [FORMAL, ENGLISH] } });
    await page.goto("/");
    const select = page.getByTestId("home-mode-select");
    await expect(select).toContainText("Dictation");
    await waitForModesListener(page);

    // What the switch-mode hotkey does: one step, then `modes_changed`.
    const cycle = () =>
      page.evaluate(() =>
        (
          window.__TAURI_INTERNALS__ as { invoke: (cmd: string) => Promise<unknown> }
        ).invoke("modes_cycle"),
      );
    await cycle();
    await expect(select).toContainText("Formal");
    await cycle();
    await expect(select).toContainText("Write in English");

    await openModes(page);
    await expect(row(page, "write-in-english").getByTestId("mode-row-selected")).toBeVisible();
    // After the last mode it starts again at Dictation.
    await cycle();
    await expect(row(page, "dictation").getByTestId("mode-row-selected")).toBeVisible();
    await expect(row(page, "write-in-english").getByTestId("mode-row-selected")).toHaveCount(0);
  });

  test("a mode added and selected elsewhere shows up in the open pane and on Home", async ({ page }) => {
    await seed(page, { refine: { enabled: true } });
    await page.goto("/");
    await openModes(page);
    await expect(page.getByTestId("mode-row")).toHaveCount(1);
    await waitForModesListener(page);

    // Not through this window's screens: the pane only hears about
    // these from `modes_changed`, as it would for the tray menu.
    const outside = (cmd: string, args: Record<string, unknown>) =>
      page.evaluate(
        ([c, a]) =>
          (
            window.__TAURI_INTERNALS__ as {
              invoke: (cmd: string, args: unknown) => Promise<unknown>;
            }
          ).invoke(c as string, a),
        [cmd, args] as const,
      );
    await outside("modes_add", {
      name: "Formal",
      instructions: "Rewrite what I say in a formal tone.",
      model: null,
      effort: null,
    });
    await expect(page.getByTestId("mode-row")).toHaveCount(2);
    await expect(row(page, "formal").getByTestId("mode-row-excerpt")).toHaveText(
      "Rewrite what I say in a formal tone.",
    );
    await expect(row(page, "dictation").getByTestId("mode-row-selected")).toBeVisible();

    await outside("modes_select", { id: "formal" });
    await expect(row(page, "formal").getByTestId("mode-row-selected")).toBeVisible();
    await expect(row(page, "dictation").getByTestId("mode-row-selected")).toHaveCount(0);

    await page.getByRole("button", { name: "Back to main" }).click();
    await expect(page.getByTestId("home-mode-select")).toContainText("Formal");
  });
});

test.describe("modes: which mode a dictation ran in", () => {
  const refined = (mode: Record<string, string | boolean>) => ({
    raw_transcript: "can you send me the numbers before friday ",
    status: "refined",
    detail: "",
    backend: "claude-cli",
    elapsed_ms: 900,
    ...mode,
  });

  test("the latest dictation names the mode, and the app when it was the app's default", async ({ page }) => {
    await page.goto("/");
    await waitForTickListener(page);
    await emitTick(page, { phase: 3, status: "transcribing" });
    await emitTick(page, {
      phase: 4,
      status: "idle",
      transcript: "Could you send me the figures before Friday? ",
      refine: refined({ mode: "Formal", mode_reason: "app-default", mode_app: "Chatter" }),
    });
    const marker = page.getByTestId("home-latest-row").getByTestId("home-latest-mode");
    await expect(marker).toHaveText("Mode: Formal, default for Chatter");

    // The selected mode needs no reason.
    await emitTick(page, { phase: 3, status: "transcribing" });
    await emitTick(page, {
      phase: 4,
      status: "idle",
      transcript: "Could you send me the figures before Monday? ",
      refine: refined({ mode: "Formal", mode_reason: "selected", mode_app: "" }),
    });
    await expect(marker).toHaveText("Mode: Formal");
  });

  test("Dictation is named when the app's default put it over the selected mode", async ({ page }) => {
    await page.goto("/");
    await waitForTickListener(page);
    await emitTick(page, { phase: 3, status: "transcribing" });
    await emitTick(page, {
      phase: 4,
      status: "idle",
      transcript: "Can you send me the numbers before Friday? ",
      // What Rust sends for the built-in mode as an app default.
      refine: refined({
        mode: "Dictation",
        mode_builtin: true,
        mode_reason: "app-default",
        mode_app: "Terminal",
      }),
    });
    await expect(page.getByTestId("home-latest-row").getByTestId("home-latest-mode")).toHaveText(
      "Mode: Dictation, default for Terminal",
    );
  });

  test("no marker for the built-in mode or when Refine did not run", async ({ page }) => {
    await page.goto("/");
    await waitForTickListener(page);
    await emitTick(page, { phase: 3, status: "transcribing" });
    await emitTick(page, {
      phase: 4,
      status: "idle",
      transcript: "Can you send me the numbers before Friday? ",
      refine: refined({ mode: "", mode_reason: "selected", mode_app: "" }),
    });
    await expect(page.getByTestId("home-latest-refine")).toContainText("Refined");
    await expect(page.getByTestId("home-latest-mode")).toHaveCount(0);

    await emitTick(page, { phase: 3, status: "transcribing" });
    await emitTick(page, { phase: 4, status: "idle", transcript: "typed as it was said " });
    await expect(page.getByTestId("home-latest-row")).toContainText("typed as it was said");
    await expect(page.getByTestId("home-latest-mode")).toHaveCount(0);
  });

  const WITH_MODE = [
    { ...ENTRIES[0], mode: "Formal" },
    { ...ENTRIES[1], mode: null },
  ];

  test("stored dictations show the mode they were written in, on Home and in History", async ({ page }) => {
    await seed(page, { history: { enabled: true }, entries: WITH_MODE });
    await page.goto("/");
    const rows = page.getByTestId("history-row");
    await expect(rows).toHaveCount(2);
    // A stored entry knows the name of the mode, not why it ran.
    await expect(rows.nth(0).getByTestId("history-row-mode")).toHaveText("Mode: Formal");
    await expect(rows.nth(1).getByTestId("history-row-mode")).toHaveCount(0);

    await page.getByTestId("sidebar-item-history").click();
    const all = page.getByTestId("history-row");
    await expect(all).toHaveCount(2);
    await expect(all.nth(0).getByTestId("history-row-mode")).toHaveText("Mode: Formal");
    await expect(all.nth(1).getByTestId("history-row-mode")).toHaveCount(0);
  });

  test("the dictation that just finished names its mode once, with the reason", async ({ page }) => {
    await seed(page, { history: { enabled: true }, entries: WITH_MODE });
    await page.goto("/");
    await waitForTickListener(page);
    await expect(page.getByTestId("history-row")).toHaveCount(2);

    await emitTick(page, { phase: 3, status: "transcribing" });
    await emitTick(page, {
      phase: 4,
      status: "idle",
      transcript: "The agenda is ready for Thursday. ",
      refine: {
        ...refined({ mode: "Formal", mode_reason: "app-default", mode_app: "Chatter" }),
        raw_transcript: "the agenda is ready for thursday ",
      },
    });
    const first = page.getByTestId("history-row").nth(0);
    await expect(first).toHaveAttribute("data-latest", "true");
    await expect(first.getByTestId("home-latest-mode")).toHaveText(
      "Mode: Formal, default for Chatter",
    );
    await expect(first.getByTestId("history-row-mode")).toHaveCount(0);
  });

  test("diagnostics: a Refine run made in a mode says which", async ({ page }) => {
    const now = Date.now();
    const run = (over: Record<string, unknown>) => ({
      at_ms: now - 60_000,
      source: "dictation",
      status: "refined",
      backend: "claude-cli",
      model: "sonnet",
      effort: "low",
      elapsed_ms: 1400,
      warm_lead_ms: 4200,
      chars_in: 120,
      chars_out: 123,
      instruction: false,
      detail: "",
      tokens_in: null,
      tokens_cached: null,
      tokens_out: null,
      cost_usd: null,
      window_5h_used: null,
      window_5h_resets_at: null,
      window_7d_used: null,
      window_7d_resets_at: null,
      mode: null,
      ...over,
    });
    const history = {
      summary: {
        total: 3,
        refined: 2,
        skipped: 0,
        fallbacks: 1,
        timeouts: 1,
        unavailable: 0,
        failed: 0,
        rejected: 0,
        instructions: 0,
        median_ms: 1400,
        p90_ms: 5003,
        max_ms: 5003,
        median_warm_ms: 1400,
        median_cold_ms: null,
        warm_runs: 3,
        cold_runs: 0,
        last_problem_at_ms: now - 30_000,
        last_problem: "no answer within 5.0 s",
        usage_runs: 0,
        avg_tokens_in: null,
        avg_tokens_out: null,
        avg_cost_usd: null,
        total_cost_usd: null,
        plan: null,
      },
      runs: [
        run({
          at_ms: now - 30_000,
          status: "timeout",
          elapsed_ms: 5003,
          detail: "no answer within 5.0 s",
          mode: "Write in English",
        }),
        run({ mode: "Formal" }),
        run({}),
      ],
    };
    await page.addInitScript((h) => {
      (window as unknown as { __owRefineHistory: unknown }).__owRefineHistory = h;
    }, history);
    await page.goto("/");
    await page.waitForSelector('[data-testid="sidebar-item-home"]');
    await page.getByTestId("sidebar-item-diagnostics").click();

    const rows = page.getByRole("table", { name: "Recent Refine runs" }).locator("tbody tr");
    await expect(rows).toHaveCount(3);
    await expect(rows.nth(0)).toContainText("Mode: Write in English · no answer within 5.0 s");
    await expect(rows.nth(1)).toContainText("Mode: Formal");
    // The built-in mode is not named.
    await expect(rows.nth(2)).not.toContainText("Mode:");
  });
});

pillTest.describe("pill: mode name after a switch", () => {
  const DICTATION = { id: "dictation", name: "Dictation", instructions: "", model: null, effort: null };
  const changed = (switchedTo: string | null): MockModesChanged => ({
    settings: { active: "formal", custom: [FORMAL], app_defaults: [] },
    modes: [
      { ...DICTATION, builtin: true },
      { ...FORMAL, builtin: false },
    ],
    switched_to: switchedTo,
  });

  const opacity = (page: Page) =>
    page.getByTestId("pill-mode-label").evaluate((el) => Number(getComputedStyle(el).opacity));

  async function mount(page: Page) {
    // The size of the real pill window.
    await page.setViewportSize({ width: 180, height: 110 });
    await page.goto("/");
    await page.locator(".pill-capsule").waitFor({ state: "visible" });
    await waitForModesListener(page);
  }

  pillTest("shows the new mode's name for a moment, then hides it", async ({ page }) => {
    await mount(page);
    const label = page.getByTestId("pill-mode-label");
    await expect(label).toHaveAttribute("data-visible", "false");
    expect(await opacity(page)).toBe(0);

    await emitModesChanged(page, changed("Formal"));
    await expect(label).toHaveText("Formal");
    await expect(label).toHaveAttribute("data-visible", "true");
    await expect.poll(() => opacity(page)).toBe(1);

    // About 1.5 s later it is gone again.
    await expect(label).toHaveAttribute("data-visible", "false", { timeout: 4000 });
    await expect.poll(() => opacity(page)).toBe(0);
  });

  pillTest("a second switch replaces the name and restarts the wait", async ({ page }) => {
    await mount(page);
    const label = page.getByTestId("pill-mode-label");
    await emitModesChanged(page, changed("Formal"));
    await expect(label).toHaveText("Formal");
    await page.waitForTimeout(900);
    await emitModesChanged(page, changed("Dictation"));
    await expect(label).toHaveText("Dictation");
    // Still up after the first name's time would have run out.
    await page.waitForTimeout(900);
    await expect(label).toHaveAttribute("data-visible", "true");
    await expect(label).toHaveAttribute("data-visible", "false", { timeout: 4000 });
  });

  pillTest("other changes to the modes show nothing", async ({ page }) => {
    await mount(page);
    await emitModesChanged(page, changed(null));
    // Give a label the time it would need to appear.
    await page.waitForTimeout(300);
    await expect(page.getByTestId("pill-mode-label")).toHaveAttribute("data-visible", "false");
    await expect(page.getByTestId("pill-mode-label")).toHaveText("");
  });

  pillTest("sits above the capsule, inside the window, and takes no clicks", async ({ page }) => {
    await mount(page);
    // The capsule at its largest: recording, 105x33.
    await page.evaluate(() =>
      window.__owEmit("pill_state", {
        status: "recording",
        levels: Array.from({ length: 12 }, () => 0.4),
      }),
    );
    const capsule = page.locator(".pill-capsule");
    await expect
      .poll(async () => Math.round((await capsule.boundingBox())?.height ?? 0), { timeout: 4000 })
      .toBe(33);

    // A name as long as core allows (40 characters).
    await emitModesChanged(page, changed("Prompt for a coding agent, in English x2"));
    const label = page.getByTestId("pill-mode-label");
    await expect(label).toHaveAttribute("data-visible", "true");

    const box = await label.boundingBox();
    const pill = await capsule.boundingBox();
    expect(box).not.toBeNull();
    expect(pill).not.toBeNull();
    if (!box || !pill) return;
    // Above the capsule, with a gap, and not past the window's edges.
    expect(box.y + box.height).toBeLessThanOrEqual(pill.y - 4);
    expect(box.y).toBeGreaterThanOrEqual(0);
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(180);

    // The capsule is the size it was: the label changes nothing there.
    expect(Math.round(pill.width)).toBe(105);
    expect(await label.evaluate((el) => getComputedStyle(el).pointerEvents)).toBe("none");
  });

  pillTest("the idle capsule keeps its size while the name shows", async ({ page }) => {
    await mount(page);
    await emitModesChanged(page, changed("Formal"));
    await expect(page.getByTestId("pill-mode-label")).toHaveAttribute("data-visible", "true");
    const pill = await page.locator(".pill-capsule").boundingBox();
    expect(Math.round(pill?.width ?? 0)).toBe(38);
    expect(Math.round(pill?.height ?? 0)).toBe(22);
  });
});
