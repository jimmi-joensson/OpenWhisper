import {
  emitAskChanged,
  emitTick,
  expect,
  test,
  waitForTickListener,
} from "./fixtures/tauri-shim";

type Page = import("@playwright/test").Page;

// Ask in the main window (TASK-97.5): the Ask settings pane, asks in
// the history lists, and asks among the Refine runs in Diagnostics.
// The answer window has its own spec (answer-window.spec.ts).

interface AskSettings {
  enabled: boolean;
  web_search: boolean;
  instructions: string;
  model: string | null;
  effort: string | null;
  timeout_ms: number;
}

async function openAsk(page: Page) {
  await page.waitForSelector('[data-testid="sidebar-item-home"]');
  await page.evaluate(() => window.__owEmit("ow_navigate", "settings"));
  await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
  await page.getByRole("tab", { name: "Ask", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Ask", exact: true })).toBeVisible();
}

// Read a value the shim recorded on `window`.
function recorded<T>(page: Page, key: string): Promise<T | undefined> {
  return page.evaluate(
    (k) => (window as unknown as Record<string, unknown>)[k] as T | undefined,
    key,
  );
}

async function lastSave(page: Page): Promise<AskSettings | undefined> {
  return (await recorded<AskSettings[]>(page, "__owAskSaves"))?.at(-1);
}

// Seed what the shim answers `ask_get` with.
async function seedAsk(
  page: Page,
  seed: { settings?: Partial<AskSettings>; backend?: Record<string, unknown> },
) {
  await page.addInitScript((s) => {
    const w = window as unknown as Record<string, unknown>;
    if (s.settings) w.__owAskSettings = s.settings;
    if (s.backend) w.__owAskBackend = s.backend;
  }, seed);
}

const OTHER_BACKEND = {
  backend: "openai-compatible",
  message: "Set up for small-model. Use the connection test to check it.",
  destination: "the server at models.example.com",
};

// Pick an option of an open select and wait for its list to go away,
// so the next select opens alone.
async function choose(page: Page, option: string) {
  await page.getByRole("option", { name: option, exact: true }).click();
  await expect(page.getByRole("option")).toHaveCount(0);
}

// How often the main window called an ask command.
async function askCalls(page: Page, cmd: string): Promise<number> {
  const calls = (await recorded<{ cmd: string }[]>(page, "__owAskCalls")) ?? [];
  return calls.filter((c) => c.cmd === cmd).length;
}

const askSwitch = (page: Page) => page.getByRole("switch", { name: "Ask", exact: true });
const webSearchSwitch = (page: Page) => page.getByRole("switch", { name: "Web search" });

test.describe("ask pane", () => {
  test("sits after Modes in the settings sidebar", async ({ page }) => {
    await page.goto("/");
    await openAsk(page);
    const tabs = await page.getByRole("tab").allTextContents();
    expect(tabs.slice(-2)).toEqual(["Modes", "Ask"]);
    await expect(page.getByRole("tab", { name: "Ask", exact: true })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });

  test("is off by default, says what it does and where questions are sent", async ({ page }) => {
    await page.goto("/");
    await openAsk(page);

    await expect(askSwitch(page)).not.toBeChecked();
    await expect(page.getByText("Gives you a second hotkey.")).toContainText(
      "ask a question out loud",
    );
    await expect(page.getByText("Gives you a second hotkey.")).toContainText(
      "The answer shows in a panel by the pill and is never typed by itself.",
    );
    const privacy = page.getByTestId("ask-privacy");
    await expect(privacy).toContainText(
      "the text of every question is sent to Anthropic (Claude), through the Claude Code app signed in on this computer.",
    );
    await expect(privacy).toContainText("Audio never leaves this computer.");
    // The backend can run: nothing to report.
    await expect(page.getByTestId("ask-backend-status")).toHaveCount(0);
    // And no way to send anything from here.
    await expect(page.getByRole("button", { name: /test/i })).toHaveCount(0);
  });

  test("only the Ask pane asks whether the backend can run", async ({ page }) => {
    await page.goto("/");
    // The app is up: the Ask settings were read (the Windows hotkey
    // fallback wants the switch), the backend was left alone.
    await page.waitForSelector('[data-testid="sidebar-item-home"]');
    await expect.poll(() => askCalls(page, "ask_get")).toBeGreaterThan(0);
    expect(await askCalls(page, "ask_status")).toBe(0);

    // The Shortcuts pane reads the switch too, from the same place.
    await page.evaluate(() => window.__owEmit("ow_navigate", "settings"));
    await page.getByRole("tab", { name: "Shortcuts" }).click();
    await expect(page.getByTestId("shortcut-off-ask")).toHaveText("Off");
    expect(await askCalls(page, "ask_status")).toBe(0);

    // The Ask pane is where the state of the backend is shown.
    await page.getByRole("tab", { name: "Ask", exact: true }).click();
    await expect(page.getByTestId("ask-privacy")).toContainText("Anthropic (Claude)");
    await expect.poll(() => askCalls(page, "ask_status")).toBe(1);

    // A change of the block is read again; the backend is not asked
    // again for it.
    const reads = await askCalls(page, "ask_get");
    await emitAskChanged(page, { enabled: true });
    await expect(askSwitch(page)).toBeChecked();
    await expect.poll(() => askCalls(page, "ask_get")).toBeGreaterThan(reads);
    expect(await askCalls(page, "ask_status")).toBe(1);
    // And the Shortcuts pane has followed, without a read of its own.
    await page.getByRole("tab", { name: "Shortcuts" }).click();
    await expect(page.getByTestId("shortcut-off-ask")).toHaveCount(0);
  });

  test("the switch saves on and off", async ({ page }) => {
    await page.goto("/");
    await openAsk(page);

    await askSwitch(page).click();
    await expect(askSwitch(page)).toBeChecked();
    await expect.poll(async () => (await lastSave(page))?.enabled).toBe(true);

    await askSwitch(page).click();
    await expect(askSwitch(page)).not.toBeChecked();
    await expect.poll(async () => (await lastSave(page))?.enabled).toBe(false);

    const saves = await recorded<AskSettings[]>(page, "__owAskSaves");
    expect(saves?.map((s) => s.enabled)).toEqual([true, false]);
    // The rest of the block is saved as it was.
    expect(saves?.at(-1)).toMatchObject({
      web_search: false,
      instructions: "",
      model: null,
      effort: null,
      timeout_ms: 60000,
    });
  });

  for (const [state, message] of [
    [
      "not-installed",
      "Claude Code is not installed on this computer. Install it from claude.com/claude-code, run `claude` once in a terminal and sign in.",
    ],
    [
      "not-signed-in",
      "Claude Code is installed but not signed in. Run `claude` in a terminal and sign in.",
    ],
    ["not-configured", "No model name is set."],
  ] as const) {
    test(`backend ${state}: says so and leads to the Refine pane`, async ({ page }) => {
      await seedAsk(page, { backend: { state, ready: false, message } });
      await page.goto("/");
      await openAsk(page);

      const status = page.getByTestId("ask-backend-status");
      await expect(status).toContainText("Not ready");
      await expect(status).toContainText(message);

      await status.getByRole("button", { name: "Open Refine settings" }).click();
      await expect(page.getByRole("tab", { name: "Refine" })).toHaveAttribute(
        "aria-selected",
        "true",
      );
      await expect(page.getByRole("heading", { name: "Refine", exact: true })).toBeVisible();
    });
  }

  test("shows the ask hotkey, read only, and leads to Shortcuts", async ({ page }) => {
    await page.goto("/");
    await openAsk(page);

    // The shim's default for the slot: a tap of Right Option.
    await expect(page.getByTestId("ask-hotkey")).toHaveText("Right ⌥");
    // Not a control: the key is changed under Shortcuts.
    await expect(page.getByRole("button", { name: /rebind/i })).toHaveCount(0);

    await page.getByRole("button", { name: "Open Shortcuts" }).click();
    await expect(page.getByRole("tab", { name: "Shortcuts" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await expect(page.getByRole("button", { name: "Rebind Ask" })).toBeVisible();
  });

  test("an ask hotkey that was cleared shows as not set", async ({ page }) => {
    await page.addInitScript(() => {
      (window as unknown as { __owHotkeys?: unknown }).__owHotkeys = {
        toggle: { kind: "chord", code: "Space", mods: ["Ctrl"] },
        cancel: { kind: "chord", code: "Escape", mods: [] },
        ask: null,
      };
    });
    await page.goto("/");
    await openAsk(page);
    await expect(page.getByTestId("ask-hotkey")).toHaveText("Not set");
  });

  test("web search is off by default and saves when switched on", async ({ page }) => {
    await page.goto("/");
    await openAsk(page);

    await expect(webSearchSwitch(page)).not.toBeChecked();
    await expect(webSearchSwitch(page)).toBeEnabled();
    const note = page.getByTestId("ask-web-search-note");
    await expect(note).toContainText("look things up on the web");
    await expect(note).toContainText("Answers take longer");
    await expect(note).toContainText("Works with the Claude backend only");

    await webSearchSwitch(page).click();
    await expect(webSearchSwitch(page)).toBeChecked();
    await expect.poll(async () => (await lastSave(page))?.web_search).toBe(true);
    // Only that changed.
    expect((await lastSave(page))?.enabled).toBe(false);
  });

  test("on the OpenAI-compatible backend web search is disabled and Refine's model is used", async ({
    page,
  }) => {
    // Web search was switched on while Claude was the backend.
    await seedAsk(page, { settings: { web_search: true }, backend: OTHER_BACKEND });
    await page.goto("/");
    await openAsk(page);

    await expect(page.getByTestId("ask-privacy")).toContainText(
      "the text of every question is sent to the server at models.example.com.",
    );
    await expect(webSearchSwitch(page)).toBeDisabled();
    await expect(webSearchSwitch(page)).not.toBeChecked();
    await expect(page.getByTestId("ask-web-search-note")).toHaveText(
      "Web search only works with the Claude backend. Refine is set up with an OpenAI-compatible server, so it is not available.",
    );

    // No model or effort of its own here.
    await expect(page.getByRole("combobox", { name: "Model" })).toHaveCount(0);
    await expect(page.getByRole("combobox", { name: "Effort" })).toHaveCount(0);
    await expect(page.getByTestId("ask-model-note")).toHaveText(
      "Questions are answered by the model set up under Refine (small-model).",
    );
    // The instructions still apply.
    await expect(page.getByLabel("Instructions")).toBeEnabled();
  });

  test("model and effort start at Default and save a choice", async ({ page }) => {
    await page.goto("/");
    await openAsk(page);
    const model = page.getByRole("combobox", { name: "Model" });
    const effort = page.getByRole("combobox", { name: "Effort" });
    await expect(model).toContainText("Default");
    await expect(effort).toContainText("Default");
    await expect(page.getByTestId("ask-model-note")).toHaveCount(0);

    // The shared lists, with Default first.
    await model.click();
    await expect(page.getByRole("option")).toHaveText([
      "Default",
      "Fable",
      "Opus",
      "Sonnet",
      "Haiku",
    ]);
    await choose(page, "Haiku");
    await expect(model).toContainText("Haiku");
    await expect.poll(async () => (await lastSave(page))?.model).toBe("haiku");

    await effort.click();
    await expect(page.getByRole("option")).toHaveText([
      "Default",
      "Low",
      "Medium",
      "High",
      "Extra high",
      "Max",
    ]);
    await choose(page, "High");
    await expect(effort).toContainText("High");
    await expect.poll(async () => (await lastSave(page))?.effort).toBe("high");
    expect((await lastSave(page))?.model).toBe("haiku");

    // Back to Default stores nothing, so core picks.
    await model.click();
    await choose(page, "Default");
    await expect(model).toContainText("Default");
    await expect.poll(async () => (await lastSave(page))?.model).toBe(null);
    await effort.click();
    await choose(page, "Default");
    await expect.poll(async () => (await lastSave(page))?.effort).toBe(null);
  });

  test("a model name saved elsewhere is shown and offered", async ({ page }) => {
    await seedAsk(page, { settings: { model: "claude-test-model", effort: "medium" } });
    await page.goto("/");
    await openAsk(page);
    const model = page.getByRole("combobox", { name: "Model" });
    await expect(model).toContainText("claude-test-model");
    await expect(page.getByRole("combobox", { name: "Effort" })).toContainText("Medium");
    await model.click();
    await expect(page.getByRole("option").last()).toHaveText("claude-test-model");
  });

  test("instructions are saved when the field is left, not while typing", async ({ page }) => {
    await page.goto("/");
    await openAsk(page);
    const field = page.getByLabel("Instructions");
    await expect(field).toHaveValue("");

    await field.fill("  Answer briefly, and in Danish when I ask in Danish.  ");
    expect(await recorded<AskSettings[]>(page, "__owAskSaves")).toBeUndefined();

    await field.blur();
    await expect
      .poll(async () => (await lastSave(page))?.instructions)
      .toBe("Answer briefly, and in Danish when I ask in Danish.");
    await expect(field).toHaveValue("Answer briefly, and in Danish when I ask in Danish.");

    // Leaving it unchanged saves nothing more.
    await field.focus();
    await field.blur();
    expect(await recorded<AskSettings[]>(page, "__owAskSaves")).toHaveLength(1);

    // Emptying it removes them.
    await field.fill("");
    await field.blur();
    await expect.poll(async () => (await lastSave(page))?.instructions).toBe("");
  });

  test("a change made elsewhere shows on the pane", async ({ page }) => {
    await page.goto("/");
    await openAsk(page);
    await expect(askSwitch(page)).not.toBeChecked();

    // The headless CLI rewrites the block; Rust announces it.
    await emitAskChanged(page, {
      enabled: true,
      web_search: true,
      instructions: "Answer in one sentence.",
      model: "opus",
      effort: "high",
    });
    await expect(askSwitch(page)).toBeChecked();
    await expect(webSearchSwitch(page)).toBeChecked();
    await expect(page.getByLabel("Instructions")).toHaveValue("Answer in one sentence.");
    await expect(page.getByRole("combobox", { name: "Model" })).toContainText("Opus");
    await expect(page.getByRole("combobox", { name: "Effort" })).toContainText("High");

    // The pane re-read the state, and a change made here builds on
    // the block from outside.
    await askSwitch(page).click();
    await expect.poll(async () => (await lastSave(page))?.enabled).toBe(false);
    expect(await lastSave(page)).toMatchObject({
      web_search: true,
      instructions: "Answer in one sentence.",
      model: "opus",
      effort: "high",
    });
  });

  test("a change made while another pane is open is there when Ask opens", async ({ page }) => {
    await page.goto("/");
    await openAsk(page);
    await page.getByRole("tab", { name: "General" }).click();
    await emitAskChanged(page, { enabled: true });
    await page.getByRole("tab", { name: "Ask", exact: true }).click();
    await expect(askSwitch(page)).toBeChecked();
  });

  test("a save that fails is undone and reported", async ({ page }) => {
    await page.goto("/");
    await openAsk(page);
    await page.evaluate(() => {
      const w = window as unknown as {
        __TAURI_INTERNALS__: { invoke: (cmd: string, args?: unknown) => Promise<unknown> };
      };
      const invoke = w.__TAURI_INTERNALS__.invoke;
      w.__TAURI_INTERNALS__.invoke = (cmd, args) =>
        cmd === "ask_set"
          ? Promise.reject("The settings file could not be written.")
          : invoke(cmd, args);
    });
    await askSwitch(page).click();
    await expect(page.getByRole("alert")).toContainText("The settings file could not be written.");
    await expect(askSwitch(page)).not.toBeChecked();
  });
});

// --- asks in the history -------------------------------------------

interface SeedEntry {
  id: string;
  hoursAgo: number;
  text: string;
  original: string | null;
  app_id: string | null;
  app_name: string | null;
  kind?: "ask";
  answer?: string;
}

async function seedHistory(page: Page, entries: SeedEntry[]) {
  await page.addInitScript((list: SeedEntry[]) => {
    const w = window as unknown as {
      __owHistorySettings?: unknown;
      __owHistoryEntries?: unknown[];
    };
    w.__owHistorySettings = { enabled: true };
    const noon = new Date();
    noon.setHours(12, 0, 0, 0);
    w.__owHistoryEntries = list.map(({ hoursAgo, ...e }) => ({
      ...e,
      at_ms: noon.getTime() - hoursAgo * 3_600_000,
      duration_ms: 4000,
      word_count: e.text.split(/\s+/).length,
    }));
  }, entries);
}

const ANSWER = "A tap is a short press of a key.\nYou let go again right away.";

const WITH_ASK: SeedEntry[] = [
  {
    id: "a2",
    hoursAgo: 1,
    text: "what is a tap",
    original: null,
    app_id: null,
    app_name: null,
    kind: "ask",
    answer: ANSWER,
  },
  {
    id: "e2",
    hoursAgo: 2,
    text: "Send the report to Anna before Friday.",
    original: "send the reaport to anna before friday",
    app_id: "com.example.mail",
    app_name: "Mailbox",
  },
  {
    id: "e1",
    hoursAgo: 3,
    text: "Remember to book the meeting room.",
    original: null,
    app_id: null,
    app_name: null,
  },
];

test.describe("history: asks", () => {
  test("an ask on Home is marked, shows the question, and has the answer one click away", async ({
    page,
  }) => {
    await seedHistory(page, WITH_ASK);
    await page.goto("/");
    const rows = page.getByTestId("history-row");
    await expect(rows).toHaveCount(3);

    const ask = rows.nth(0);
    await expect(ask).toHaveAttribute("data-kind", "ask");
    await expect(ask.getByTestId("history-row-ask")).toHaveText("Ask");
    await expect(ask.locator(".ow-history-row__text")).toHaveText("what is a tap");
    await expect(ask.getByTestId("history-row-answer")).toHaveCount(0);

    await ask.getByRole("button", { name: "Show answer" }).click();
    const answer = ask.getByTestId("history-row-answer");
    await expect(answer).toHaveText(ANSWER);
    // The answer keeps its line breaks.
    expect(await answer.evaluate((el) => getComputedStyle(el).whiteSpace)).toBe("pre-wrap");
    await ask.getByRole("button", { name: "Hide answer" }).click();
    await expect(ask.getByTestId("history-row-answer")).toHaveCount(0);

    // Dictations are not marked and have no answer.
    for (const i of [1, 2]) {
      await expect(rows.nth(i)).not.toHaveAttribute("data-kind", "ask");
      await expect(rows.nth(i).getByTestId("history-row-ask")).toHaveCount(0);
      await expect(rows.nth(i).getByRole("button", { name: "Show answer" })).toHaveCount(0);
    }
    // And a refined dictation still offers its original.
    await expect(rows.nth(1).getByRole("button", { name: "Show original" })).toBeVisible();
  });

  test("copy takes the question, or the answer while it is shown", async ({ page, context }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    await seedHistory(page, WITH_ASK);
    await page.goto("/");
    const ask = page.getByTestId("history-row").nth(0);
    const copy = ask.getByTestId("history-row-copy");
    const clipboard = () => page.evaluate(() => navigator.clipboard.readText());

    await ask.hover();
    await expect(copy).toHaveAttribute("aria-label", "Copy question");
    await copy.click();
    await expect(copy).toHaveAttribute("aria-label", "Copied");
    expect(await clipboard()).toBe("what is a tap");

    await ask.getByRole("button", { name: "Show answer" }).click();
    await expect(copy).toHaveAttribute("aria-label", "Copy answer", { timeout: 3000 });
    await copy.click();
    await expect(copy).toHaveAttribute("aria-label", "Copied");
    expect(await clipboard()).toBe(ANSWER);

    // A dictation's copy button is as before.
    const dictation = page.getByTestId("history-row").nth(1);
    await dictation.hover();
    await expect(dictation.getByTestId("history-row-copy")).toHaveAttribute(
      "aria-label",
      "Copy transcript",
    );
  });

  test("the History screen marks asks, shows the answer and finds them by it", async ({
    page,
  }) => {
    await seedHistory(page, WITH_ASK);
    await page.goto("/");
    await page.getByTestId("sidebar-item-history").click();
    await expect(page.getByTestId("history-pane")).toBeVisible();
    const rows = page.getByTestId("history-row");
    await expect(rows).toHaveCount(3);
    await expect(page.getByTestId("history-row-ask")).toHaveCount(1);

    // Words that are only in the answer.
    await page.getByTestId("history-search").fill("let go again");
    await expect(rows).toHaveCount(1);
    await expect(rows.nth(0).getByTestId("history-row-ask")).toHaveText("Ask");
    await expect(rows.nth(0).locator(".ow-history-row__text")).toHaveText("what is a tap");
    await rows.nth(0).getByRole("button", { name: "Show answer" }).click();
    await expect(rows.nth(0).getByTestId("history-row-answer")).toContainText(
      "You let go again right away.",
    );

    // An ask can be deleted like any entry.
    await rows.nth(0).hover();
    await rows.nth(0).getByTestId("history-row-delete").click();
    await expect.poll(() => recorded<string[]>(page, "__owHistoryDeleted")).toEqual(["a2"]);
  });

  test("an ask above the dictation that just finished does not push it into a row of its own", async ({
    page,
  }) => {
    await seedHistory(page, WITH_ASK);
    await page.goto("/");
    await waitForTickListener(page);
    await expect(page.getByTestId("history-row")).toHaveCount(3);

    // The newest dictation (the second entry, under the ask) finishes.
    await emitTick(page, { phase: 3, status: "transcribing" });
    await emitTick(page, {
      phase: 4,
      status: "idle",
      transcript: "Send the report to Anna before Friday. ",
      replacements: [],
      refine: {
        raw_transcript: "send the reaport to anna before friday ",
        status: "refined",
        detail: "",
        backend: "claude-cli",
        elapsed_ms: 800,
      },
    });

    const dictation = page.getByTestId("history-row").nth(1);
    await expect(dictation).toHaveAttribute("data-latest", "true");
    await expect(dictation.getByTestId("home-latest-refine")).toContainText("Refined");
    await expect(page.getByTestId("home-latest-row")).toHaveCount(0);
    // The ask is not taken for it.
    await expect(page.getByTestId("history-row").nth(0)).not.toHaveAttribute("data-latest", "true");
  });
});

// --- asks in Diagnostics -------------------------------------------

test.describe("diagnostics: asks among the Refine runs", () => {
  async function openDiagnostics(page: Page) {
    await page.waitForSelector('[data-testid="sidebar-item-home"]');
    await page.getByTestId("sidebar-item-diagnostics").click();
    await expect(page.getByTestId("diagnostics-refine")).toBeVisible();
  }

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
    tokens_in: 640,
    tokens_cached: 571,
    tokens_out: 22,
    cost_usd: 0.0005,
    window_5h_used: null,
    window_5h_resets_at: null,
    window_7d_used: null,
    window_7d_resets_at: null,
    mode: null,
    first_text_ms: null,
    ...over,
  });
  const ask = (over: Record<string, unknown>) =>
    run({ source: "ask", status: "answered", tokens_in: 900, tokens_out: 180, ...over });
  const summary = (over: Record<string, unknown>) => ({
    total: 0,
    asks: 0,
    asks_answered: 0,
    ask_median_first_text_ms: null,
    ask_median_ms: null,
    refined: 0,
    skipped: 0,
    fallbacks: 0,
    timeouts: 0,
    unavailable: 0,
    failed: 0,
    rejected: 0,
    instructions: 0,
    median_ms: null,
    p90_ms: null,
    max_ms: null,
    median_warm_ms: null,
    median_cold_ms: null,
    warm_runs: 0,
    cold_runs: 0,
    last_problem_at_ms: null,
    last_problem: "",
    usage_runs: 0,
    avg_tokens_in: null,
    avg_tokens_out: null,
    avg_cost_usd: null,
    total_cost_usd: null,
    plan: null,
    ...over,
  });

  const MIXED = {
    summary: summary({
      total: 5,
      asks: 3,
      asks_answered: 1,
      ask_median_first_text_ms: 620,
      ask_median_ms: 2400,
      refined: 2,
      median_ms: 1400,
      p90_ms: 1500,
      max_ms: 1500,
      median_warm_ms: 1400,
      warm_runs: 2,
    }),
    runs: [
      ask({ at_ms: now - 20_000, elapsed_ms: 2400, first_text_ms: 620 }),
      ask({
        at_ms: now - 30_000,
        status: "cancelled",
        elapsed_ms: 900,
        tokens_in: null,
        tokens_out: null,
      }),
      run({ at_ms: now - 40_000 }),
      ask({
        at_ms: now - 50_000,
        status: "timeout",
        elapsed_ms: 60_000,
        warm_lead_ms: null,
        detail: "no answer within 60 s",
        tokens_in: null,
        tokens_out: null,
      }),
      run({ elapsed_ms: 1500 }),
    ],
  };

  test("ask rows say Ask, their outcome in words and the time to the first text", async ({
    page,
  }) => {
    await page.addInitScript((history) => {
      const w = window as unknown as Record<string, unknown>;
      w.__owRefineHistory = history;
      w.__owRefineSettings = { enabled: true };
    }, MIXED);
    await page.goto("/");
    await openDiagnostics(page);

    const rows = page.getByRole("table", { name: "Recent Refine runs" }).locator("tbody tr");
    await expect(rows).toHaveCount(5);

    await expect(rows.nth(0)).toHaveAttribute("data-source", "ask");
    await expect(rows.nth(0).locator("td").nth(1)).toHaveText("Answered");
    await expect(rows.nth(0).locator("td").nth(2)).toHaveText("2.40 s");
    await expect(rows.nth(0).locator("td").last()).toHaveText("Ask · first text after 620 ms");
    await expect(rows.nth(0)).toContainText("900 → 180");

    await expect(rows.nth(1).locator("td").nth(1)).toHaveText("Cancelled");
    await expect(rows.nth(1).locator("td").last()).toHaveText("Ask");

    await expect(rows.nth(3).locator("td").nth(1)).toHaveText("Timed out");
    await expect(rows.nth(3).locator("td").last()).toHaveText("Ask · no answer within 60 s");

    // Refine runs are as before.
    await expect(rows.nth(2)).toHaveAttribute("data-source", "dictation");
    await expect(rows.nth(2).locator("td").nth(1)).toHaveText("Refined");
    await expect(rows.nth(2).locator("td").last()).toHaveText("");
  });

  test("the summary counts asks on their own, outside the Refine numbers", async ({ page }) => {
    await page.addInitScript((history) => {
      const w = window as unknown as Record<string, unknown>;
      w.__owRefineHistory = history;
      w.__owRefineSettings = { enabled: true };
    }, MIXED);
    await page.goto("/");
    await openDiagnostics(page);

    await expect(page.getByTestId("diagnostics-refine-asks")).toHaveText(
      "Asks: 1 of 3 answered. Median: first text after 620 ms, whole answer after 2.40 s",
    );
    // 5 rows, 3 of them asks: 2 of 2 dictations refined.
    await expect(page.getByTestId("diagnostics-refine-refined")).toHaveText("2 of 2");
    await expect(page.getByTestId("diagnostics-refine-median")).toHaveText("1.40 s");
  });

  test("with only asks there are no Refine numbers, just the asks", async ({ page }) => {
    await page.addInitScript(
      (history) => {
        (window as unknown as Record<string, unknown>).__owRefineHistory = history;
      },
      {
        summary: summary({ total: 2, asks: 2, asks_answered: 0 }),
        runs: [
          ask({ status: "cancelled", elapsed_ms: 700 }),
          ask({ status: "failed", elapsed_ms: 300, detail: "the server answered 500" }),
        ],
      },
    );
    await page.goto("/");
    await openDiagnostics(page);

    await expect(page.getByTestId("diagnostics-refine-asks")).toHaveText("Asks: 0 of 2 answered");
    await expect(page.getByTestId("diagnostics-refine-median")).toHaveCount(0);
    await expect(page.getByTestId("diagnostics-refine-refined")).toHaveCount(0);
    await expect(page.getByTestId("diagnostics-refine-empty")).toHaveCount(0);
    const rows = page.getByRole("table", { name: "Recent Refine runs" }).locator("tbody tr");
    await expect(rows).toHaveCount(2);
    await expect(rows.nth(1).locator("td").nth(1)).toHaveText("Failed");
    await expect(rows.nth(1).locator("td").last()).toHaveText("Ask · the server answered 500");
  });

  test("without asks there is no ask line", async ({ page }) => {
    await page.addInitScript(
      (history) => {
        (window as unknown as Record<string, unknown>).__owRefineHistory = history;
      },
      {
        summary: summary({ total: 1, refined: 1, median_ms: 1400, p90_ms: 1400, max_ms: 1400 }),
        runs: [run({})],
      },
    );
    await page.goto("/");
    await openDiagnostics(page);
    await expect(page.getByTestId("diagnostics-refine-refined")).toHaveText("1 of 1");
    await expect(page.getByTestId("diagnostics-refine-asks")).toHaveCount(0);
  });
});
