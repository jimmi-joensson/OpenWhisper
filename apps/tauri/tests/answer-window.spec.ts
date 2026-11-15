import {
  answerTest as test,
  emitAskTick,
  expect,
  waitForAskTickListener,
  type MockAskConfirm,
  type MockAskStep,
} from "./fixtures/tauri-shim";

type Page = import("@playwright/test").Page;

// The answer window of Ask (TASK-97.5): same Vite bundle, window
// label "answer". main.tsx routes the label to AnswerPanel, which
// shows core's ask snapshot (`ask_snapshot` on mount, then every
// `ask_tick`) and whose buttons are commands.
//
// The real window is 420 x 360 logical points and transparent.
const WINDOW = { width: 420, height: 360 };
test.use({ viewport: WINDOW });

// The commands the panel called, in order (recorded by the shim).
async function calls(page: Page): Promise<string[]> {
  return page.evaluate(() =>
    (
      (window as unknown as { __owAskCalls?: { cmd: string }[] }).__owAskCalls ?? []
    ).map((c) => c.cmd),
  );
}

// The same, each with the arguments it was called with. Without the
// `ask_snapshot` the panel reads when it opens.
async function callsWithArgs(
  page: Page,
): Promise<{ cmd: string; args: Record<string, unknown> }[]> {
  return page.evaluate(() =>
    (
      (window as unknown as {
        __owAskCalls?: { cmd: string; args: Record<string, unknown> }[];
      }).__owAskCalls ?? []
    ).filter((c) => c.cmd !== "ask_snapshot"),
  );
}

function recorded<T>(page: Page, key: string): Promise<T | undefined> {
  return page.evaluate(
    (k) => (window as unknown as Record<string, unknown>)[k] as T | undefined,
    key,
  );
}

async function open(page: Page) {
  await page.goto("/");
  await waitForAskTickListener(page);
}

const card = (page: Page) => page.getByTestId("answer-card");
const button = (page: Page, name: string) =>
  card(page).getByRole("button", { name, exact: true });

