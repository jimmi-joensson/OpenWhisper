// TASK-91: Transcribe route, pick / drop a file, progress, result + copy.
//
// The core job is stubbed in tests/fixtures/tauri-shim.ts: tests write
// `window.__owFileJob` to move the snapshot through its phases and the
// hook's 200 ms poll picks it up.
//
// MANUAL SMOKE (not CI-testable)
//   Real decode + recognition needs the Tauri shell and the model.
//   `pnpm dev:tauri`, open Transcribe, drop a WhatsApp .opus and an
//   .mp4 video; confirm progress moves per chunk, Cancel stops at the
//   next chunk, and Copy puts the text on the clipboard.

import type { Page } from "@playwright/test";
import { expect, test } from "./fixtures/tauri-shim";

const BASE_JOB = {
  phase: 0,
  busy: false,
  status: "",
  path: "/Users/test/voice.opus",
  file_name: "voice.opus",
  progress: 0,
  chunks_done: 0,
  chunks_total: 0,
  audio_ms: 0,
  elapsed_ms: 0,
  text: "",
  confidence: 0,
  error: "",
  cancelling: false,
};

async function setJob(page: Page, job: Partial<typeof BASE_JOB>) {
  await page.evaluate(
    (j) => {
      (window as unknown as { __owFileJob: unknown }).__owFileJob = j;
    },
    { ...BASE_JOB, ...job },
  );
}

async function openTranscribe(page: Page) {
  await page.goto("/");
  await page.getByTestId("sidebar-item-transcribe").click();
  await expect(page.getByRole("heading", { name: "Transcribe a file" })).toBeVisible();
}

