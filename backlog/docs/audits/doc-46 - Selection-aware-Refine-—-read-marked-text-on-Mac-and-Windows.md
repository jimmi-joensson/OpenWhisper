---
id: doc-46
title: Selection-aware Refine — read marked text on Mac and Windows
type: research
created_date: '2026-05-09 08:27'
status: research
related:
  - milestones/m-1
---

# Selection-aware Refine — read marked text on Mac and Windows

Feature exploration: capture the user's currently-selected text in the focused application so the Refine block can rewrite or adjust it. User gesture: highlight a paragraph, invoke OpenWhisper, say *"make this more concise"*, output replaces the selection.

This is a strong companion to the dictation flow — turns OpenWhisper from a transcription tool into a writing tool. Highest-ROI feature currently on the table outside the Emma-parity arc, and cheaper than Vision.

## Mechanism — by platform

### macOS

**Primary path — Accessibility API.** Read `AXSelectedText` attribute from the focused `AXUIElement`. Native AppKit apps (TextEdit, Mail, Notes, Pages, Xcode) all expose it. Web browsers (Safari, Chrome, Firefox) expose via their AX shim — works in most input contexts. **No new TCC grant required** — the Accessibility permission OpenWhisper already needs for paste covers selected-text reads.

**Fallback — `⌘C` + read pasteboard + restore.** Universal but destructive:

1. Snapshot current pasteboard (multi-format: `public.utf8-plain-text`, `public.rtf`, image types, file refs).
2. Send `⌘C`.
3. Poll pasteboard for change-count update.
4. Read new contents.
5. Write the snapshot back.

