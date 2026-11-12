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
  input_cut: false,
  timeout_s: 120,
};

const READ_DONE = step({ id: "s1", state: "done", outcome: "3 messages" });

const steps = (page: Page) => page.getByTestId("answer-step");
const confirmCard = (page: Page) => page.getByTestId("answer-confirm");

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
    await expect(page.getByTestId("answer-confirm-cut")).toHaveCount(0);
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
    await page.keyboard.press("Enter");
    await page.keyboard.press("Space");
    await page.waitForTimeout(150);
    expect(await confirmCalls(page)).toEqual([]);
    await expect(confirmCard(page)).toBeVisible();
  });

  test("an input that was cut says so plainly", async ({ page }) => {
    await open(page);
    await confirming(page, {
      confirm: { ...CONFIRM, input: '{\n  "text": "' + "a".repeat(300), input_cut: true },
    });
    await expect(page.getByTestId("answer-confirm-cut")).toHaveText(
      "The input is longer than what is shown. You would allow something you cannot see in full.",
    );
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
      confirm: { ...CONFIRM, input: `{\n${lines.join("\n")}\n}`, input_cut: true },
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
    for (const id of ["answer-confirm-title", "answer-confirm-what", "answer-confirm-timeout", "answer-confirm-cut"]) {
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
});
