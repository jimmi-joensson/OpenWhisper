import {
  emitTick,
  expect,
  test,
  waitForTickListener,
} from "./fixtures/tauri-shim";

type Page = import("@playwright/test").Page;

async function openRefine(page: Page) {
  await page.waitForSelector('[data-testid="sidebar-item-home"]');
  await page.evaluate(() => window.__owEmit("ow_navigate", "settings"));
  await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
  await page.getByRole("tab", { name: "Refine" }).click();
  await expect(page.getByRole("heading", { name: "Refine", exact: true })).toBeVisible();
}

// Read a value the shim recorded on `window`.
function recorded<T>(page: Page, key: string): Promise<T | undefined> {
  return page.evaluate(
    (k) => (window as unknown as Record<string, unknown>)[k] as T | undefined,
    key,
  );
}

test.describe("refine pane", () => {
  test("is off by default and says where the text goes", async ({ page }) => {
    await page.goto("/");
    await openRefine(page);

    await expect(page.getByRole("switch", { name: "Refine dictations" })).not.toBeChecked();
    const privacy = page.getByTestId("refine-privacy");
    await expect(privacy).toContainText(
      "the text of every dictation is sent to Anthropic (Claude), through the Claude Code app signed in on this computer",
    );
    await expect(privacy).toContainText("Audio never leaves this computer");
    await expect(privacy).toContainText("within 5 seconds");
    await expect(privacy).toContainText("typed as transcribed");
  });

  test("turning it on saves enabled=true", async ({ page }) => {
    await page.goto("/");
    await openRefine(page);
    await page.getByRole("switch", { name: "Refine dictations" }).click();
    await expect(page.getByRole("switch", { name: "Refine dictations" })).toBeChecked();
    await expect
      .poll(async () => (await recorded<{ enabled: boolean }[]>(page, "__owRefineSaves"))?.at(-1)?.enabled)
      .toBe(true);
  });

  test("shows the signed-in Claude subscription as ready", async ({ page }) => {
    await page.goto("/");
    await openRefine(page);
    const status = page.getByTestId("refine-backend-status");
    await expect(status).toHaveAttribute("data-ready", "true");
    await expect(status).toContainText("Ready");
    await expect(status).toContainText("signed in with your Claude subscription (max)");
  });

  test("Claude Code not installed: says so and offers a re-check", async ({ page }) => {
    await page.addInitScript(() => {
      (window as unknown as { __owRefineStatus: unknown }).__owRefineStatus = {
        state: "not-installed",
        ready: false,
        message:
          "Claude Code is not installed on this computer. Install it from claude.com/claude-code, run `claude` once in a terminal and sign in.",
      };
    });
    await page.goto("/");
    await openRefine(page);
    const status = page.getByTestId("refine-backend-status");
    await expect(status).toHaveAttribute("data-ready", "false");
    await expect(status).toContainText("Not ready");
    await expect(status).toContainText("Claude Code is not installed on this computer");

    // After installing, "Check again" re-runs the probe.
    await page.evaluate(() => {
      (window as unknown as { __owRefineStatus?: unknown }).__owRefineStatus = undefined;
    });
    await status.getByRole("button", { name: "Check again" }).click();
    await expect(status).toHaveAttribute("data-ready", "true");
  });

  test("Claude Code not signed in: says what to do", async ({ page }) => {
    await page.addInitScript(() => {
      (window as unknown as { __owRefineStatus: unknown }).__owRefineStatus = {
        state: "not-signed-in",
        ready: false,
        message:
          "Claude Code is installed but not signed in. Run `claude` in a terminal and sign in.",
      };
    });
    await page.goto("/");
    await openRefine(page);
    await expect(page.getByTestId("refine-backend-status")).toContainText(
      "installed but not signed in. Run `claude` in a terminal and sign in.",
    );
  });

  test("test connection shows what came back and how long it took", async ({ page }) => {
    await page.goto("/");
    await openRefine(page);
    await page.getByRole("button", { name: "Test connection" }).click();
    const result = page.getByTestId("refine-test-result");
    await expect(result).toContainText("Working. Answered in 1.2 s.");
    await expect(result).toContainText("so um this is a test of the refine step");
    await expect(result).toContainText("This is a test of the Refine step.");
    expect(await recorded<number>(page, "__owRefineTestCount")).toBe(1);
  });

  test("test connection shows the failure reason", async ({ page }) => {
    await page.addInitScript(() => {
      (window as unknown as { __owRefineTest: unknown }).__owRefineTest = {
        ok: false,
        output: "",
        message: "no answer within 5.0 s",
      };
    });
    await page.goto("/");
    await openRefine(page);
    await page.getByRole("button", { name: "Test connection" }).click();
    const result = page.getByTestId("refine-test-result");
    await expect(result).toContainText("Not working");
    await expect(result).toContainText("no answer within 5.0 s");
  });

  test("OpenAI-compatible: fields save, privacy line names the server", async ({ page }) => {
    await page.goto("/");
    await openRefine(page);
    // No server fields while Claude is selected.
    await expect(page.getByRole("textbox", { name: "Server address" })).toHaveCount(0);

    await page.getByRole("group", { name: "Refine model" }).getByText("OpenAI-compatible").click();
    const url = page.getByRole("textbox", { name: "Server address" });
    await expect(url).toHaveValue("https://api.openai.com/v1");
    await expect(page.getByTestId("refine-privacy")).toContainText("the server at api.openai.com");

    // Not ready until a model is named.
    const status = page.getByTestId("refine-backend-status");
    await expect(status).toHaveAttribute("data-ready", "false");
    await expect(status).toContainText("No model name is set.");

    await url.fill("http://localhost:11434/v1/");
    await page.getByRole("textbox", { name: "Model name" }).fill("  llama-small ");
    await page.getByRole("textbox", { name: "Model name" }).press("Enter");

    await expect(status).toHaveAttribute("data-ready", "true");
    await expect(page.getByTestId("refine-privacy")).toContainText("the server at localhost:11434");
    const last = (await recorded<{ backend: string; openai: { base_url: string; model: string } }[]>(
      page,
      "__owRefineSaves",
    ))?.at(-1);
    expect(last?.backend).toBe("openai-compatible");
    expect(last?.openai).toEqual({
      base_url: "http://localhost:11434/v1",
      model: "llama-small",
      effort: null,
    });
  });

  test("API key goes to the keychain command and is never shown again", async ({ page }) => {
    await page.addInitScript(() => {
      (window as unknown as { __owRefineSettings: unknown }).__owRefineSettings = {
        backend: "openai-compatible",
        openai: { base_url: "https://api.openai.com/v1", model: "small-model" },
      };
    });
    await page.goto("/");
    await openRefine(page);

    const key = page.getByLabel("API key");
    await expect(key).toHaveAttribute("type", "password");
    await expect(key).toHaveAttribute("placeholder", "Paste your key");
    await expect(page.getByTestId("refine-key-note")).toContainText("never in a settings file");
    await expect(page.getByRole("button", { name: "Save key" })).toBeDisabled();

    await key.fill("sk-secret-123");
    await page.getByRole("button", { name: "Save key" }).click();

    // Field is cleared; the stored key is acknowledged, not echoed.
    await expect(key).toHaveValue("");
    await expect(key).toHaveAttribute("placeholder", /A key is stored/);
    expect(await recorded<string>(page, "__owRefineKey")).toBe("sk-secret-123");
    await expect(page.locator("body")).not.toContainText("sk-secret-123");
    // And it was never part of a settings save.
    const saves = (await recorded<unknown[]>(page, "__owRefineSaves")) ?? [];
    expect(JSON.stringify(saves)).not.toContain("sk-secret-123");

    await page.getByRole("button", { name: "Remove" }).click();
    await expect(key).toHaveAttribute("placeholder", "Paste your key");
    expect(await recorded<string>(page, "__owRefineKey")).toBeUndefined();
  });

  test("instructions: default shown, custom saved, reset goes back", async ({ page }) => {
    await page.goto("/");
    await openRefine(page);
    const box = page.getByRole("textbox", { name: "What Refine should always do" });
    await expect(box).toHaveValue(/^Fix punctuation and capitalization\./);
    await expect(
      page.getByRole("button", { name: "Back to the standard instructions" }),
    ).toHaveCount(0);

    await box.fill("Fix punctuation only. Keep every word.");
    await box.blur();
    await expect
      .poll(async () =>
        (await recorded<{ instructions: string }[]>(page, "__owRefineSaves"))?.at(-1)?.instructions,
      )
      .toBe("Fix punctuation only. Keep every word.");

    await page.getByRole("button", { name: "Back to the standard instructions" }).click();
    await expect(box).toHaveValue(/^Fix punctuation and capitalization\./);
    // Stored as "" so the built-in text stays in charge.
    await expect
      .poll(async () =>
        (await recorded<{ instructions: string }[]>(page, "__owRefineSaves"))?.at(-1)?.instructions,
      )
      .toBe("");
  });

  async function pick(page: Page, combobox: string, option: string) {
    await page.getByRole("combobox", { name: combobox }).click();
    await page.getByRole("option", { name: option, exact: true }).click();
  }

  test("Claude model and effort are drop-downs: Sonnet and Low by default", async ({ page }) => {
    await page.goto("/");
    await openRefine(page);
    const model = page.getByRole("combobox", { name: "Claude model" });
    const effort = page.getByRole("combobox", { name: "Effort" });
    await expect(model).toContainText("Sonnet");
    await expect(effort).toContainText("Low");

    // Everything the CLI takes is on offer, not a hand-picked pair.
    await model.click();
    for (const name of ["Fable", "Opus", "Sonnet", "Haiku", "Other…"]) {
      await expect(page.getByRole("option", { name, exact: true })).toBeVisible();
    }
    await page.getByRole("option", { name: "Opus", exact: true }).click();
    await expect(model).toContainText("Opus");
    await expect(page.getByTestId("refine-model-note")).toContainText("Not measured here");

    await effort.click();
    for (const name of ["Low", "Medium", "High", "Extra high", "Max"]) {
      await expect(page.getByRole("option", { name, exact: true })).toBeVisible();
    }
    await page.getByRole("option", { name: "Extra high", exact: true }).click();
    await expect(effort).toContainText("Extra high");

    const last = (await recorded<{ claude_cli: { model: string; effort: string } }[]>(
      page,
      "__owRefineSaves",
    ))?.at(-1);
    expect(last?.claude_cli.model).toBe("opus");
    expect(last?.claude_cli.effort).toBe("xhigh");
  });

  test("Other… takes any model name the plan offers", async ({ page }) => {
    await page.goto("/");
    await openRefine(page);
    await expect(page.getByRole("textbox", { name: "Model name" })).toHaveCount(0);
    await pick(page, "Claude model", "Other…");
    const name = page.getByRole("textbox", { name: "Model name" });
    await name.fill("  claude-fable-5-2 ");
    await name.press("Enter");
    await expect
      .poll(async () =>
        (await recorded<{ claude_cli: { model: string } }[]>(page, "__owRefineSaves"))?.at(-1)
          ?.claude_cli.model,
      )
      .toBe("claude-fable-5-2");
    await expect(page.getByRole("combobox", { name: "Claude model" })).toContainText("Other…");

    // Picking a listed model again hides the name field.
    await pick(page, "Claude model", "Haiku");
    await expect(page.getByRole("textbox", { name: "Model name" })).toHaveCount(0);
    await expect(page.getByTestId("refine-model-note")).toContainText("Fastest");
  });

  test("a model name set outside the app shows as Other…", async ({ page }) => {
    await page.addInitScript(() => {
      (window as unknown as { __owRefineSettings: unknown }).__owRefineSettings = {
        claude_cli: { model: "claude-opus-5-5" },
      };
    });
    await page.goto("/");
    await openRefine(page);
    await expect(page.getByRole("combobox", { name: "Claude model" })).toContainText("Other…");
    await expect(page.getByRole("textbox", { name: "Model name" })).toHaveValue("claude-opus-5-5");
  });

  test("OpenAI-compatible effort is not sent unless chosen", async ({ page }) => {
    await page.addInitScript(() => {
      (window as unknown as { __owRefineSettings: unknown }).__owRefineSettings = {
        backend: "openai-compatible",
        openai: { base_url: "https://api.openai.com/v1", model: "small-model" },
      };
    });
    await page.goto("/");
    await openRefine(page);
    // No Claude model picker for this backend.
    await expect(page.getByRole("combobox", { name: "Claude model" })).toHaveCount(0);
    const effort = page.getByRole("combobox", { name: "Effort" });
    await expect(effort).toContainText("Not sent");

    await pick(page, "Effort", "Minimal");
    await expect
      .poll(async () =>
        (await recorded<{ openai: { effort: string | null; model: string } }[]>(page, "__owRefineSaves"))
          ?.at(-1)?.openai,
      )
      .toMatchObject({ effort: "minimal", model: "small-model" });

    await pick(page, "Effort", "Not sent");
    await expect
      .poll(async () =>
        (await recorded<{ openai: { effort: string | null } }[]>(page, "__owRefineSaves"))?.at(-1)
          ?.openai.effort,
      )
      .toBeNull();
  });

  test("explains spoken instructions and where the line is", async ({ page }) => {
    await page.goto("/");
    await openRefine(page);
    const note = page.getByTestId("refine-spoken-note");
    await expect(note).toContainText("make that more formal");
    await expect(note).toContainText("write this in English");
    await expect(note).toContainText("never answered or carried out");
  });

  test("never use long dashes is on by default, says what it does and saves off and on", async ({
    page,
  }) => {
    await page.goto("/");
    await openRefine(page);
    const dashes = page.getByRole("switch", { name: "Never use long dashes" });
    await expect(dashes).toBeChecked();
    await expect(
      page.getByText(
        "Refine and Ask write a hyphen, a comma or a new sentence instead of an em dash or en dash.",
      ),
    ).toBeVisible();
    // The sentence itself is written without one (U+2014, U+2013).
    const pane = await page.getByRole("tabpanel").innerText();
    expect(pane).not.toMatch(/[\u2013\u2014]/);

    const lastSave = async () =>
      (await recorded<{ no_long_dashes: boolean; enabled: boolean }[]>(page, "__owRefineSaves"))?.at(-1);
    await dashes.click();
    await expect(dashes).not.toBeChecked();
    await expect.poll(async () => (await lastSave())?.no_long_dashes).toBe(false);
    // The switch works with Refine off, and leaves it off.
    expect((await lastSave())?.enabled).toBe(false);

    await dashes.click();
    await expect(dashes).toBeChecked();
    await expect.poll(async () => (await lastSave())?.no_long_dashes).toBe(true);
  });

  test("never use long dashes shows what is stored, and on for a block without it", async ({
    page,
  }) => {
    await page.addInitScript(() => {
      (window as unknown as { __owRefineSettings: unknown }).__owRefineSettings = {
        no_long_dashes: false,
      };
    });
    await page.goto("/");
    await openRefine(page);
    await expect(page.getByRole("switch", { name: "Never use long dashes" })).not.toBeChecked();

    // A block from before the switch existed.
    await page.addInitScript(() => {
      (window as unknown as { __owRefineSettings: unknown }).__owRefineSettings = {
        no_long_dashes: undefined,
      };
    });
    await page.reload();
    await openRefine(page);
    await expect(page.getByRole("switch", { name: "Never use long dashes" })).toBeChecked();
  });

  test("the API key note names the platform's own store", async ({ page }) => {
    await page.addInitScript(() => {
      Object.defineProperty(navigator, "platform", { get: () => "Win32" });
      (window as unknown as { __owRefineSettings: unknown }).__owRefineSettings = {
        backend: "openai-compatible",
      };
    });
    await page.goto("/");
    await openRefine(page);
    const note = page.getByTestId("refine-key-note");
    await expect(note).toContainText("Windows Credential Manager");
    await expect(note).not.toContainText("Keychain");
  });
});