Race conditions if the user is mid-copy elsewhere. Lossy on exotic clipboards (multi-format payloads can't always be perfectly round-tripped). Acceptable as a fallback when AX returns empty.

### Windows

**Primary path — UI Automation.** `TextPattern.GetSelection()` on the focused element. WPF / WinUI / UWP / Office / properly-built Win32 all expose it. Browsers via UIA shim — mostly works. Standard unprivileged process can call it; no admin rights needed.

**Fallback — `Ctrl+C` + clipboard restore.** Same dance as macOS. Same caveats.

## Reliability gotchas

- **Electron apps lie.** Some custom text widgets in Electron apps don't expose selection through AX/UIA correctly — must fall back to clipboard.
- **Terminals and canvas-rendered apps return nothing.** Selection in a terminal emulator or a `<canvas>`-based editor is often invisible to the OS accessibility layer. Clipboard fallback works.
- **Selection can be lost when the pill window is brought to front.** Hotkey handlers commonly steal focus before reading selection. Capture order must be: read selection synchronously inside the hotkey handler → *then* present the pill / show UI.
- **Clipboard save/restore is racy.** Multi-format payloads, rapid user input, and clipboard managers in the wild all complicate the dance. Worth doing, won't be perfect.

## Two integration shapes

### Option 1 — A "Selection" tool on Refine

Add `Selection` to the Refine block's TOOLS catalog (sibling to the proposed Screen tool from the Vision design). Binary toggle in the tools sheet. When active:

- Hotkey handler reads selection before pill activation.
- Dictation prompt becomes *"rewrite this selection per the instruction"*.
- Output replaces the selection (paste-over) instead of inserting at cursor.
- If no selection is detected, falls back to normal dictation insert-at-cursor.

Smallest possible change. Keeps the pipeline canvas unchanged. Discovery is mediocre — the gesture is meaningfully different from dictation but is buried in a tool sheet.

### Option 2 — A new pipeline row (recommended)

Sibling to the dictation row and the proposed Vision row, at the Pipeline canvas level:

```
[Trigger: "When invoked with a selection"] → [Refine block] → [Output: "Replace selection"]
```

Empty-selection state degrades to normal dictation. The trigger end-cap glyph is selection-themed (cursor with text-marquee). The output end-cap is a paste-over glyph (text replacing existing text). Refine model is shared with the dictation row's Refine block — same loaded weights, no extra memory cost.

Better mental model: the user gesture (mark + invoke) is genuinely different from raw dictation. Matches the precedent set by the Vision row treating distinct triggers as sibling pipelines. Discovery is much higher.

## Pairing with Vision

Selection + Vision composes powerfully. Example utterances become coherent:

- *"Look at my screen and rewrite the selected paragraph based on what's in the chart."*
- *"This selection is wrong — fix it using what you see."*

Selection gives Refine **what to change**. Vision gives Refine **context to change it against**. Worth designing both as composable inputs to the Refine block rather than isolated features. The TOOLS slot on Refine becomes the natural home for both as input modifiers.

## Effort

| Piece | Estimate |
|---|---|
| Mac AX selection read (primary) | ~1 day. Existing AX bindings for paste in core scaffold the harness. |
| Windows UIA selection read (primary) | ~1 day. |
| Clipboard save/restore fallback (multi-format dance) | ~3 days. Most of the engineering risk lives here. |
| Replace-selection paste path (vs insert-at-cursor) | ~1 day. Sequence: select-all-of-prior-selection → paste new text. |
| UI surface — Option 1 (Tool toggle) | ~1 day. |
| UI surface — Option 2 (Pipeline row) | ~2 days. |
| Tests + edge cases (Electron apps, empty selection, focus-loss timing) | ~1 day. |

**Total: ~1 week** at Option 2. Smaller than Vision (~4 wks), smaller than custom modes (~2 wks), comparable to cloud Whisper provider (~1 wk).

## Risks

- **Electron / custom-widget apps** will need clipboard fallback. Some apps (e.g. VS Code embedded terminals, certain Discord text fields) may break entirely. Acceptable — fall back gracefully to insert-at-cursor with a small status note.
- **Clipboard managers** (Maccy, Paste, Ditto) on the user's box may capture the temporary clipboard write during the fallback dance. Need to test with the popular ones. Worst case: clipboard manager records "selected text" as a paste history entry — annoying but not destructive.
- **Focus-loss after selection capture.** If reading selection takes too long (some AX/UIA calls can stall), pill activation may have already moved focus. Mitigation: capture is a fast attribute read, kept on the synchronous path of the hotkey handler.
- **Multi-format clipboards.** Restoring an image clipboard back after a text fallback can fail silently for some image types. Mitigation: at minimum restore plaintext + RTF; image restoration is best-effort.

## Permissions

No new platform permission grants. Mac uses the existing Accessibility entitlement; Windows uses standard unprivileged UIA. This is a meaningful unlock — every other feature in the Emma-parity arc that involved screen capture or vision needed a fresh permission flow. Selection reading slots in for free.

## Recommendation

**Ship before Vision.** Reasons:

1. Smaller (1 wk vs 4 wks).
2. No new permissions.
3. No principle conflicts.
4. Compounds with Vision once Vision lands (composable inputs to Refine).
5. Reframes the product story: OpenWhisper becomes a *writing tool that listens*, not just a *transcription tool*. Stronger differentiator than vision-OCR alone for users who already type 80% of the day.
6. Pairs with the cloud-LLM cleanup unlock (TASK-17 / TASK-45 revival) — "rewrite selected text" is the canonical use case for an LLM cleanup pass.

**Sequence in the Emma-parity arc:**

```
1. Cloud LLM cleanup (revive TASK-17 / TASK-45)              ~1 wk
2. Selection-aware Refine (this doc)                         ~1 wk
3. Custom modes                                              ~2 wks
4. Custom actions                                            ~3 wks
5. Document analysis                                         ~2 wks
6. Local vision (Vision row + screen tool)                   ~4 wks
7. Reminders                                                 ~1.5 wks
```

Selection moves to position 2 — second feature in the arc, immediately after the cleanup unlock. Lands as a small visible win that makes everything afterwards feel more cohesive.

## Open questions

- **Option 1 vs Option 2** — tool toggle vs pipeline row. Recommendation is Option 2 for discoverability and mental-model coherence with Vision. Worth a Claude Design pass to confirm.
- **Replace-selection vs insert-after-selection.** Default should be replace, but some users may want the original preserved with the rewrite appended. Decide once usage patterns emerge — defer.
- **Voice command alternative trigger.** While dictating, "rewrite the highlighted paragraph as…" could capture selection at utterance time. Adds latency since selection must be re-read after voice parse. Defer to post-launch.
- **History / undo.** Replacing a selection silently could feel scary. Consider a one-shot toast `Replaced 47 words — ⌘Z to undo` on first use, dismissible.

## Net read

Selection-aware Refine is the cheapest, most-leveraged feature on the OpenWhisper roadmap right now. ~1 week of engineering, no new permissions, no principle conflicts, composes with everything else planned. Pull forward to the front of the Emma-parity arc.
