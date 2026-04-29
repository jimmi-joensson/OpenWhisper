---
name: openwhisper-iteration-budget
description: Iteration discipline rule — at most TWO code attempts at a misbehaving feature (initial implementation + one feedback-driven fix) before stopping to do deeper research. READ before reaching for a third "let me try X" change in response to "still not working" feedback. Applies to any feature implementation, bug fix, or platform integration where the first observable test contradicts what the code was supposed to do.
---

# Iteration budget — research before the third attempt

## The rule

You get **two code attempts** at a misbehaving feature before you stop and do deeper research:

1. **Attempt #1 — initial implementation.** Build the feature based on the spec/plan. Test it.
2. **Attempt #2 — first feedback-driven fix.** User reports it doesn't work, you diagnose, you adjust, you ship the change. Test it again.

If attempt #2 still doesn't work, **stop iterating on guesses**. Do real research before writing any more code:

- Web search for prior art in the same problem class (e.g. "render HUD over fullscreen macOS app").
- Read the canonical platform docs for the API you're using.
- Look at how shipping apps in the same niche solved it.
- Spawn a research agent (`general-purpose` or `Explore`) with a budget for breadth, not depth.

Only after that research produces a concrete approach with a stated reason it should work — and ideally a citation that someone else has shipped it — do you write more code.

## Why

Two attempts is the right budget because:

- **First attempt** is "what the spec said to do." Sometimes the spec is wrong. Worth one round of feedback to find out.
- **Second attempt** is "obvious correction based on the new symptom." If the obvious correction also fails, the problem is no longer obvious — your model of the system is missing something the docs / community / OS-level docs would tell you, but local guessing won't.
- **Third+ attempt without research** burns trust, cycle time, and dev-loop momentum. Iterations get smaller and more cargo-culted. The user can see the loop forming before you can. The 2nd→3rd→4th→5th pattern is recognizable from the outside as "thrashing."
- The OpenWhisper codebase has hit this trap repeatedly on platform integrations (Tauri × macOS Spaces, WebView2 × WH_KEYBOARD_LL, AppKit × `acceptsFirstMouse`). In every case, 30 minutes of upfront research would have produced the right answer in one attempt; the multi-attempt loop produced wrong answers and a TASK-57-style "oops, that watchdog corrupted kernel state" entry in `openwhisper-platform-gotchas`.

## How to apply

When you ship a code change in response to "still not working" feedback, ask yourself: **is this my second attempt, or my third+?**

- **2nd attempt** — fine, ship the obvious correction.
- **3rd+ attempt** — STOP. Tell the user explicitly: *"Two attempts haven't worked. Pausing for research before next code change."* Then research. Don't write code in this turn.

Treat it as a hard gate, not a heuristic. The temptation will be "I see the next thing to try, let me just do it" — that's the trap. The "next thing to try" after two failed attempts is almost always also wrong, because the failure mode you're modelling is wrong.

The research itself can — and usually should — happen in a subagent so the main thread stays uncluttered. A `general-purpose` agent with a specific, source-naming prompt ("search Apple Developer Forums + tauri GitHub issues + indie Mac dev blogs for X; report under 600 words with cited URLs") is the standard shape.

## What counts as "an attempt"

- Writing code that you believe addresses the symptom, then asking the user to re-test → counts.
- Reading more of the codebase / running a diagnostic command without changing code → does NOT count.
- Adding logging / instrumentation to learn what's happening → does NOT count, this IS research.
- Reverting your last change because it made things worse → does NOT count as a new attempt; the budget tracks forward attempts.

If you're unsure whether your next change is "research-informed" or "another guess," default to it being a guess, and do the research first.

## What "doing the research" looks like

Not just "I'll think about it." Concrete:

- A web search with terms specific to the failure mode, not the feature name.
- Reading the actual platform source / docs you're calling into (e.g. tao's `set_visible_on_all_workspaces` impl, not Tauri's docs about it).
- Looking at one or two shipping apps in the same niche to see what they did.
- For platform-API issues: the relevant Apple Developer Forum thread / Microsoft Learn page / Tauri GitHub issue almost always exists; find it.

The output of research should be: **one stated approach + one cited reason it should work** — *before* you change code. If you can't write that sentence, you haven't researched enough yet.

## Boundary

This rule does NOT apply to:

- Multi-step features where each step is independently scoped (5 plan tasks, each with 1-2 attempts allowed). The budget resets per discrete sub-problem.
- Refactors, cleanups, doc edits, or anything where there's no observable "does it work" check — those don't fit the iteration shape.
- Type errors, lint errors, build errors — those are the compiler/linter telling you exactly what's wrong; just fix them.

It applies specifically to: *feature implementations or bug fixes where the observable behavior is wrong, you wrote a fix, the observable behavior is still wrong, and you're tempted to write another fix.* That's the moment to stop.