test.describe("home: refine marker on the latest transcript", () => {
  test("refined transcript offers the original", async ({ page }) => {
    await page.goto("/");
    await waitForTickListener(page);
    await emitTick(page, { phase: 3, status: "transcribing", status_message: "refining…" });
    await emitTick(page, {
      phase: 4,
      status: "idle",
      transcript: "I think we should move the meeting with Jeppe to Thursday. ",
      refine: {
        raw_transcript: "so i think we should move the meeting with Jeppe to thursday ",
        status: "refined",
        detail: "",
        backend: "claude-cli",
        elapsed_ms: 780,
      },
    });
    const row = page.getByTestId("home-latest-row");
    await expect(row).toContainText("I think we should move the meeting with Jeppe to Thursday.");
    await expect(page.getByTestId("home-latest-refine")).toContainText("Refined");
    await expect(page.getByTestId("home-latest-original")).toHaveCount(0);

    await page.getByRole("button", { name: "Show original" }).click();
    await expect(page.getByTestId("home-latest-original")).toHaveText(
      "so i think we should move the meeting with Jeppe to thursday",
    );
    await page.getByRole("button", { name: "Hide original" }).click();
    await expect(page.getByTestId("home-latest-original")).toHaveCount(0);
  });

  test("a followed spoken instruction is named, original one click away", async ({ page }) => {
    await page.goto("/");
    await waitForTickListener(page);
    await emitTick(page, { phase: 3, status: "transcribing" });
    await emitTick(page, {
      phase: 4,
      status: "idle",
      transcript: "Hi Emanoil, we're moving the meeting to Friday. ",
      refine: {
        raw_transcript: "hej Emanoil vi rykker mødet til fredag skriv det på engelsk ",
        status: "refined",
        detail: "",
        instruction: "skriv det på engelsk",
        backend: "claude-cli",
        elapsed_ms: 1320,
      },
    });
    await expect(page.getByTestId("home-latest-refine")).toContainText(
      "Refined, as you asked: “skriv det på engelsk”",
    );
    await page.getByRole("button", { name: "Show original" }).click();
    await expect(page.getByTestId("home-latest-original")).toContainText(
      "hej Emanoil vi rykker mødet til fredag skriv det på engelsk",
    );
  });

  test("fallback says the text was typed as transcribed and why", async ({ page }) => {
    await page.goto("/");
    await waitForTickListener(page);
    await emitTick(page, { phase: 3, status: "transcribing" });
    await emitTick(page, {
      phase: 4,
      status: "idle",
      transcript: "so i think we should move the meeting ",
      refine: {
        raw_transcript: "so i think we should move the meeting ",
        status: "timeout",
        detail: "no answer within 5.0 s",
        backend: "claude-cli",
        elapsed_ms: 5003,
      },
    });
    const marker = page.getByTestId("home-latest-refine");
    await expect(marker).toHaveAttribute("data-fallback", "true");
    await expect(marker).toContainText(
      "Not refined, typed as transcribed: no answer within 5.0 s",
    );
    await expect(page.getByRole("button", { name: "Show original" })).toHaveCount(0);
  });

  test("no marker when Refine is off or the utterance was skipped", async ({ page }) => {
    await page.goto("/");
    await waitForTickListener(page);
    await emitTick(page, { phase: 3, status: "transcribing" });
    await emitTick(page, { phase: 4, status: "idle", transcript: "plain text " });
    await expect(page.getByTestId("home-latest-row")).toContainText("plain text");
    await expect(page.getByTestId("home-latest-refine")).toHaveCount(0);

    await emitTick(page, { phase: 3, status: "transcribing" });
    await emitTick(page, {
      phase: 4,
      status: "idle",
      transcript: "okay thanks ",
      refine: {
        raw_transcript: "okay thanks ",
        status: "skipped-short",
        detail: "2 words, fewer than 4: not sent",
        backend: "",
        elapsed_ms: 0,
      },
    });
    await expect(page.getByTestId("home-latest-row")).toContainText("okay thanks");
    await expect(page.getByTestId("home-latest-refine")).toHaveCount(0);
  });
});

