import {
  answerTest as test,
  emitAskTick,
  expect,
  waitForAskTickListener,
} from "./fixtures/tauri-shim";

// The answer window of Ask (TASK-97.4): same Vite bundle, window label
// "answer". These only check the wiring (main.tsx routes the label to
// AnswerPanel, which follows `ask_tick`) against the placeholder
// panel. The panel itself is TASK-97.5; its spec replaces these.
test.describe("answer window", () => {
  test("the answer label loads the panel, not the main window or the pill", async ({
    page,
  }) => {
    await page.goto("/");
    await expect(page.getByTestId("answer-panel")).toBeVisible();
    await expect(page.getByTestId("answer-panel")).toHaveAttribute("data-phase", "idle");
    await expect(page.getByTestId("sidebar-item-home")).toHaveCount(0);
    await expect(page.locator(".pill-root")).toHaveCount(0);

    // Its page is see-through, like the pill's: the main window's
    // stylesheet (an opaque background) did not come along.
    const background = await page.evaluate(
      () => getComputedStyle(document.body).backgroundColor,
    );
    expect(background).toBe("rgba(0, 0, 0, 0)");
  });

  test("the panel follows ask_tick: phase and answer text", async ({ page }) => {
    await page.goto("/");
    await waitForAskTickListener(page);

    await emitAskTick(page, { phase: "thinking", question: "what is a tap" });
    await expect(page.getByTestId("answer-panel")).toHaveAttribute("data-phase", "thinking");
    await expect(page.getByTestId("answer-text")).toHaveText("");

    await emitAskTick(page, { phase: "answering", answer: "A tap is" });
    await expect(page.getByTestId("answer-phase")).toHaveText("answering");
    await expect(page.getByTestId("answer-text")).toHaveText("A tap is");

    await emitAskTick(page, { phase: "done", answer: "A tap is a short press." });
    await expect(page.getByTestId("answer-panel")).toHaveAttribute("data-phase", "done");
    await expect(page.getByTestId("answer-text")).toHaveText("A tap is a short press.");

    // An older tick that arrives late does not take the answer back.
    await emitAskTick(page, { phase: "answering", answer: "A tap", version: 1 });
    await expect(page.getByTestId("answer-text")).toHaveText("A tap is a short press.");
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
  });
});