test.describe("answer window", () => {
  test("the answer label loads the panel, not the main window or the pill", async ({
    page,
  }) => {
    await page.goto("/");
    await expect(page.getByTestId("answer-panel")).toHaveAttribute("data-phase", "idle");
    await expect(page.getByTestId("sidebar-item-home")).toHaveCount(0);
    await expect(page.locator(".pill-root")).toHaveCount(0);
    // No ask, no card.
    await expect(card(page)).toHaveCount(0);

    // The page is see-through, like the pill's: the app background
    // of the main window's stylesheet does not show here.
    const backgrounds = await page.evaluate(() =>
      [document.documentElement, document.body, document.getElementById("root")!].map(
        (el) => getComputedStyle(el).backgroundColor,
      ),
    );
    expect(backgrounds).toEqual(["rgba(0, 0, 0, 0)", "rgba(0, 0, 0, 0)", "rgba(0, 0, 0, 0)"]);
  });

  test("every phase renders from ask_tick, and the answer grows", async ({ page }) => {
    await open(page);

    await emitAskTick(page, { phase: "listening" });
    await expect(page.getByTestId("answer-panel")).toHaveAttribute("data-phase", "listening");
    await expect(page.getByTestId("answer-status")).toHaveText("Listening");
    await expect(page.getByTestId("answer-question")).toHaveCount(0);
    await expect(page.getByTestId("answer-source")).toHaveText("Will be sent to Claude (Sonnet)");
    await expect(button(page, "Cancel")).toBeVisible();

    await emitAskTick(page, { phase: "thinking", question: "what is a tap" });
    await expect(page.getByTestId("answer-panel")).toHaveAttribute("data-phase", "thinking");
    await expect(page.getByTestId("answer-status")).toHaveText("Thinking");
    await expect(page.getByTestId("answer-question")).toHaveText("what is a tap");
    await expect(page.getByTestId("answer-source")).toHaveText("Sent to Claude (Sonnet)");
    await expect(page.getByTestId("answer-text")).toHaveCount(0);
    await expect(button(page, "Cancel")).toBeVisible();

    await emitAskTick(page, { phase: "answering", question: "what is a tap", answer: "A tap is" });
    await expect(page.getByTestId("answer-text")).toHaveText("A tap is");
    await expect(page.getByTestId("answer-status")).toHaveCount(0);
    await emitAskTick(page, {
      phase: "answering",
      question: "what is a tap",
      answer: "A tap is a short",
    });
    await expect(page.getByTestId("answer-text")).toHaveText("A tap is a short");
    // Still running: it can be cancelled and copied, not inserted.
    await expect(button(page, "Cancel")).toBeVisible();
    await expect(button(page, "Copy")).toBeVisible();
    await expect(button(page, "Insert")).toHaveCount(0);
    await expect(button(page, "Dismiss")).toHaveCount(0);

    await emitAskTick(page, {
      phase: "done",
      question: "what is a tap",
      answer: "A tap is a short press.",
    });
    await expect(page.getByTestId("answer-panel")).toHaveAttribute("data-phase", "done");
    await expect(page.getByTestId("answer-text")).toHaveText("A tap is a short press.");
    await expect(page.getByTestId("answer-question")).toHaveText("what is a tap");
    await expect(button(page, "Copy")).toBeVisible();
    await expect(button(page, "Insert")).toBeVisible();
    await expect(button(page, "Dismiss")).toBeVisible();
    await expect(button(page, "Cancel")).toHaveCount(0);

    // An older tick that arrives late does not take the answer back.
    await emitAskTick(page, { phase: "answering", answer: "A tap", version: 1 });
    await expect(page.getByTestId("answer-text")).toHaveText("A tap is a short press.");

    // Back to idle (the shell hides the window): nothing is left.
    // (The late tick above reset the shim's version counter.)
    await emitAskTick(page, { phase: "idle", id: 0, version: 100 });
    await expect(card(page)).toHaveCount(0);
  });

  test("a panel that opens mid-ask starts from the snapshot", async ({ page }) => {
    await page.addInitScript(() => {
      (window as unknown as { __owAskSnapshot?: unknown }).__owAskSnapshot = {
        id: 2,
        phase: "done",
        question: "what is a tap",
        answer: "A short press.",
        notice: "",
        error: "",
        backend: "claude-cli",
        model: "sonnet",
        effort: "low",
        web_search: false,
        searching: false,
        search_query: "",
        started_at_ms: 0,
        elapsed_ms: 900,
        first_text_ms: 400,
        usage: null,
        version: 9,
      };
    });
    await page.goto("/");
    await expect(page.getByTestId("answer-panel")).toHaveAttribute("data-phase", "done");
    await expect(page.getByTestId("answer-text")).toHaveText("A short press.");
    await expect(button(page, "Insert")).toBeVisible();
  });

  test("searching the web shows what is being looked up", async ({ page }) => {
    await open(page);
    await emitAskTick(page, {
      phase: "thinking",
      question: "when does the ferry to the island leave",
      web_search: true,
      searching: true,
      search_query: "island ferry timetable",
    });
    await expect(page.getByTestId("answer-status")).toHaveText(
      "Searching the web for “island ferry timetable”",
    );
    await expect(page.getByTestId("answer-source")).toHaveText(
      "Sent to Claude (Sonnet), web search on",
    );

    // No query yet: the plain words.
    await emitAskTick(page, { phase: "thinking", web_search: true, searching: true });
    await expect(page.getByTestId("answer-status")).toHaveText("Searching the web");

    // A search in the middle of an answer shows under the text.
    await emitAskTick(page, {
      phase: "answering",
      answer: "The first ferry",
      web_search: true,
      searching: true,
      search_query: "first departure",
    });
    await expect(page.getByTestId("answer-text")).toHaveText("The first ferry");
    await expect(page.getByTestId("answer-status")).toHaveText(
      "Searching the web for “first departure”",
    );

    // Search over: back to thinking.
    await emitAskTick(page, { phase: "thinking", web_search: true });
    await expect(page.getByTestId("answer-status")).toHaveText("Thinking");
  });

  test("the footer says where the question went, and shows a notice", async ({ page }) => {
    await open(page);
    await emitAskTick(page, {
      phase: "done",
      question: "what is a tap",
      answer: "A short press.",
      backend: "openai-compatible",
      model: "small-model",
      notice:
        "Web search is only available with the Claude backend. This answer comes from the model alone.",
    });
    await expect(page.getByTestId("answer-source")).toHaveText(
      "Sent to your OpenAI-compatible server (small-model)",
    );
    await expect(page.getByTestId("answer-notice")).toHaveText(
      "Web search is only available with the Claude backend. This answer comes from the model alone.",
    );

    // A Claude model outside the named ones is shown as it is.
    await emitAskTick(page, {
      phase: "done",
      answer: "A short press.",
      model: "claude-test-model",
    });
    await expect(page.getByTestId("answer-source")).toHaveText(
      "Sent to Claude (claude-test-model)",
    );
    await expect(page.getByTestId("answer-notice")).toHaveCount(0);
  });

  test("an answer that needed the web says web search is off and leads to the setting", async ({
    page,
  }) => {
    await open(page);
    const suggestion = page.getByTestId("answer-suggestion");
    const link = button(page, "Turn on web search");

    // While the answer is still arriving.
    await emitAskTick(page, {
      phase: "answering",
      question: "how will the weather be today",
      answer: "I cannot check",
      suggestion: "web-search",
    });
    await expect(suggestion).toHaveText(
      "Web search is off, so this answer could not look anything up. Turn on web search",
    );
    await expect(link).toBeVisible();

    // And once it is complete, under the answer and inside the window.
    await emitAskTick(page, {
      phase: "done",
      question: "how will the weather be today",
      answer: "I cannot check the weather from here.",
      suggestion: "web-search",
    });
    await expect(suggestion).toBeVisible();
    await expect(link).toBeInViewport({ ratio: 1 });
    const [answerBox, suggestionBox, sourceBox] = await Promise.all([
      page.getByTestId("answer-text").boundingBox(),
      suggestion.boundingBox(),
      page.getByTestId("answer-source").boundingBox(),
    ]);
    expect(suggestionBox!.y).toBeGreaterThanOrEqual(answerBox!.y + answerBox!.height);
    expect(sourceBox!.y).toBeGreaterThanOrEqual(suggestionBox!.y + suggestionBox!.height);
    // The same quiet line as a notice: small, muted text.
    const style = await suggestion.evaluate((el) => {
      const s = getComputedStyle(el);
      return { size: s.fontSize, color: s.color };
    });
    const sourceStyle = await page.getByTestId("answer-source").evaluate((el) => {
      const s = getComputedStyle(el);
      return { size: s.fontSize, color: s.color };
    });
    expect(style).toEqual(sourceStyle);
    // A link by its look without a hover: this window is never active.
    expect(await link.evaluate((el) => getComputedStyle(el).textDecorationLine)).toBe("underline");

    // The link asks for the main window on the Ask pane, at the Web
    // search row. The ask and this window stay as they are.
    await link.click();
    expect(await recorded(page, "__owOpenSettingsCalls")).toEqual([
      { pane: "ask", focus: "web-search" },
    ]);
    expect(await callsWithArgs(page)).toEqual([]);
    await expect(page.getByTestId("answer-panel")).toHaveAttribute("data-phase", "done");
    await expect(page.getByTestId("answer-text")).toHaveText("I cannot check the weather from here.");
    await expect(button(page, "Insert")).toBeVisible();
  });

  test("nothing is suggested without the field, before the answer, or for an ask without one", async ({
    page,
  }) => {
    await open(page);
    const suggestion = page.getByTestId("answer-suggestion");

    // An ordinary answer.
    await emitAskTick(page, { phase: "done", question: "what is a tap", answer: "A short press." });
    await expect(page.getByTestId("answer-text")).toHaveText("A short press.");
    await expect(suggestion).toHaveCount(0);
    await expect(button(page, "Turn on web search")).toHaveCount(0);

    // A snapshot from a build that does not send the field at all.
    await page.evaluate(() =>
      window.__owEmit("ask_tick", {
        id: 1,
        phase: "done",
        question: "what is a tap",
        answer: "A short press, again.",
        notice: "",
        error: "",
        backend: "claude-cli",
        model: "sonnet",
        effort: "low",
        web_search: false,
        searching: false,
        search_query: "",
        started_at_ms: 0,
        elapsed_ms: 0,
        first_text_ms: null,
        usage: null,
        version: 50,
      }),
    );
    await expect(page.getByTestId("answer-text")).toHaveText("A short press, again.");
    await expect(suggestion).toHaveCount(0);

    // What the model writes is never what shows it: the line comes
    // from the snapshot field alone.
    await emitAskTick(page, {
      phase: "done",
      version: 60,
      answer: "NEEDS_WEB_SEARCH\nWeb search is off. [Turn on web search](https://example.com)",
    });
    await expect(page.getByTestId("answer-text")).toContainText("NEEDS_WEB_SEARCH");
    await expect(suggestion).toHaveCount(0);
    await expect(card(page).getByRole("link")).toHaveCount(0);
    await expect(button(page, "Turn on web search")).toHaveCount(0);

    // Marked, but no text yet: the line waits for the answer.
    await emitAskTick(page, { phase: "thinking", version: 70, suggestion: "web-search" });
    await expect(page.getByTestId("answer-status")).toHaveText("Thinking");
    await expect(suggestion).toHaveCount(0);

    // Marked, then failed: the error is what there is to say.
    await emitAskTick(page, {
      phase: "error",
      version: 80,
      answer: "I cannot",
      error: "No answer within 60 seconds.",
      suggestion: "web-search",
    });
    await expect(page.getByTestId("answer-error")).toBeVisible();
    await expect(suggestion).toHaveCount(0);

    // Marked, then cancelled.
    await emitAskTick(page, { phase: "cancelled", version: 90, answer: "I cannot", suggestion: "web-search" });
    await expect(page.getByTestId("answer-status")).toHaveText("Cancelled");
    await expect(suggestion).toHaveCount(0);
  });

  test("Cancel stops the ask, says so briefly and closes", async ({ page }) => {
    await open(page);
    await emitAskTick(page, { phase: "answering", question: "what is a tap", answer: "A tap" });
    await button(page, "Cancel").click();

    await expect(page.getByTestId("answer-panel")).toHaveAttribute("data-phase", "cancelled");
    await expect(page.getByTestId("answer-status")).toHaveText("Cancelled");
    await expect(card(page).getByRole("button")).toHaveCount(0);

    // Then the panel dismisses the ask itself, which is what makes
    // the shell hide the window.
    await expect(page.getByTestId("answer-panel")).toHaveAttribute("data-phase", "idle");
    await expect(card(page)).toHaveCount(0);
    expect((await calls(page)).filter((c) => c !== "ask_snapshot")).toEqual([
      "ask_cancel",
      "ask_dismiss",
    ]);
  });

  test("every command names the ask the panel is showing", async ({ page }) => {
    await open(page);

    // Cancel, and the close the panel sends by itself afterwards.
    await emitAskTick(page, { id: 7, phase: "answering", question: "what is a tap", answer: "A" });
    await button(page, "Cancel").click();
    await expect(card(page)).toHaveCount(0);
    expect(await callsWithArgs(page)).toEqual([
      { cmd: "ask_cancel", args: { id: 7 } },
      { cmd: "ask_dismiss", args: { id: 7 } },
    ]);

    // Insert.
    await emitAskTick(page, { id: 8, phase: "done", question: "what is a tap", answer: "A press." });
    await button(page, "Insert").click();
    await expect(card(page)).toHaveCount(0);
    expect((await callsWithArgs(page)).at(-1)).toEqual({ cmd: "ask_insert", args: { id: 8 } });

    // Dismiss.
    await emitAskTick(page, { id: 9, phase: "done", question: "what is a tap", answer: "A press." });
    await button(page, "Dismiss").click();
    await expect(card(page)).toHaveCount(0);
    expect((await callsWithArgs(page)).at(-1)).toEqual({ cmd: "ask_dismiss", args: { id: 9 } });
  });

  test("the close of a cancelled ask leaves an ask that started meanwhile alone", async ({
    page,
  }) => {
    await open(page);
    // Ask 7 was cancelled; the panel says so and will close it.
    await emitAskTick(page, { id: 7, phase: "cancelled", question: "what is a tap" });
    await expect(page.getByTestId("answer-status")).toHaveText("Cancelled");

    // Before it does, the next ask starts recording. Its tick has
    // not reached the panel yet: only Rust knows about it.
    await page.evaluate(() => {
      const w = window as unknown as { __owAskSnapshot: Record<string, unknown> };
      w.__owAskSnapshot = { ...w.__owAskSnapshot, id: 8, phase: "listening", question: "" };
    });

    // The close goes out for ask 7, by name.
    await expect
      .poll(async () => (await callsWithArgs(page)).at(-1))
      .toEqual({ cmd: "ask_dismiss", args: { id: 7 } });
    // So ask 8 is still there, and shows when its tick arrives.
    const current = await recorded<{ id: number; phase: string }>(page, "__owAskSnapshot");
    expect([current?.id, current?.phase]).toEqual([8, "listening"]);
    await emitAskTick(page, { id: 8, phase: "listening" });
    await expect(page.getByTestId("answer-status")).toHaveText("Listening");
    await expect(button(page, "Cancel")).toBeVisible();
  });

  test("an ask cancelled from outside closes the same way", async ({ page }) => {
    await open(page);
    // The cancel hotkey: the phase arrives as a tick.
    await emitAskTick(page, { phase: "cancelled", question: "what is a tap" });
    await expect(page.getByTestId("answer-status")).toHaveText("Cancelled");
    await expect(card(page)).toHaveCount(0);
    expect(await calls(page)).toContain("ask_dismiss");
  });

  test("Cancel while listening drops the ask", async ({ page }) => {
    await open(page);
    await emitAskTick(page, { phase: "listening" });
    await button(page, "Cancel").click();
    await expect(card(page)).toHaveCount(0);
    expect(await calls(page)).toContain("ask_cancel");
  });

  test("Copy copies what has arrived and says Copied for a moment", async ({ page }) => {
    await open(page);
    await emitAskTick(page, { phase: "answering", question: "what is a tap", answer: "A tap is" });
    await button(page, "Copy").click();
    await expect(button(page, "Copied")).toBeVisible();
    expect(await recorded<string[]>(page, "__owAskCopied")).toEqual(["A tap is"]);
    // Back to Copy by itself.
    await expect(button(page, "Copy")).toBeVisible();

    await emitAskTick(page, {
      phase: "done",
      question: "what is a tap",
      answer: "A tap is a short press.",
    });
    await button(page, "Copy").click();
    await expect(button(page, "Copied")).toBeVisible();
    expect(await recorded<string[]>(page, "__owAskCopied")).toEqual([
      "A tap is",
      "A tap is a short press.",
    ]);
    // Copying leaves the answer where it is.
    await expect(page.getByTestId("answer-text")).toHaveText("A tap is a short press.");
  });

  test("Insert hands over the finished answer and closes", async ({ page }) => {
    await open(page);
    await emitAskTick(page, { phase: "thinking", question: "what is a tap" });
    await expect(button(page, "Insert")).toHaveCount(0);
    await emitAskTick(page, { phase: "answering", question: "what is a tap", answer: "A tap" });
    await expect(button(page, "Insert")).toHaveCount(0);

    await emitAskTick(page, {
      phase: "done",
      question: "what is a tap",
      answer: "A tap is a short press.",
    });
    await button(page, "Insert").click();
    await expect(card(page)).toHaveCount(0);
    expect(await recorded<string[]>(page, "__owAskInserted")).toEqual([
      "A tap is a short press.",
    ]);
    expect(await calls(page)).toContain("ask_insert");
  });

  test("Dismiss closes the ask", async ({ page }) => {
    await open(page);
    await emitAskTick(page, { phase: "done", question: "what is a tap", answer: "A short press." });
    await button(page, "Dismiss").click();
    await expect(card(page)).toHaveCount(0);
    expect(await calls(page)).toContain("ask_dismiss");
    expect(await recorded<string[]>(page, "__owAskInserted")).toBeUndefined();
  });

  test("an error shows its message and can only be dismissed", async ({ page }) => {
    await open(page);
    await emitAskTick(page, {
      phase: "error",
      question: "what is a tap",
      error: "No answer within 60 seconds. Try again, or pick a faster model.",
    });
    await expect(page.getByTestId("answer-panel")).toHaveAttribute("data-phase", "error");
    await expect(page.getByTestId("answer-error")).toContainText(
      "No answer within 60 seconds. Try again, or pick a faster model.",
    );
    await expect(page.getByTestId("answer-question")).toHaveText("what is a tap");
    await expect(button(page, "Insert")).toHaveCount(0);
    await expect(button(page, "Cancel")).toHaveCount(0);
    await expect(button(page, "Copy")).toHaveCount(0);

    await button(page, "Dismiss").click();
    await expect(card(page)).toHaveCount(0);
    expect(await calls(page)).toContain("ask_dismiss");
  });

  test("an ask that failed midway keeps what arrived, to copy", async ({ page }) => {
    await open(page);
    await emitAskTick(page, {
      phase: "error",
      question: "what is a tap",
      answer: "A tap is",
      error: "The connection was lost.",
    });
    await expect(page.getByTestId("answer-text")).toHaveText("A tap is");
    await expect(page.getByTestId("answer-error")).toContainText("The connection was lost.");
    await expect(button(page, "Copy")).toBeVisible();
    await expect(button(page, "Insert")).toHaveCount(0);
  });
});