test.describe("diagnostics: refine section", () => {
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
    window_5h_used: 0.3,
    window_5h_resets_at: Math.floor(now / 1000) + 3 * 3600,
    window_7d_used: 0.21,
    window_7d_resets_at: Math.floor(now / 1000) + 4 * 86400,
    ...over,
  });
  const HISTORY = {
    summary: {
      total: 5,
      refined: 3,
      skipped: 1,
      fallbacks: 1,
      timeouts: 1,
      unavailable: 0,
      failed: 0,
      rejected: 0,
      instructions: 1,
      median_ms: 1400,
      p90_ms: 2100,
      max_ms: 2100,
      median_warm_ms: 1300,
      median_cold_ms: 2100,
      warm_runs: 2,
      cold_runs: 1,
      last_problem_at_ms: now - 30_000,
      last_problem: "no answer within 5.0 s",
      usage_runs: 4,
      avg_tokens_in: 640,
      avg_tokens_out: 22,
      avg_cost_usd: 0.0005,
      total_cost_usd: 0.002,
      plan: {
        as_of_ms: now - 30_000,
        five_hour_used: 0.3,
        five_hour_resets_at: Math.floor(now / 1000) + 3 * 3600,
        seven_day_used: 0.21,
        seven_day_resets_at: Math.floor(now / 1000) + 4 * 86400,
        window_runs: 4,
        window_moved: 0.2,
        max_share_per_run: 0.05,
        full_at_ms: now + 2 * 3600 * 1000,
        pace_known: true,
      },
    },
    runs: [
      run({ at_ms: now - 30_000, status: "timeout", elapsed_ms: 5003, detail: "no answer within 5.0 s" }),
      run({ instruction: true }),
      run({ elapsed_ms: 2100, warm_lead_ms: null, source: "test" }),
      run({ status: "skipped-short", elapsed_ms: 0, warm_lead_ms: null, detail: "2 words, fewer than 4: not sent" }),
      run({ elapsed_ms: 1300 }),
    ],
  };

  test("says Refine is off and shows no numbers without runs", async ({ page }) => {
    await page.goto("/");
    await openDiagnostics(page);
    await expect(page.getByTestId("diagnostics-refine-empty")).toContainText("Refine is off");
    await expect(page.getByTestId("diagnostics-refine-median")).toHaveCount(0);
    // The connection is still checked, so a broken setup shows before
    // the first dictation.
    await expect(page.getByTestId("diagnostics-refine-connection")).toContainText("Ready");
  });

  test("shows a connection problem with what to do", async ({ page }) => {
    await page.addInitScript(() => {
      (window as unknown as { __owRefineStatus: unknown }).__owRefineStatus = {
        state: "not-signed-in",
        ready: false,
        message:
          "Claude Code is installed but not signed in. Run `claude` in a terminal and sign in.",
      };
    });
    await page.goto("/");
    await openDiagnostics(page);
    const connection = page.getByTestId("diagnostics-refine-connection");
    await expect(connection).toContainText("Not ready");
    await expect(connection).toContainText("installed but not signed in");
  });

  test("shows response times, outcomes and the recent runs", async ({ page }) => {
    await page.addInitScript((history) => {
      const w = window as unknown as Record<string, unknown>;
      w.__owRefineHistory = history;
      w.__owRefineSettings = { enabled: true };
    }, HISTORY);
    await page.goto("/");
    await openDiagnostics(page);

    await expect(page.getByTestId("diagnostics-refine-median")).toHaveText("1.40 s");
    await expect(page.getByTestId("diagnostics-refine-p90")).toHaveText("2.10 s");
    // 5 runs, 1 too short to send: 3 of 4 refined.
    await expect(page.getByTestId("diagnostics-refine-refined")).toHaveText("3 of 4");
    await expect(page.getByTestId("diagnostics-refine-fallbacks")).toHaveText("1");
    await expect(page.getByTestId("diagnostics-refine")).toContainText("1 timed out");
    await expect(page.getByTestId("diagnostics-refine-start")).toContainText(
      "Model started while you talked: 2 runs, median 1.30 s",
    );
    await expect(page.getByTestId("diagnostics-refine-start")).toContainText(
      "started only when the text was ready: 1 run, median 2.10 s",
    );
    await expect(page.getByTestId("diagnostics-refine-last-problem")).toContainText(
      "no answer within 5.0 s",
    );

    const rows = page.getByRole("table", { name: "Recent Refine runs" }).locator("tbody tr");
    await expect(rows).toHaveCount(5);
    await expect(rows.nth(0)).toContainText("Timed out");
    await expect(rows.nth(0)).toContainText("5.00 s");
    await expect(rows.nth(1)).toContainText("followed a spoken instruction");
    await expect(rows.nth(1)).toContainText("4.20 s ahead");
    await expect(rows.nth(2)).toContainText("cold");
    await expect(rows.nth(2)).toContainText("connection test");
    await expect(rows.nth(3)).toContainText("Too short");
    await expect(rows.nth(1)).toContainText("sonnet · low");
  });

  test("shows tokens per dictation, list price and where the plan stands", async ({ page }) => {
    await page.addInitScript((history) => {
      (window as unknown as Record<string, unknown>).__owRefineHistory = history;
    }, HISTORY);
    await page.goto("/");
    await openDiagnostics(page);

    await expect(page.getByTestId("diagnostics-refine-tokens")).toHaveText("640 → 22");
    await expect(page.getByTestId("diagnostics-refine-cost")).toHaveText("$0.0005");
    await expect(page.getByTestId("diagnostics-refine-usage")).toContainText("Not billed on a plan");
    await expect(page.getByTestId("diagnostics-refine-window-5h")).toHaveText("30%");
    await expect(page.getByTestId("diagnostics-refine-window-7d")).toHaveText("21%");
    // The share is an upper bound and says what it includes.
    const share = page.getByTestId("diagnostics-refine-share");
    await expect(share).toContainText("at most 5.0% of the 5-hour window");
    await expect(share).toContainText("moved 20 points across 4 runs");
    await expect(share).toContainText("everything else you did with Claude");
    await expect(page.getByTestId("diagnostics-refine-projection")).toContainText(
      "the 5-hour window is full around",
    );
    // Per-run tokens in the table.
    const rows = page.getByRole("table", { name: "Recent Refine runs" }).locator("tbody tr");
    await expect(rows.nth(1)).toContainText("640 → 22");
  });

  test("says so when a dictation is too small to move the window", async ({ page }) => {
    await page.addInitScript((history) => {
      const h = JSON.parse(JSON.stringify(history));
      h.summary.plan = {
        ...h.summary.plan,
        window_moved: null,
        max_share_per_run: null,
        full_at_ms: null,
        pace_known: false,
      };
      (window as unknown as Record<string, unknown>).__owRefineHistory = h;
    }, HISTORY);
    await page.goto("/");
    await openDiagnostics(page);
    await expect(page.getByTestId("diagnostics-refine-share")).toContainText(
      "too small to see: 4 runs in this window without it moving a single point",
    );
    await expect(page.getByTestId("diagnostics-refine-projection")).toHaveCount(0);
  });

  test("no usage block for a backend that reports none", async ({ page }) => {
    await page.addInitScript((history) => {
      const h = JSON.parse(JSON.stringify(history));
      h.summary = {
        ...h.summary,
        usage_runs: 0,
        avg_tokens_in: null,
        avg_tokens_out: null,
        avg_cost_usd: null,
        total_cost_usd: null,
        plan: null,
      };
      (window as unknown as Record<string, unknown>).__owRefineHistory = h;
    }, HISTORY);
    await page.goto("/");
    await openDiagnostics(page);
    await expect(page.getByTestId("diagnostics-refine-median")).toBeVisible();
    await expect(page.getByTestId("diagnostics-refine-usage")).toHaveCount(0);
  });

  test("Test now runs the connection test; Clear empties the history", async ({ page }) => {
    await page.addInitScript((history) => {
      (window as unknown as Record<string, unknown>).__owRefineHistory = history;
    }, HISTORY);
    await page.goto("/");
    await openDiagnostics(page);

    await page.getByRole("button", { name: "Test now" }).click();
    await expect(page.getByTestId("diagnostics-refine-test")).toContainText(
      "Test answered in 1.18 s",
    );
    expect(await recorded<number>(page, "__owRefineTestCount")).toBe(1);

    await page.getByRole("button", { name: "Clear" }).click();
    await expect(page.getByTestId("diagnostics-refine-empty")).toBeVisible();
    expect(await recorded<number>(page, "__owRefineHistoryClearCount")).toBe(1);
  });
});