test.describe("transcribe pane", () => {
  test("sidebar routes to Transcribe and updates the titlebar", async ({ page }) => {
    await openTranscribe(page);
    await expect(page.getByTestId("sidebar-item-transcribe")).toHaveAttribute(
      "aria-current",
      "page",
    );
    await expect(page.locator(".ow-titlebar__title")).toHaveText("OpenWhisper — Transcribe");
    await expect(page.getByTestId("transcribe-dropzone")).toBeVisible();
  });

  test("Choose file opens the picker with core's extensions and starts a job", async ({
    page,
  }) => {
    await page.addInitScript(() => {
      (window as unknown as { __owDialogPick: string }).__owDialogPick =
        "/Users/test/voice.opus";
    });
    await openTranscribe(page);
    await page.getByTestId("transcribe-choose").click();

    await expect(page.getByTestId("transcribe-progress")).toBeVisible();
    await expect(page.getByTestId("transcribe-status")).toContainText("Reading audio…");
    const { starts, options } = await page.evaluate(() => {
      const w = window as unknown as { __owFileStarts?: string[]; __owDialogOptions?: unknown };
      return { starts: w.__owFileStarts, options: w.__owDialogOptions };
    });
    expect(starts).toEqual(["/Users/test/voice.opus"]);
    expect(options).toMatchObject({
      multiple: false,
      filters: [{ extensions: ["wav", "mp3", "m4a", "opus", "mp4"] }],
    });
    // Picker is disabled while a job is running.
    await expect(page.getByTestId("transcribe-choose")).toBeDisabled();
  });

  test("dismissed picker starts nothing", async ({ page }) => {
    await openTranscribe(page);
    await page.getByTestId("transcribe-choose").click();
    await expect(page.getByTestId("transcribe-progress")).toHaveCount(0);
    const starts = await page.evaluate(
      () => (window as unknown as { __owFileStarts?: string[] }).__owFileStarts,
    );
    expect(starts).toBeUndefined();
  });

  test("dropping a file starts a job with its path", async ({ page }) => {
    await openTranscribe(page);
    await page.waitForFunction(
      () => window.__owEmit("tauri://drag-enter", { paths: ["/x"], position: { x: 0, y: 0 } }) > 0,
    );
    await expect(page.getByTestId("transcribe-dropzone")).toHaveAttribute("data-drag-over", "true");
    await expect(page.getByTestId("transcribe-dropzone")).toContainText("Drop to transcribe");

    await page.evaluate(() =>
      window.__owEmit("tauri://drag-drop", {
        paths: ["/Users/test/meeting.mp4"],
        position: { x: 0, y: 0 },
      }),
    );
    await expect(page.getByTestId("transcribe-progress")).toContainText("meeting.mp4");
    await expect(page.getByTestId("transcribe-dropzone")).not.toHaveAttribute(
      "data-drag-over",
      "true",
    );
  });

  test("progress, then result with copy", async ({ page, context }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    await openTranscribe(page);
    await setJob(page, {
      phase: 3,
      busy: true,
      status: "Transcribing… 40%",
      progress: 0.4,
      chunks_done: 2,
      chunks_total: 5,
      audio_ms: 120_000,
    });
    // Poll only runs while busy; a fresh mount reads the snapshot.
    await page.getByTestId("sidebar-item-home").click();
    await page.getByTestId("sidebar-item-transcribe").click();

    const progress = page.getByTestId("transcribe-progress");
    await expect(progress).toContainText("Transcribing… 40%");
    await expect(progress).toContainText("2:00 of audio");
    await expect(progress.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "40");

    await setJob(page, {
      phase: 4,
      busy: false,
      status: "Done",
      progress: 1,
      audio_ms: 120_000,
      elapsed_ms: 4_600,
      text: "Sveiki, šodien bija serviss.",
    });
    const result = page.getByTestId("transcribe-result");
    await expect(result).toContainText("Sveiki, šodien bija serviss.");
    await expect(result).toContainText("transcribed in 4.6 s");
    await expect(page.getByTestId("transcribe-progress")).toHaveCount(0);

    await page.getByTestId("transcribe-copy").click();
    await expect(page.getByTestId("transcribe-copy")).toContainText("Copied");
    const clip = await page.evaluate(() => navigator.clipboard.readText());
    expect(clip).toBe("Sveiki, šodien bija serviss.");
  });

  test("cancel calls the backend", async ({ page }) => {
    await openTranscribe(page);
    await setJob(page, { phase: 1, busy: true, status: "Reading audio…" });
    await page.getByTestId("sidebar-item-home").click();
    await page.getByTestId("sidebar-item-transcribe").click();

    await page.getByTestId("transcribe-cancel").click();
    const count = await page.evaluate(
      () => (window as unknown as { __owFileCancelCount?: number }).__owFileCancelCount,
    );
    expect(count).toBe(1);

    await setJob(page, { phase: 6, busy: false, status: "Cancelled" });
    await expect(page.getByTestId("transcribe-cancelled")).toContainText("voice.opus");
  });

  test("errors render as a destructive alert", async ({ page }) => {
    await setJobOnLoad(page, {
      phase: 5,
      status: "Couldn't transcribe this file: file has no audio track",
      error: "file has no audio track",
    });
    await openTranscribe(page);
    await expect(page.getByTestId("transcribe-error")).toContainText("file has no audio track");
  });

  test("start failure is surfaced", async ({ page }) => {
    await page.addInitScript(() => {
      const w = window as unknown as { __owDialogPick: string; __owFileStartError: string };
      w.__owDialogPick = "/Users/test/gone.wav";
      w.__owFileStartError = "not a file: /Users/test/gone.wav";
    });
    await openTranscribe(page);
    await page.getByTestId("transcribe-choose").click();
    await expect(page.getByTestId("transcribe-start-error")).toContainText("not a file");
  });
});

async function setJobOnLoad(page: Page, job: Partial<typeof BASE_JOB>) {
  await page.addInitScript(
    (j) => {
      (window as unknown as { __owFileJob: unknown }).__owFileJob = j;
    },
    { ...BASE_JOB, ...job },
  );
}