test.describe("answer window: the answer text", () => {
  async function show(page: Page, answer: string) {
    await open(page);
    await emitAskTick(page, { phase: "done", question: "show me", answer });
    return page.getByTestId("answer-text");
  }

  test("paragraphs and line breaks are kept", async ({ page }) => {
    const text = await show(page, "First line\nsecond line\n\nA new paragraph.");
    await expect(text.locator("p")).toHaveCount(2);
    await expect(text.locator("p").nth(0)).toContainText("First line");
    await expect(text.locator("p").nth(0)).toContainText("second line");
    await expect(text.locator("p").nth(0).locator("br")).toHaveCount(1);
    await expect(text.locator("p").nth(1)).toHaveText("A new paragraph.");
  });

  test("bullet and numbered lists", async ({ page }) => {
    const text = await show(
      page,
      [
        "Pack this:",
        "- a towel",
        "- **sun cream**",
        "  - the small bottle",
        "",
        "Then:",
        "",
        "3. Lock the door",
        "",
        "4. Leave the key with Anna",
      ].join("\n"),
    );
    const bullets = text.locator(":scope > ul");
    await expect(bullets).toHaveCount(1);
    await expect(bullets.locator(":scope > li")).toHaveCount(2);
    await expect(bullets.locator(":scope > li").nth(0)).toHaveText("a towel");
    await expect(bullets.locator(":scope > li").nth(1).locator("strong")).toHaveText("sun cream");
    await expect(bullets.locator(":scope > li").nth(1).locator("ul > li")).toHaveText("the small bottle");

    const numbered = text.locator(":scope > ol");
    await expect(numbered).toHaveCount(1);
    await expect(numbered).toHaveAttribute("start", "3");
    await expect(numbered.locator(":scope > li")).toHaveText(["Lock the door", "Leave the key with Anna"]);
    // The list markers are drawn (Tailwind's reset removes them).
    expect(await bullets.evaluate((el) => getComputedStyle(el).listStyleType)).toBe("disc");
    expect(await numbered.evaluate((el) => getComputedStyle(el).listStyleType)).toBe("decimal");
  });

  test("bold, inline code and a heading", async ({ page }) => {
    const text = await show(
      page,
      "## Short version\n\nPress **Right Option** and run `ow status`. A lone ** stays as it is.",
    );
    await expect(text.locator("p").nth(0).locator("strong")).toHaveText("Short version");
    await expect(text).not.toContainText("##");
    const body = text.locator("p").nth(1);
    await expect(body.locator("strong")).toHaveText("Right Option");
    await expect(body.locator("code")).toHaveText("ow status");
    await expect(body).toContainText("A lone ** stays as it is.");
  });

  test("a code block keeps its lines, also while it is still arriving", async ({ page }) => {
    const text = await show(page, "Run this:\n\n```sh\nmkdir notes\ncd notes\n```\n\nDone.");
    const pre = text.locator("pre");
    await expect(pre).toHaveCount(1);
    expect(await pre.locator("code").evaluate((el) => el.textContent)).toBe(
      "mkdir notes\ncd notes",
    );
    await expect(text).not.toContainText("```");
    await expect(text.locator("p").last()).toHaveText("Done.");

    // No closing fence yet: everything after the opening is code.
    await emitAskTick(page, { phase: "answering", answer: "```\nline one\n**not bold**" });
    expect(await text.locator("pre code").evaluate((el) => el.textContent)).toBe(
      "line one\n**not bold**",
    );
    await expect(text.locator("strong")).toHaveCount(0);
  });

  test("a link stays text", async ({ page }) => {
    const text = await show(
      page,
      "See [the guide](https://example.com/guide) or https://example.com/faq for more.",
    );
    await expect(text).toHaveText(
      "See the guide (https://example.com/guide) or https://example.com/faq for more.",
    );
    await expect(card(page).locator("a")).toHaveCount(0);
  });

  test("markup in an answer is shown as text, never run", async ({ page }) => {
    const text = await show(
      page,
      'Careful: <script>window.__owned = true</script> and <img src=x onerror="window.__owned = true"> <b>not bold</b>',
    );
    await expect(text).toContainText("<script>window.__owned = true</script>");
    await expect(text).toContainText('<img src=x onerror="window.__owned = true">');
    await expect(text).toContainText("<b>not bold</b>");
    await expect(card(page).locator("script, img, b")).toHaveCount(0);
    expect(await recorded<boolean>(page, "__owned")).toBeUndefined();
  });

  test("the answer can be selected with the mouse, the rest cannot", async ({ page }) => {
    const text = await show(page, "A short press.");
    expect(await text.evaluate((el) => getComputedStyle(el).userSelect)).toBe("text");
    expect(await text.locator("p").evaluate((el) => getComputedStyle(el).userSelect)).toBe("text");
    // The page as a whole is not selectable, as in the main window.
    expect(await page.evaluate(() => getComputedStyle(document.body).userSelect)).toBe("none");
  });
});

