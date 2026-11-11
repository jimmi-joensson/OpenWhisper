import {
  answerTest as test,
  emitAskTick,
  expect,
  waitForAskTickListener,
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
