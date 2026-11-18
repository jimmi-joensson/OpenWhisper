import { describe, expect, it } from "vitest";
import {
  PHASE_DONE,
  PHASE_IDLE,
  PHASE_RECORDING,
  PHASE_TRANSCRIBING,
  type DictationTick,
} from "./dictation";
import { NO_REFINE } from "./refine";
import { dictationJustFinished } from "./use-last-transcription";

function tick(patch: Partial<DictationTick>): DictationTick {
  return {
    phase: PHASE_IDLE,
    session_kind: "dictation",
    status: "idle",
    status_message: "",
    transcript: "",
    replacements: [],
    refine: NO_REFINE,
    confidence: 0,
    sample_count: 0,
    elapsed_ms: 0,
    error_message: "",
    can_toggle: true,
    is_recording: false,
    cancel_would_act: false,
    level: 0,
    download_bytes_done: 0,
    download_bytes_total: 0,
    ...patch,
  };
}

describe("dictationJustFinished", () => {
  it("fires when a dictation leaves transcribing with text", () => {
    const done = tick({ phase: PHASE_DONE, transcript: "hello there" });
    expect(dictationJustFinished(PHASE_TRANSCRIBING, done)).toBe(true);
    // Straight to idle, and arriving at done from elsewhere, count too.
    expect(
      dictationJustFinished(PHASE_TRANSCRIBING, tick({ transcript: "hello there" })),
    ).toBe(true);
    expect(dictationJustFinished(PHASE_RECORDING, done)).toBe(true);
  });

  it("stays quiet while nothing finished or there is no text", () => {
    expect(dictationJustFinished(PHASE_DONE, tick({ phase: PHASE_DONE, transcript: "x" }))).toBe(
      false,
    );
    expect(
      dictationJustFinished(PHASE_RECORDING, tick({ phase: PHASE_TRANSCRIBING, transcript: "x" })),
    ).toBe(false);
    expect(dictationJustFinished(PHASE_TRANSCRIBING, tick({ phase: PHASE_DONE, transcript: " " }))).toBe(
      false,
    );
  });

  it("never fires for an ask, which ends on done with the last dictation's transcript", () => {
    const ask = tick({
      phase: PHASE_DONE,
      session_kind: "ask",
      status_message: "question sent",
      transcript: "the dictation before the ask",
    });
    expect(dictationJustFinished(PHASE_TRANSCRIBING, ask)).toBe(false);
    expect(dictationJustFinished(PHASE_RECORDING, ask)).toBe(false);
    expect(
      dictationJustFinished(PHASE_TRANSCRIBING, { ...ask, phase: PHASE_IDLE }),
    ).toBe(false);
  });

  it("treats a tick without a kind as a dictation", () => {
    // A tick from a shell that predates the field.
    const legacy = tick({ phase: PHASE_DONE, transcript: "hello" }) as Partial<DictationTick>;
    delete legacy.session_kind;
    expect(dictationJustFinished(PHASE_TRANSCRIBING, legacy as DictationTick)).toBe(true);
  });
});