test.describe("answer window: the card", () => {
  const LONG = Array.from(
    { length: 40 },
    (_, i) => `Paragraph ${i + 1}: a tap is a short press of a key, let go again right away.`,
  ).join("\n\n");

  async function box(page: Page) {
    const b = await card(page).boundingBox();
    if (!b) throw new Error("no card");
    return b;
  }

  // Where the scroll area stands: at the newest text, or not.
  function scroll(page: Page) {
    return page.getByTestId("answer-scroll").evaluate((el) => ({
      top: el.scrollTop,
      overflow: el.scrollHeight - el.clientHeight,
      fromEnd: el.scrollHeight - el.clientHeight - el.scrollTop,
    }));
  }

  test("a short answer sits at the bottom edge, next to the pill", async ({ page }) => {
    await open(page);
    await emitAskTick(page, { phase: "done", question: "what is a tap", answer: "A short press." });
    await expect(button(page, "Insert")).toBeVisible();
    // The entrance (a few pixels of rise) has to finish first.
    await expect.poll(async () => Math.round((await box(page)).y + (await box(page)).height)).toBe(
      WINDOW.height - 12,
    );
    const b = await box(page);
    expect(b.x).toBe(12);
    expect(b.width).toBe(WINDOW.width - 24);
    expect(b.height).toBeLessThan(200);
    expect((await scroll(page)).overflow).toBe(0);
  });

  test("a long answer scrolls and the card stays inside the window", async ({ page }) => {
    await open(page);
    await emitAskTick(page, {
      phase: "done",
      question:
        "what is a tap, and how is it different from holding a key down for a while, and does it matter which key it is, and what happens when two keys are tapped at once",
      answer: LONG,
    });
    await expect(button(page, "Insert")).toBeVisible();
    await expect.poll(async () => Math.round((await box(page)).y)).toBe(12);
    const b = await box(page);
    expect(Math.round(b.y + b.height)).toBe(WINDOW.height - 12);
    expect(b.x).toBe(12);
    expect(b.width).toBe(WINDOW.width - 24);

    // The answer area scrolls; the question and the buttons stay put.
    expect((await scroll(page)).overflow).toBeGreaterThan(100);
    for (const name of ["Dismiss", "Copy", "Insert"]) {
      await expect(button(page, name)).toBeInViewport({ ratio: 1 });
    }
    await expect(page.getByTestId("answer-question")).toBeInViewport({ ratio: 1 });
    // The question is clamped to two lines.
    const question = await page.getByTestId("answer-question").boundingBox();
    expect(question!.height).toBeLessThan(40);
    // Nothing spills out of the page.
    const page_ = await page.evaluate(() => ({
      w: document.documentElement.scrollWidth,
      h: document.documentElement.scrollHeight,
    }));
    expect(page_).toEqual({ w: WINDOW.width, h: WINDOW.height });
  });

  test("the view follows the newest text, unless the person scrolled up", async ({ page }) => {
    await open(page);
    const upTo = (n: number) => LONG.split("\n\n").slice(0, n).join("\n\n");
    await emitAskTick(page, { phase: "answering", question: "what is a tap", answer: upTo(20) });
    await expect.poll(async () => (await scroll(page)).overflow).toBeGreaterThan(100);
    expect((await scroll(page)).fromEnd).toBeLessThanOrEqual(1);

    // More text: still at the end.
    await emitAskTick(page, { phase: "answering", question: "what is a tap", answer: upTo(30) });
    await expect(page.getByTestId("answer-text")).toContainText("Paragraph 30:");
    expect((await scroll(page)).fromEnd).toBeLessThanOrEqual(1);

    // The person scrolls up to read: new text no longer moves the view.
    await page.getByTestId("answer-scroll").hover();
    await page.mouse.wheel(0, -400);
    await expect.poll(async () => (await scroll(page)).fromEnd).toBeGreaterThan(300);
    const held = (await scroll(page)).top;
    await emitAskTick(page, { phase: "answering", question: "what is a tap", answer: upTo(40) });
    await expect(page.getByTestId("answer-text")).toContainText("Paragraph 40:");
    expect((await scroll(page)).top).toBe(held);

    // Back at the end, it follows again.
    await page.mouse.wheel(0, 100000);
    await expect.poll(async () => (await scroll(page)).fromEnd).toBeLessThanOrEqual(1);
    await emitAskTick(page, {
      phase: "done",
      question: "what is a tap",
      answer: `${LONG}\n\nThe end.`,
    });
    await expect(page.getByTestId("answer-text")).toContainText("The end.");
    expect((await scroll(page)).fromEnd).toBeLessThanOrEqual(1);

    // A new ask starts at its own end, wherever the last one was left.
    await page.mouse.wheel(0, -400);
    await expect.poll(async () => (await scroll(page)).fromEnd).toBeGreaterThan(300);
    await emitAskTick(page, { id: 2, phase: "answering", question: "and a hold", answer: upTo(25) });
    await expect(page.getByTestId("answer-question")).toHaveText("and a hold");
    expect((await scroll(page)).fromEnd).toBeLessThanOrEqual(1);
  });

  test("the card is solid, with a border and a shadow that fits the window", async ({ page }) => {
    await open(page);
    await emitAskTick(page, { phase: "done", question: "what is a tap", answer: "A short press." });
    const style = await card(page).evaluate((el) => {
      const s = getComputedStyle(el);
      return { background: s.backgroundColor, shadow: s.boxShadow };
    });
    // Not see-through: an opaque colour (no alpha channel).
    expect(style.background).not.toMatch(/\/|rgba\(/);
    expect(style.background).not.toBe("transparent");
    expect(style.shadow).not.toBe("none");
  });

  test("follows the theme of the main window", async ({ page }) => {
    await page.addInitScript(() => {
      // Only when nothing is stored, so the change below survives.
      if (!localStorage.getItem("openwhisper-theme")) {
        localStorage.setItem("openwhisper-theme", "light");
      }
    });
    await open(page);
    await emitAskTick(page, { phase: "done", question: "what is a tap", answer: "A short press." });
    const theme = () =>
      page.evaluate(() => ({
        light: document.documentElement.classList.contains("light"),
        dark: document.documentElement.classList.contains("dark"),
      }));
    const background = () => card(page).evaluate((el) => getComputedStyle(el).backgroundColor);
    expect(await theme()).toEqual({ light: true, dark: false });
    const light = await background();

    // The theme is changed in the main window while this one stays
    // loaded. The next ask shows in the new theme.
    await page.evaluate(() => localStorage.setItem("openwhisper-theme", "dark"));
    await emitAskTick(page, { id: 2, phase: "thinking", question: "and a hold" });
    await expect.poll(theme).toEqual({ light: false, dark: true });
    expect(await background()).not.toBe(light);
  });

  test("the entrance is a fade and a small rise, and only a fade with reduced motion", async ({
    page,
  }) => {
    await open(page);
    await emitAskTick(page, { phase: "thinking", question: "what is a tap" });
    const animation = () => card(page).evaluate((el) => getComputedStyle(el).animationName);
    expect(await animation()).toBe("answer-card-in");

    await page.emulateMedia({ reducedMotion: "reduce" });
    expect(await animation()).toBe("answer-card-fade");
  });
});

// --- an ask with tools (Agent, TASK-99) ----------------------------------
//
// The steps and the confirm are core's (`AskSnapshot::steps` and
// `confirm`); the panel shows them and answers the confirm with two
// commands. Connections, tools and people are made up.

function step(given: Partial<MockAskStep> & Pick<MockAskStep, "id" | "state">): MockAskStep {
  return {
    connection: "teamchat",
    tool: "read_thread",
    title: "Read thread",
    kind: "read",
    input_summary: 'channel: "general"',
    outcome: "",
    ...given,
  };
}

const SEND: MockAskStep = step({
  id: "s2",
  state: "waiting",
  tool: "send_message",
  title: "Send message",
  kind: "act",
  input_summary: 'to: "Bo", text: "See you at ten."',
});

const CONFIRM: MockAskConfirm = {
  request_id: "r1",
  step_id: "s2",
  connection: "teamchat",
  tool: "send_message",
  title: "Send message",
  input: '{\n  "to": "Bo",\n  "text": "See you at ten."\n}',
  timeout_s: 120,
  // As core raises it: a yes counts 600 ms from now.
  armed_in_ms: 600,
};

const READ_DONE = step({ id: "s1", state: "done", outcome: "3 messages" });

const steps = (page: Page) => page.getByTestId("answer-step");
const confirmCard = (page: Page) => page.getByTestId("answer-confirm");

// The card has been on show long enough: Allow is active.
async function armed(page: Page) {
  await expect(confirmCard(page)).toHaveAttribute("data-armed", "true");
  await expect(button(page, "Allow")).toBeEnabled();
}

// Stop the page's clock, so nothing in it runs out by itself: a card
// stays as it came up until the test moves the time on.
async function stopClock(page: Page) {
  await page.clock.install();
  await page.clock.pauseAt((await page.evaluate(() => Date.now())) + 1000);
}

// A click at a point of the window, the way a hand does it: on
// whatever is there by the time it lands.
async function clickAt(page: Page, at: { x: number; y: number; width: number; height: number }) {
  await page.mouse.click(at.x + at.width / 2, at.y + at.height / 2);
}

// The card has come to rest: its entrance (a fade and a small rise)
// is over, so a box measured now is the box it keeps.
async function atRest(page: Page) {
  await card(page).evaluate((el) => Promise.all(el.getAnimations().map((a) => a.finished)));
}

// The box of `inner` lies inside the box of `outer`.
function inside(
  inner: { x: number; y: number; width: number; height: number } | null,
  outer: { x: number; y: number; width: number; height: number } | null,
): boolean {
  return (
    inner !== null &&
    outer !== null &&
    inner.x >= outer.x - 0.5 &&
    inner.y >= outer.y - 0.5 &&
    inner.x + inner.width <= outer.x + outer.width + 0.5 &&
    inner.y + inner.height <= outer.y + outer.height + 0.5
  );
}

// The two answers to a confirm, as the shim recorded them.
async function confirmCalls(page: Page) {
  return (await callsWithArgs(page)).filter((c) => c.cmd.startsWith("ask_confirm_"));
}

async function confirming(page: Page, given: Partial<Parameters<typeof emitAskTick>[1]> = {}) {
  await emitAskTick(page, {
    id: 7,
    phase: "confirming",
    question: "tell Bo I will be there at ten",
    steps: [READ_DONE, SEND],
    confirm: CONFIRM,
    ...given,
  });
  await expect(confirmCard(page)).toBeVisible();
}

test.describe("answer window: the steps of an ask with tools", () => {
  test("each step shows its tool, its connection, its input on one line and where it stands", async ({
    page,
  }) => {
    await open(page);
    await emitAskTick(page, {
      phase: "thinking",
      question: "what did Anna say",
      steps: [
        step({ id: "a", state: "done", outcome: "3 messages" }),
        step({
          id: "b",
          state: "failed",
          tool: "search_messages",
          title: "Search messages",
          input_summary: 'query: "Anna"',
          outcome: "The connection did not answer.",
        }),
        step({
          id: "c",
          state: "denied",
          tool: "delete_message",
          title: "Delete message",
          kind: "act",
          input_summary: 'id: "m-17"',
          outcome: "You said no.",
        }),
        step({
          id: "d",
          state: "waiting",
          tool: "send_message",
          title: "Send message",
          kind: "act",
          input_summary: 'to: "Anna"',
        }),
        step({ id: "e", state: "running", connection: "tracker", tool: "list_issues", title: "" }),
      ],
    });
    await expect(steps(page)).toHaveCount(5);
    const words = await steps(page).evaluateAll((els) =>
      els.map((el) => [
        el.getAttribute("data-state"),
        el.querySelector('[data-testid="answer-step-title"]')?.textContent,
        el.querySelector('[data-testid="answer-step-connection"]')?.textContent,
        el.querySelector('[data-testid="answer-step-state"]')?.textContent,
      ]),
    );
    expect(words).toEqual([
      ["done", "Read thread", "teamchat", "Done"],
      ["failed", "Search messages", "teamchat", "Failed"],
      ["denied", "Delete message", "teamchat", "Not run"],
      ["waiting", "Send message", "teamchat", "Waiting for you"],
      // No title: the tool's own name.
      ["running", "list_issues", "tracker", "Running"],
    ]);
    await expect(steps(page).nth(0).getByTestId("answer-step-input")).toHaveText(
      'channel: "general"',
    );
    await expect(steps(page).nth(3)).toHaveAttribute("data-kind", "act");
    // Why a call failed or did not run is said; what a call that
    // worked returned is not shown.
    await expect(steps(page).nth(0).getByTestId("answer-step-outcome")).toHaveCount(0);
    await expect(steps(page).nth(1).getByTestId("answer-step-outcome")).toHaveText(
      "The connection did not answer.",
    );
    await expect(steps(page).nth(2).getByTestId("answer-step-outcome")).toHaveText("You said no.");
    // Only the step that runs has a spinner.
    await expect(steps(page).nth(4).locator('[data-slot="spinner"], [role="status"]')).toHaveCount(1);
    await expect(steps(page).nth(0).locator('[data-slot="spinner"], [role="status"]')).toHaveCount(0);
  });

  test("steps show as they happen, above the answer that follows", async ({ page }) => {
    await open(page);
    await emitAskTick(page, { phase: "thinking", question: "what did Anna say" });
    await expect(steps(page)).toHaveCount(0);
    await expect(page.getByTestId("answer-steps")).toHaveCount(0);

    await emitAskTick(page, {
      phase: "thinking",
      question: "what did Anna say",
      steps: [step({ id: "a", state: "running" })],
    });
    await expect(steps(page)).toHaveCount(1);
    await expect(steps(page).nth(0).getByTestId("answer-step-state")).toHaveText("Running");
    await expect(page.getByTestId("answer-status")).toHaveText("Thinking");

    await emitAskTick(page, {
      phase: "answering",
      question: "what did Anna say",
      steps: [step({ id: "a", state: "done", outcome: "3 messages" })],
      answer: "Anna said the release is on Friday.",
    });
    await expect(steps(page).nth(0).getByTestId("answer-step-state")).toHaveText("Done");
    await expect(page.getByTestId("answer-text")).toHaveText("Anna said the release is on Friday.");
    const stepBox = await steps(page).nth(0).boundingBox();
    const textBox = await page.getByTestId("answer-text").boundingBox();
    expect(stepBox && textBox && stepBox.y + stepBox.height <= textBox.y).toBe(true);
  });

  test("many steps scroll inside the card, which stays in the window", async ({ page }) => {
    await open(page);
    await emitAskTick(page, {
      phase: "done",
      question: "go through the channel",
      steps: Array.from({ length: 24 }, (_, i) =>
        step({ id: `s${i}`, state: "done", input_summary: `page: ${i + 1}` }),
      ),
      answer: "Nothing new in the channel.",
    });
    await expect(steps(page)).toHaveCount(24);
    await expect(button(page, "Insert")).toBeVisible();
    const b = await card(page).boundingBox();
    expect(b && b.y >= 0 && b.y + b.height <= WINDOW.height).toBe(true);
    const overflow = await page
      .getByTestId("answer-scroll")
      .evaluate((el) => el.scrollHeight - el.clientHeight);
    expect(overflow).toBeGreaterThan(0);
    // Pinned to the end: the answer is in view.
    await expect(page.getByTestId("answer-text")).toBeInViewport();
  });

  test("what a tool was given is shown as text, never as markup", async ({ page }) => {
    await open(page);
    await emitAskTick(page, {
      phase: "thinking",
      steps: [
        step({
          id: "a",
          state: "failed",
          title: "<b>Read</b>",
          input_summary: '<img src=x onerror="window.__owPwned = true">',
          outcome: "<script>window.__owPwned = true</script>",
        }),
      ],
    });
    await expect(steps(page).nth(0).getByTestId("answer-step-title")).toHaveText("<b>Read</b>");
    await expect(steps(page).nth(0).getByTestId("answer-step-input")).toHaveText(
      '<img src=x onerror="window.__owPwned = true">',
    );
    await expect(steps(page).locator("img, script, b")).toHaveCount(0);
    expect(await recorded(page, "__owPwned")).toBeUndefined();
  });

  test("a long outcome without a place to break wraps inside the card", async ({ page }) => {
    await open(page);
    const outcome = `https://teamchat.example/api/v2/${"errors-".repeat(60)}end`;
    await emitAskTick(page, {
      phase: "thinking",
      question: "what did Anna say",
      steps: [step({ id: "a", state: "failed", outcome })],
    });
    const line = steps(page).nth(0).getByTestId("answer-step-outcome");
    await expect(line).toHaveText(outcome);
    await atRest(page);
    const cardBox = await card(page).boundingBox();
    const box = await line.boundingBox();
    expect(box && cardBox && box.x >= cardBox.x && box.x + box.width <= cardBox.x + cardBox.width).toBe(
      true,
    );
    // On several lines, and nothing to scroll sideways anywhere.
    expect(box && box.height > 32).toBe(true);
    const sideways = await page
      .getByTestId("answer-scroll")
      .evaluate((el) => el.scrollWidth - el.clientWidth);
    expect(sideways).toBe(0);
    expect(await card(page).evaluate((el) => el.scrollWidth - el.clientWidth)).toBe(0);
  });

  test("a step that was running when the ask ended says its result is not known", async ({
    page,
  }) => {
    await open(page);
    await emitAskTick(page, {
      phase: "error",
      question: "tell Bo I will be there at ten",
      error: "No answer within 60 seconds.",
      answer: "(No answer. No answer within 60 seconds.)",
      steps: [
        step({
          id: "a",
          state: "failed",
          tool: "send_message",
          title: "Send message",
          kind: "act",
          outcome: "The ask ended while this was running, so its result is not known.",
        }),
      ],
    });
    await expect(steps(page).nth(0).getByTestId("answer-step-outcome")).toHaveText(
      "The ask ended while this was running, so its result is not known.",
    );
  });

  test("an escaped right-to-left override is shown as the text core wrote", async ({ page }) => {
    await open(page);
    // Core replaces a character that would turn text around with a
    // visible escape before anything reaches this window. What
    // arrives is these eight plain characters.
    const escaped = "\\u{202E}";
    expect(escaped).toHaveLength(8);
    await emitAskTick(page, {
      id: 7,
      phase: "confirming",
      question: "tell Bo I will be there at ten",
      steps: [
        step({
          id: "s1",
          state: "failed",
          title: `Read ${escaped}daerht`,
          connection: `team${escaped}chat`,
          input_summary: `channel: "${escaped}lareneg"`,
          outcome: `No channel called ${escaped}lareneg.`,
        }),
        { ...SEND, title: `Send ${escaped}egassem`, input_summary: `to: "${escaped}oB"` },
      ],
      confirm: {
        ...CONFIRM,
        title: `Send ${escaped}egassem`,
        connection: `team${escaped}chat`,
        input: `{\n  "to": "${escaped}oB",\n  "text": "See you at ten."\n}`,
      },
    });
    await expect(confirmCard(page)).toBeVisible();
    await expect(page.getByTestId("answer-confirm-title")).toHaveText(
      "Allow Send \\u{202E}egassem on team\\u{202E}chat?",
    );
    expect(await page.getByTestId("answer-confirm-input").evaluate((el) => el.textContent)).toBe(
      '{\n  "to": "\\u{202E}oB",\n  "text": "See you at ten."\n}',
    );
    const first = steps(page).nth(0);
    await expect(first.getByTestId("answer-step-title")).toHaveText("Read \\u{202E}daerht");
    await expect(first.getByTestId("answer-step-connection")).toHaveText("team\\u{202E}chat");
    await expect(first.getByTestId("answer-step-input")).toHaveText('channel: "\\u{202E}lareneg"');
    await expect(first.getByTestId("answer-step-outcome")).toHaveText(
      "No channel called \\u{202E}lareneg.",
    );
    // Nothing in the window holds the character itself, and nothing
    // was unescaped on the way.
    const raw = await page.evaluate(() => document.body.textContent?.includes("\u202E"));
    expect(raw).toBe(false);
  });

  test("a raw override, should one ever arrive, cannot reorder the app's own words around it", async ({
    page,
  }) => {
    await open(page);
    // Core never sends this. A shim that did would meet the second
    // layer: text from outside is laid out as a piece of its own.
    const raw = "\u202E";
    await emitAskTick(page, {
      id: 7,
      phase: "confirming",
      question: "tell Bo I will be there at ten",
      steps: [{ ...SEND, title: `Send${raw}`, connection: `team${raw}` }],
      confirm: { ...CONFIRM, title: `Send${raw}`, connection: `team${raw}`, tool: `send${raw}` },
    });
    await expect(confirmCard(page)).toBeVisible();
    // Where the app's own words are on screen, from left to right.
    const order = await page.evaluate(() => {
      const left = (root: Element, word: string): number => {
        const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
        for (let node = walker.nextNode(); node; node = walker.nextNode()) {
          const at = (node.textContent ?? "").indexOf(word);
          if (at < 0) continue;
          const range = document.createRange();
          range.setStart(node, at);
          range.setEnd(node, at + word.length);
          const box = range.getBoundingClientRect();
          return Math.round(box.left + (word.length > 1 ? 0 : box.width / 2));
        }
        throw new Error(`"${word}" is not on the page`);
      };
      const title = document.querySelector('[data-testid="answer-confirm-title"]');
      const what = document.querySelector('[data-testid="answer-confirm-what"]');
      const step = document.querySelector('[data-testid="answer-step"]');
      if (!title || !what || !step) throw new Error("no confirm");
      const name = title.querySelector('[data-testid="answer-confirm-name"]');
      const connection = title.querySelector('[data-testid="answer-confirm-connection"]');
      const tool = what.querySelector('[data-testid="answer-confirm-tool"]');
      if (!name || !connection || !tool) throw new Error("no names");
      return {
        title: [
          left(title, "Allow"),
          Math.round(name.getBoundingClientRect().left),
          left(title, "on"),
          Math.round(connection.getBoundingClientRect().left),
          left(title, "?"),
        ],
        what: [Math.round(tool.getBoundingClientRect().left), left(what, "would run")],
        step: [
          Math.round(
            (step.querySelector('[data-testid="answer-step-title"]') as Element).getBoundingClientRect()
              .left,
          ),
          Math.round(
            (
              step.querySelector('[data-testid="answer-step-connection"]') as Element
            ).getBoundingClientRect().left,
          ),
          left(step, "Waiting for you"),
        ],
        isolated: [name, connection, tool].map((el) => getComputedStyle(el).unicodeBidi),
      };
    });
    const ascending = (xs: number[]) => xs.every((x, i) => i === 0 || x > xs[i - 1]);
    expect(ascending(order.title), `title: ${order.title}`).toBe(true);
    expect(ascending(order.what), `what: ${order.what}`).toBe(true);
    expect(ascending(order.step), `step: ${order.step}`).toBe(true);
    expect(order.isolated).toEqual(["isolate", "isolate", "isolate"]);
    // The buttons are where they always are.
    const deny = await button(page, "Deny").boundingBox();
    const allow = await button(page, "Allow").boundingBox();
    expect(deny && allow && deny.x > allow.x).toBe(true);
  });

  test("an ask without tools shows no steps and no confirm", async ({ page }) => {
    await open(page);
    await emitAskTick(page, { phase: "done", question: "what is a tap", answer: "A short press." });
    await expect(page.getByTestId("answer-steps")).toHaveCount(0);
    await expect(confirmCard(page)).toHaveCount(0);
    await expect(page.getByTestId("answer-context-source")).toHaveCount(0);
  });
});

test.describe("answer window: the confirm", () => {
  test("shows the tool, the connection, the whole input and how long it waits", async ({ page }) => {
    await open(page);
    await confirming(page);
    await expect(page.getByTestId("answer-panel")).toHaveAttribute("data-phase", "confirming");
    await expect(page.getByTestId("answer-confirm-title")).toHaveText(
      "Allow Send message on teamchat?",
    );
    // The tool by its own name too, next to the name it shows.
    await expect(page.getByTestId("answer-confirm-what")).toHaveText(
      "send_message would run with exactly this input:",
    );
    // The input exactly as core gave it, line breaks and all, in a
    // monospace font.
    const input = page.getByTestId("answer-confirm-input");
    expect(await input.evaluate((el) => el.textContent)).toBe(CONFIRM.input);
    expect(await input.evaluate((el) => getComputedStyle(el).whiteSpace)).toBe("pre-wrap");
    expect(await input.evaluate((el) => getComputedStyle(el).fontFamily)).toMatch(/mono/i);
    await expect(page.getByTestId("answer-confirm-timeout")).toHaveText(
      "No answer within 120 seconds counts as no.",
    );
    // The step it is about waits.
    await expect(steps(page).nth(1).getByTestId("answer-step-state")).toHaveText("Waiting for you");
    // The ask can still be stopped; nothing can be inserted yet.
    await expect(button(page, "Cancel")).toBeVisible();
    await expect(button(page, "Insert")).toHaveCount(0);
    // Nothing was answered by showing it.
    expect(await confirmCalls(page)).toEqual([]);
  });

  test("Allow answers with the ask and the request, and the ask goes on", async ({ page }) => {
    await open(page);
    await confirming(page);
    await button(page, "Allow").click();
    await expect(confirmCard(page)).toHaveCount(0);
    expect(await confirmCalls(page)).toEqual([
      { cmd: "ask_confirm_allow", args: { askId: 7, requestId: "r1" } },
    ]);
    await expect(page.getByTestId("answer-panel")).toHaveAttribute("data-phase", "thinking");
    await expect(steps(page).nth(1).getByTestId("answer-step-state")).toHaveText("Running");
  });

  test("Deny answers with the ask and the request, and the step did not run", async ({ page }) => {
    await open(page);
    await confirming(page);
    await button(page, "Deny").click();
    await expect(confirmCard(page)).toHaveCount(0);
    expect(await confirmCalls(page)).toEqual([
      { cmd: "ask_confirm_deny", args: { askId: 7, requestId: "r1" } },
    ]);
    await expect(steps(page).nth(1)).toHaveAttribute("data-state", "denied");
    await expect(steps(page).nth(1).getByTestId("answer-step-state")).toHaveText("Not run");
    await expect(steps(page).nth(1).getByTestId("answer-step-outcome")).toHaveText("You said no.");
  });

  test("Deny is the button that stands out, and no key allows", async ({ page }) => {
    await open(page);
    await confirming(page);
    // Deny is the filled button, Allow the outlined one.
    await expect(button(page, "Deny")).toHaveAttribute("data-variant", "default");
    await expect(button(page, "Allow")).toHaveAttribute("data-variant", "outline");
    // Deny comes first for the keyboard and sits last on screen,
    // where the main button of the panel is.
    const order = await confirmCard(page)
      .getByRole("button")
      .evaluateAll((els) => els.map((el) => el.textContent?.trim()));
    expect(order).toEqual(["Deny", "Allow"]);
    const deny = await button(page, "Deny").boundingBox();
    const allow = await button(page, "Allow").boundingBox();
    expect(deny && allow && deny.x > allow.x).toBe(true);
    // Neither is a submit button, and nothing has the focus: Enter
    // and Space answer nothing.
    for (const name of ["Deny", "Allow"]) {
      await expect(button(page, name)).toHaveAttribute("type", "button");
    }
    expect(await confirmCard(page).locator("form").count()).toBe(0);
    expect(await page.evaluate(() => document.activeElement?.tagName)).toBe("BODY");
    // With Allow active, so a key that reached it would count.
    await armed(page);
    await page.keyboard.press("Enter");
    await page.keyboard.press("Space");
    // A key press that answered would have called a command by the
    // time the press is over (the handler runs with the event). So
    // the answer that follows is the first one there is.
    await expect(confirmCard(page)).toBeVisible();
    await button(page, "Deny").click();
    expect(await confirmCalls(page)).toEqual([
      { cmd: "ask_confirm_deny", args: { askId: 7, requestId: "r1" } },
    ]);
  });

  test("Allow is inactive when the card comes up and active a moment later, Deny at once", async ({
    page,
  }) => {
    await open(page);
    await stopClock(page);
    await confirming(page);
    const allow = button(page, "Allow");
    await expect(confirmCard(page)).toHaveAttribute("data-armed", "false");
    await expect(allow).toBeDisabled();
    await expect(button(page, "Deny")).toBeEnabled();
    const at = await allow.boundingBox();
    if (!at) throw new Error("no Allow button");
    // A click on it does nothing, however often.
    await clickAt(page, at);
    await clickAt(page, at);
    expect(await confirmCalls(page)).toEqual([]);
    // Just short of the moment: still inactive.
    await page.clock.runFor(590);
    await expect(allow).toBeDisabled();
    // Nothing has to arrive for it to become active: no tick is sent
    // here, the card counts by itself.
    await page.clock.runFor(20);
    await expect(confirmCard(page)).toHaveAttribute("data-armed", "true");
    await expect(allow).toBeEnabled();
    await clickAt(page, at);
    expect(await confirmCalls(page)).toEqual([
      { cmd: "ask_confirm_allow", args: { askId: 7, requestId: "r1" } },
    ]);
  });

  test("a confirm that has waited already still starts inactive in this window", async ({
    page,
  }) => {
    await open(page);
    await stopClock(page);
    // A confirm that has been waiting, as far as the card knows.
    await confirming(page, { confirm: { ...CONFIRM, armed_in_ms: 0 } });
    // Still a moment in this window before a yes is taken.
    await expect(button(page, "Allow")).toBeDisabled();
    await page.clock.runFor(610);
    await expect(button(page, "Allow")).toBeEnabled();
  });

  test("Deny works from the first moment", async ({ page }) => {
    await open(page);
    await stopClock(page);
    await confirming(page);
    await expect(button(page, "Allow")).toBeDisabled();
    const at = await button(page, "Deny").boundingBox();
    if (!at) throw new Error("no Deny button");
    await clickAt(page, at);
    expect(await confirmCalls(page)).toEqual([
      { cmd: "ask_confirm_deny", args: { askId: 7, requestId: "r1" } },
    ]);
  });

  // The next confirm of the same ask, as core raises it.
  const SECOND = {
    steps: [
      READ_DONE,
      { ...SEND, state: "running" as const },
      step({
        id: "s3",
        state: "waiting",
        tool: "delete_message",
        title: "Delete message",
        kind: "act",
        input_summary: 'id: "m-17"',
      }),
    ],
    confirm: {
      ...CONFIRM,
      request_id: "r2",
      step_id: "s3",
      tool: "delete_message",
      title: "Delete message",
      input: '{\n  "id": "m-17"\n}',
    },
  };

  test("a double click on Allow does not allow the confirm that comes next", async ({ page }) => {
    await open(page);
    await confirming(page);
    await armed(page);
    const at = await button(page, "Allow").boundingBox();
    if (!at) throw new Error("no Allow button");
    await stopClock(page);
    // The first click of the two allows what the person read.
    await clickAt(page, at);
    await expect(confirmCard(page)).toHaveCount(0);
    // 150 ms later the model asks for the next tool, and the second
    // click lands where Allow was.
    await page.clock.runFor(150);
    await confirming(page, SECOND);
    await expect(confirmCard(page)).toHaveAttribute("data-request", "r2");
    const again = await button(page, "Allow").boundingBox();
    expect(again && Math.abs(again.x - at.x) < 40 && Math.abs(again.y - at.y) < 40).toBe(true);
    await clickAt(page, again ?? at);
    await clickAt(page, at);
    // Nothing reached Rust for the second confirm.
    expect(await confirmCalls(page)).toEqual([
      { cmd: "ask_confirm_allow", args: { askId: 7, requestId: "r1" } },
    ]);
    await expect(confirmCard(page)).toHaveAttribute("data-armed", "false");
    await expect(page.getByTestId("answer-confirm-title")).toHaveText(
      "Allow Delete message on teamchat?",
    );
    // And had it reached Rust, core would have refused it: a yes
    // counts only after the confirm has been on show for a moment.
    const early = await page.evaluate(() =>
      (
        window.__TAURI_INTERNALS__ as {
          invoke: (cmd: string, args: unknown) => Promise<unknown>;
        }
      ).invoke("ask_confirm_allow", { askId: 7, requestId: "r2" }),
    );
    expect(early).toBe(false);
    await expect(confirmCard(page)).toBeVisible();
    // Read, then allowed: this one counts.
    await page.clock.runFor(700);
    await armed(page);
    await clickAt(page, again ?? at);
    expect((await confirmCalls(page)).at(-1)).toEqual({
      cmd: "ask_confirm_allow",
      args: { askId: 7, requestId: "r2" },
    });
    await expect(confirmCard(page)).toHaveCount(0);
  });

  test("a confirm that takes the place of another starts inactive, and a click meant for the first is not taken", async ({
    page,
  }) => {
    await open(page);
    await confirming(page);
    await armed(page);
    const at = await button(page, "Allow").boundingBox();
    if (!at) throw new Error("no Allow button");
    await stopClock(page);
    // The first one ran out of time and the next came up within the
    // same tick: the card is replaced under the pointer.
    await confirming(page, SECOND);
    await expect(confirmCard(page)).toHaveAttribute("data-request", "r2");
    await expect(confirmCard(page)).toHaveAttribute("data-armed", "false");
    await clickAt(page, at);
    expect(await confirmCalls(page)).toEqual([]);
    await expect(confirmCard(page)).toBeVisible();
    // The new card answers for its own request only.
    await page.clock.runFor(700);
    await armed(page);
    await button(page, "Allow").click();
    expect(await confirmCalls(page)).toEqual([
      { cmd: "ask_confirm_allow", args: { askId: 7, requestId: "r2" } },
    ]);
  });

  test("Deny and Allow sit at opposite ends of the card", async ({ page }) => {
    await open(page);
    await confirming(page);
    await atRest(page);
    const deny = await button(page, "Deny").boundingBox();
    const allow = await button(page, "Allow").boundingBox();
    const whole = await confirmCard(page).boundingBox();
    if (!deny || !allow || !whole) throw new Error("no boxes");
    // More than a third of the card between them, the line about
    // the time in that room.
    expect(deny.x - (allow.x + allow.width)).toBeGreaterThan(whole.width / 3);
    const note = await page.getByTestId("answer-confirm-timeout").boundingBox();
    expect(note && note.x >= allow.x + allow.width && note.x + note.width <= deny.x).toBe(true);
  });

  test("a long input scrolls in itself and the buttons stay inside the window", async ({ page }) => {
    await open(page);
    const lines = Array.from({ length: 60 }, (_, i) => `  "line_${i + 1}": "value ${i + 1}",`);
    await confirming(page, {
      question:
        "tell Bo I will be there at ten and ask whether the room on the second floor is still free then",
      notice: "Web search is not available with this backend.",
      context_source: { kind: "connection", name: "teamchat" },
      steps: [
        READ_DONE,
        step({ id: "x", state: "done", tool: "search_messages", title: "Search messages" }),
        SEND,
      ],
      confirm: { ...CONFIRM, input: `{\n${lines.join("\n")}\n}` },
    });
    const b = await card(page).boundingBox();
    expect(b && b.y >= 0 && b.y + b.height <= WINDOW.height).toBe(true);
    for (const name of ["Deny", "Allow", "Cancel"]) {
      const box = await button(page, name).boundingBox();
      expect(box && box.y >= 0 && box.y + box.height <= WINDOW.height, name).toBe(true);
      await expect(button(page, name)).toBeInViewport({ ratio: 1 });
    }
    // The whole input is there, and it scrolls where it is.
    const input = page.getByTestId("answer-confirm-input");
    await expect(input).toBeInViewport();
    const inner = await input.evaluate((el) => ({
      overflow: el.scrollHeight - el.clientHeight,
      overflowY: getComputedStyle(el).overflowY,
      text: el.textContent ?? "",
    }));
    expect(inner.overflow).toBeGreaterThan(0);
    expect(inner.overflowY).toBe("auto");
    expect(inner.text).toContain('"line_60": "value 60"');
    // Every line of the card is whole: only the input gives way.
    for (const id of ["answer-confirm-title", "answer-confirm-what", "answer-confirm-timeout"]) {
      await expect(page.getByTestId(id)).toBeInViewport({ ratio: 1 });
      const line = await page
        .getByTestId(id)
        .evaluate((el) => ({ shown: el.clientHeight, needed: el.scrollHeight }));
      expect(line.shown, id).toBeGreaterThanOrEqual(line.needed);
    }
    // The notes of the footer step aside while the confirm shows.
    await expect(page.getByTestId("answer-notice")).toHaveCount(0);
    await expect(page.getByTestId("answer-context-source")).toHaveCount(0);
    // The confirm is answered from where it is.
    await button(page, "Deny").click();
    expect(await confirmCalls(page)).toEqual([
      { cmd: "ask_confirm_deny", args: { askId: 7, requestId: "r1" } },
    ]);
  });

  test("the input is text, whatever is in it", async ({ page }) => {
    await open(page);
    await confirming(page, {
      confirm: {
        ...CONFIRM,
        title: "<i>Send</i>",
        input: '{\n  "text": "<img src=x onerror=\\"window.__owPwned = true\\">"\n}',
      },
    });
    await expect(page.getByTestId("answer-confirm-title")).toHaveText(
      "Allow <i>Send</i> on teamchat?",
    );
    await expect(page.getByTestId("answer-confirm-input")).toContainText("<img src=x");
    await expect(confirmCard(page).locator("img, i")).toHaveCount(0);
    expect(await recorded(page, "__owPwned")).toBeUndefined();
  });

  test("a second confirm of the same ask is asked again, with its own request", async ({ page }) => {
    await open(page);
    await confirming(page);
    await button(page, "Deny").click();
    await expect(confirmCard(page)).toHaveCount(0);
    // The model tries another tool that acts.
    await confirming(page, {
      steps: [
        READ_DONE,
        { ...SEND, state: "denied", outcome: "You said no." },
        step({
          id: "s3",
          state: "waiting",
          tool: "delete_message",
          title: "Delete message",
          kind: "act",
          input_summary: 'id: "m-17"',
        }),
      ],
      confirm: {
        ...CONFIRM,
        request_id: "r2",
        step_id: "s3",
        tool: "delete_message",
        title: "Delete message",
        input: '{\n  "id": "m-17"\n}',
      },
    });
    await expect(page.getByTestId("answer-confirm-title")).toHaveText(
      "Allow Delete message on teamchat?",
    );
    await button(page, "Deny").click();
    expect(await confirmCalls(page)).toEqual([
      { cmd: "ask_confirm_deny", args: { askId: 7, requestId: "r1" } },
      { cmd: "ask_confirm_deny", args: { askId: 7, requestId: "r2" } },
    ]);
  });

  test("a short input is shown whole, and the notes of the footer come back after", async ({
    page,
  }) => {
    await open(page);
    await confirming(page, {
      notice: "Web search is not available with this backend.",
      context_source: { kind: "connection", name: "teamchat" },
    });
    // Four lines of input, none of them scrolled away.
    const input = page.getByTestId("answer-confirm-input");
    expect(await input.evaluate((el) => el.scrollHeight - el.clientHeight)).toBe(0);
    await expect(page.getByTestId("answer-notice")).toHaveCount(0);
    await button(page, "Deny").click();
    await expect(page.getByTestId("answer-notice")).toHaveText(
      "Web search is not available with this backend.",
    );
    await expect(page.getByTestId("answer-context-source")).toHaveText(
      "Using the teamchat connection instead of the screen",
    );
  });

  test("a very long input and very long names: everything stays inside the card at 420 x 360", async ({
    page,
  }) => {
    await open(page);
    // 120 characters each, without a place to break.
    const title = "SendAMessageToEveryone".repeat(6).slice(0, 120);
    const tool = "send_message_to_everyone_".repeat(5).slice(0, 120);
    const connection = "teamchat-for-the-whole-company-".repeat(4).slice(0, 120);
    const input = `{\n  "to": "Bo",\n  "text": "${"See you at ten. ".repeat(940).slice(0, 14970)}"\n}`;
    expect(input.length).toBeGreaterThanOrEqual(15000);
    await confirming(page, {
      question:
        "tell Bo I will be there at ten and ask whether the room on the second floor is still free then",
      notice: "Web search is not available with this backend.",
      context_source: { kind: "connection", name: connection },
      answer: "Let me look at the thread first.",
      steps: [READ_DONE, { ...SEND, title, tool, connection }],
      confirm: { ...CONFIRM, title, tool, connection, input },
    });
    await atRest(page);
    const window = { x: 0, y: 0, width: WINDOW.width, height: WINDOW.height };
    const cardBox = await card(page).boundingBox();
    const confirmBox = await confirmCard(page).boundingBox();
    expect(inside(cardBox, window), "the card in the window").toBe(true);
    expect(inside(confirmBox, cardBox), "the confirm in the card").toBe(true);
    // Deny, Allow and the line about the time: whole, inside the
    // confirm, inside the card, inside the window.
    for (const name of ["Deny", "Allow"]) {
      const box = await button(page, name).boundingBox();
      expect(inside(box, confirmBox), `${name} in the confirm`).toBe(true);
      expect(inside(box, cardBox), `${name} in the card`).toBe(true);
      expect(inside(box, window), `${name} in the window`).toBe(true);
      expect(box && box.height >= 24, `${name} is whole`).toBe(true);
    }
    const note = page.getByTestId("answer-confirm-timeout");
    expect(inside(await note.boundingBox(), confirmBox), "the time line in the confirm").toBe(true);
    expect(await note.evaluate((el) => el.scrollHeight <= el.clientHeight)).toBe(true);
    // Cancel is still there.
    expect(inside(await button(page, "Cancel").boundingBox(), cardBox)).toBe(true);
    // The three names are whole: every character, wrapped, nothing
    // cut off at the side or the bottom, nothing ending in an
    // ellipsis, all of it in view without scrolling.
    await expect(page.getByTestId("answer-confirm-name")).toHaveText(title);
    await expect(page.getByTestId("answer-confirm-connection")).toHaveText(connection);
    await expect(page.getByTestId("answer-confirm-tool")).toHaveText(tool);
    for (const id of ["answer-confirm-title", "answer-confirm-what"]) {
      const line = page.getByTestId(id);
      const fits = await line.evaluate((el) => ({
        wide: el.scrollWidth <= el.clientWidth,
        tall: el.scrollHeight <= el.clientHeight,
        lines: el.getClientRects().length,
        overflow: getComputedStyle(el).textOverflow,
      }));
      expect(fits.wide && fits.tall, id).toBe(true);
      expect(fits.overflow, id).not.toBe("ellipsis");
      expect(inside(await line.boundingBox(), confirmBox), `${id} in the confirm`).toBe(true);
      await expect(line).toBeInViewport({ ratio: 1 });
    }
    const names = await page.locator(".answer-confirm-names").evaluate((el) => ({
      hidden: el.scrollHeight - el.clientHeight,
      sideways: el.scrollWidth - el.clientWidth,
    }));
    expect(names).toEqual({ hidden: 0, sideways: 0 });
    // The input takes the room that is left, at least four lines of
    // it, and scrolls where it is, with all of it there.
    const pre = page.getByTestId("answer-confirm-input");
    expect(inside(await pre.boundingBox(), confirmBox), "the input in the confirm").toBe(true);
    const shown = await pre.evaluate((el) => ({
      height: el.clientHeight,
      overflow: el.scrollHeight - el.clientHeight,
      sideways: el.scrollWidth - el.clientWidth,
      text: el.textContent ?? "",
    }));
    expect(shown.height).toBeGreaterThanOrEqual(4 * 16);
    expect(shown.overflow).toBeGreaterThan(0);
    expect(shown.sideways).toBe(0);
    expect(shown.text).toBe(input);
    // Scrolled to its end, the buttons are where they were.
    const before = await button(page, "Deny").boundingBox();
    await pre.evaluate((el) => {
      el.scrollTop = el.scrollHeight;
    });
    expect(await button(page, "Deny").boundingBox()).toEqual(before);
    await button(page, "Deny").click();
    expect(await confirmCalls(page)).toEqual([
      { cmd: "ask_confirm_deny", args: { askId: 7, requestId: "r1" } },
    ]);
  });

  test("names with wide letters and no room at all: the names scroll before a button leaves the card", async ({
    page,
  }) => {
    await open(page);
    const title = "W".repeat(120);
    const tool = "W".repeat(120);
    const connection = "M".repeat(120);
    await confirming(page, {
      steps: [READ_DONE, { ...SEND, title, tool, connection }],
      confirm: { ...CONFIRM, title, tool, connection, input: `{\n  "text": "${"a".repeat(3000)}"\n}` },
    });
    await atRest(page);
    const cardBox = await card(page).boundingBox();
    const confirmBox = await confirmCard(page).boundingBox();
    expect(inside(cardBox, { x: 0, y: 0, width: WINDOW.width, height: WINDOW.height })).toBe(true);
    for (const name of ["Deny", "Allow"]) {
      expect(inside(await button(page, name).boundingBox(), confirmBox), name).toBe(true);
    }
    expect(inside(await page.getByTestId("answer-confirm-timeout").boundingBox(), confirmBox)).toBe(
      true,
    );
    // Still four lines of the input, and the names whole and
    // wrapped, in a part of their own that scrolls.
    const pre = await page.getByTestId("answer-confirm-input").evaluate((el) => el.clientHeight);
    expect(pre).toBeGreaterThanOrEqual(4 * 16);
    const names = await page.locator(".answer-confirm-names").evaluate((el) => ({
      sideways: el.scrollWidth - el.clientWidth,
      overflowY: getComputedStyle(el).overflowY,
      text: el.textContent ?? "",
    }));
    expect(names.sideways).toBe(0);
    expect(names.overflowY).toBe("auto");
    expect(names.text).toContain(title);
    expect(names.text).toContain(connection);
  });

  test("the step that waits stays in view while its card shows, also once the answer has started", async ({
    page,
  }) => {
    await open(page);
    const paragraphs = Array.from(
      { length: 12 },
      (_, i) => `Paragraph ${i + 1} of what the model wrote before it asked.`,
    );
    await confirming(page, { answer: paragraphs.join("\n\n") });
    const waiting = page.locator('[data-testid="answer-step"][data-state="waiting"]');
    await expect(waiting).toHaveCount(1);
    await expect(waiting.getByTestId("answer-step-title")).toHaveText("Send message");
    await expect(waiting.getByTestId("answer-step-state")).toHaveText("Waiting for you");
    await expect(waiting).toBeInViewport({ ratio: 1 });
    // It is not in the part that scrolls, so no scroll position
    // takes it away.
    const scroll = page.getByTestId("answer-scroll");
    expect(await waiting.evaluate((el) => el.closest('[data-testid="answer-scroll"]') === null)).toBe(
      true,
    );
    for (const to of [0, 100000]) {
      await scroll.evaluate((el, top) => {
        el.scrollTop = top;
      }, to);
      await expect(waiting).toBeInViewport({ ratio: 1 });
    }
    // Right above the card that asks about it.
    await atRest(page);
    const stepBox = await waiting.boundingBox();
    const cardBox = await confirmCard(page).boundingBox();
    expect(stepBox && cardBox && stepBox.y + stepBox.height <= cardBox.y + 0.5).toBe(true);
    // Answered: it is one of the steps again.
    await button(page, "Deny").click();
    await expect(page.getByTestId("answer-steps-waiting")).toHaveCount(0);
    await expect(steps(page)).toHaveCount(2);
    await expect(steps(page).nth(1)).toHaveAttribute("data-state", "denied");
  });

  test("a confirm closed because Claude Code stopped says so on the step", async ({ page }) => {
    await open(page);
    await confirming(page);
    // Core: the CLI died while the confirm waited (`gone`). The
    // confirm is closed at once and the step says why, in core's
    // words.
    await emitAskTick(page, {
      id: 7,
      phase: "error",
      question: "tell Bo I will be there at ten",
      error: "Claude Code stopped before it answered.",
      steps: [
        READ_DONE,
        {
          ...SEND,
          state: "denied",
          outcome: "Claude Code stopped before you answered, so it was not run.",
        },
      ],
      confirm: null,
    });
    await expect(confirmCard(page)).toHaveCount(0);
    await expect(steps(page).nth(1).getByTestId("answer-step-state")).toHaveText("Not run");
    await expect(steps(page).nth(1).getByTestId("answer-step-outcome")).toHaveText(
      "Claude Code stopped before you answered, so it was not run.",
    );
    await expect(page.getByTestId("answer-error")).toContainText(
      "Claude Code stopped before it answered.",
    );
    expect(await confirmCalls(page)).toEqual([]);
  });

  test("a confirm that ran out of time closes by itself", async ({ page }) => {
    await open(page);
    await confirming(page);
    // Core: no answer in time is a no, and the ask goes on.
    await emitAskTick(page, {
      id: 7,
      phase: "thinking",
      question: "tell Bo I will be there at ten",
      steps: [
        READ_DONE,
        { ...SEND, state: "denied", outcome: "You did not answer in time, so it was not done." },
      ],
      confirm: null,
    });
    await expect(confirmCard(page)).toHaveCount(0);
    await expect(steps(page).nth(1).getByTestId("answer-step-outcome")).toHaveText(
      "You did not answer in time, so it was not done.",
    );
    expect(await confirmCalls(page)).toEqual([]);
  });

  test("Cancel while a confirm waits stops the ask, and the tool did not run", async ({ page }) => {
    await open(page);
    await confirming(page);
    await button(page, "Cancel").click();
    await expect(page.getByTestId("answer-panel")).toHaveAttribute("data-phase", "cancelled");
    await expect(confirmCard(page)).toHaveCount(0);
    await expect(steps(page).nth(1)).toHaveAttribute("data-state", "denied");
    expect((await callsWithArgs(page)).map((c) => c.cmd)).toEqual(["ask_cancel"]);
    expect((await callsWithArgs(page))[0].args).toEqual({ id: 7 });
  });
});

test.describe("answer window: where the ask looked", () => {
  test("says when a connection was used instead of the screen", async ({ page }) => {
    await open(page);
    await emitAskTick(page, {
      phase: "thinking",
      question: "what did Anna say",
      context_source: { kind: "connection", name: "teamchat" },
    });
    await expect(page.getByTestId("answer-context-source")).toHaveText(
      "Using the teamchat connection instead of the screen",
    );
    await emitAskTick(page, {
      phase: "done",
      question: "what did Anna say",
      answer: "The release is on Friday.",
      context_source: { kind: "connection", name: "teamchat" },
    });
    await expect(page.getByTestId("answer-context-source")).toHaveText(
      "Used the teamchat connection instead of the screen",
    );
    await expect(page.getByTestId("answer-screen-included")).toHaveCount(0);
  });

  test("a screenshot that was asked for as well is said too", async ({ page }) => {
    await open(page);
    await emitAskTick(page, {
      phase: "done",
      question: "look at my screen, what did Anna say",
      answer: "The release is on Friday.",
      screen: "included",
      context_source: { kind: "connection", name: "teamchat" },
    });
    await expect(page.getByTestId("answer-context-source")).toHaveText(
      "Used the teamchat connection",
    );
    await expect(page.getByTestId("answer-screen-included")).toHaveText("Screen included");
  });

  test("an ask that looked at the screen, or nowhere, has no such line", async ({ page }) => {
    await open(page);
    await emitAskTick(page, {
      phase: "done",
      answer: "A file is missing.",
      screen: "included",
      context_source: { kind: "screen" },
    });
    await expect(page.getByTestId("answer-screen-included")).toBeVisible();
    await expect(page.getByTestId("answer-context-source")).toHaveCount(0);
    await emitAskTick(page, { phase: "done", answer: "A short press." });
    await expect(page.getByTestId("answer-context-source")).toHaveCount(0);
  });

  test("what core has to say about the connections shows under the answer, whole", async ({ page }) => {
    await open(page);
    // Core's notices of an agent ask: a connection that was left
    // out, one that was still starting, tools that showed up after
    // the lookup. Each is a sentence or two; they come as one text.
    const notice =
      'The connection "tracker" has not been looked up since the app started, so this answer was made without it. Refresh the connections in the Agent settings to use it. ' +
      'The connection "teamchat" was still starting when this was answered, so the answer may have been made without it. Ask again in a moment.';
    await emitAskTick(page, {
      phase: "done",
      question: "what did Anna say",
      answer: "I could not look that up.",
      notice,
    });
    const line = page.getByTestId("answer-notice");
    await expect(line).toHaveText(notice);
    await atRest(page);
    await expect(line).toBeInViewport({ ratio: 1 });
    const cardBox = await card(page).boundingBox();
    expect(inside(await line.boundingBox(), cardBox)).toBe(true);
    expect(await line.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(true);
    // The buttons are still there.
    await expect(button(page, "Insert")).toBeInViewport({ ratio: 1 });
    // While the ask runs, too.
    await emitAskTick(page, { phase: "thinking", question: "what did Anna say", notice });
    await expect(line).toHaveText(notice);
    // A name without a place to break does not push the card wider.
    await emitAskTick(page, {
      phase: "done",
      question: "what did Anna say",
      answer: "I could not look that up.",
      notice: `The connection "${"teamchat-".repeat(14)}" has not been looked up since the app started.`,
    });
    expect(inside(await line.boundingBox(), await card(page).boundingBox())).toBe(true);
    expect(await card(page).evaluate((el) => el.scrollWidth - el.clientWidth)).toBe(0);
  });
});
