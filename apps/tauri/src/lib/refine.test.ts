import { describe, expect, it } from "vitest";
import { effortLabel, effortPhrase, modelLabel, refinedByLabel } from "./refine";

describe("modelLabel", () => {
  it("reads a Claude model as family and version", () => {
    expect(modelLabel("claude-sonnet-5-5")).toBe("Sonnet 5.5");
    expect(modelLabel("claude-opus-4-1-20250805")).toBe("Opus 4.1");
    expect(modelLabel("claude-3-5-sonnet-20241022")).toBe("Sonnet 3.5");
    expect(modelLabel("claude-haiku-5")).toBe("Haiku 5");
  });

  it("reads a bare family name, as the settings have it", () => {
    expect(modelLabel("sonnet")).toBe("Sonnet");
    expect(modelLabel("Opus")).toBe("Opus");
  });

  it("leaves any other model as it is written", () => {
    expect(modelLabel("qwen2.5:7b")).toBe("qwen2.5:7b");
    expect(modelLabel("gpt-4o-mini")).toBe("gpt-4o-mini");
  });
});

describe("refinedByLabel", () => {
  it("prefers the model and falls back to the backend", () => {
    expect(refinedByLabel("claude-sonnet-5-5", "claude-cli")).toBe("Sonnet 5.5");
    expect(refinedByLabel("", "claude-cli")).toBe("Claude");
    expect(refinedByLabel(undefined, "openai-compatible")).toBe("OpenAI-compatible");
    // A history entry that stored the backend's id.
    expect(refinedByLabel("claude-cli")).toBe("Claude");
    expect(refinedByLabel(null)).toBe("");
  });
});

describe("effortLabel", () => {
  it("names the effort the way the Refine settings do, or nothing", () => {
    expect(effortLabel("low")).toBe("Low");
    expect(effortLabel("xhigh")).toBe("Extra high");
    expect(effortLabel("max")).toBe("Max");
    expect(effortLabel("")).toBe("");
    expect(effortLabel(undefined)).toBe("");
    // An effort this build does not know: as written.
    expect(effortLabel("turbo")).toBe("turbo");
  });
});

describe("effortPhrase", () => {
  it("says the effort after a model name, or nothing", () => {
    expect(effortPhrase("low")).toBe("low effort");
    expect(effortPhrase("xhigh")).toBe("extra high effort");
    expect(effortPhrase("")).toBe("");
    expect(effortPhrase(null)).toBe("");
  });
});
