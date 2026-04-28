---
name: writing-skills
description: Write effective, context-efficient AI agent skills. Use when creating, editing, refactoring, or reviewing a skill — including discipline-enforcing skills (TDD-style, verification gates, "do X before Y") that must resist rationalization under pressure.
---

# Writing Skills

## Contents
- [The Goldilocks Zone](#the-goldilocks-zone)
- [Defaults to Override](#defaults-to-override)
- [Sizing & Structure](#sizing--structure)
- [Skill Scoping](#skill-scoping)
- [Writing Quality](#writing-quality)
- [Discipline-Enforcing Skills Need Extra Care](#discipline-enforcing-skills-need-extra-care)
- [SKILL.md as an Open Standard](#skillmd-as-an-open-standard)
- [Workflows](#workflows)

Skills are active workflow knowledge, not documentation dumps. A skill teaches an agent *how to perform a task*, not *what a tool does*. This distinction drives every decision below.

## The Goldilocks Zone

The most common failure in skill writing is getting the altitude wrong.

**Too prescriptive** — Hardcoded step-by-step logic, brittle if-else rules, listing every edge case. This limits the agent's reasoning and creates fragility. Reasoning models perform *worse* with overly detailed instructions because they overanalyze the constraints instead of finding optimal solutions.

**Too vague** — High-level platitudes, generic guidance, assuming shared context. The agent defaults to generic output because it has no signal to deviate.

**The sweet spot** — Express the *principles behind behaviors*, not the behaviors themselves. Override the specific defaults that produce bad output and let the agent reason about everything else.

Example — the difference between altitude levels for a PDF processing skill:

**Too prescriptive:** "1. Import pdfplumber. 2. Open file with pdfplumber.open(). 3. Iterate pages. 4. Call page.extract_text(). 5. If text is empty, fall back to OCR using pytesseract. 6. Save output as .txt alongside the input."

**Too vague:** "Handle PDFs appropriately. Extract text and process it for downstream use."

**Sweet spot (overriding defaults with principles):** "Extract text with pdfplumber, not PyMuPDF. For scanned pages (no extractable text), fall back to pytesseract OCR. Preserve page boundaries in output — downstream tasks need page numbers."

The sweet spot is 3 lines. It overrides exactly three convergent defaults (library choice, OCR fallback strategy, output format) and trusts **the agent** to handle everything else.

## Defaults to Override

Without a skill, agents converge on these patterns when writing skills:

- **Documentation dumps**: Explaining what tools are and how they work instead of encoding active workflow knowledge. Agents know what tools do — skills should teach them *how you use them differently*.
- **Explaining the obvious**: Paragraphs about what a PDF is, how React state works, what an API does. Delete anything the agent already knows.
- **Multiple options instead of one default**: "You can use pdfplumber, PyMuPDF, or pdfminer..." Pick one. Add an escape hatch for edge cases.
- **Passive voice**: "You can use..." / "It's possible to..." Write imperatively: "Extract text with pdfplumber."
- **Flat structure**: Everything in SKILL.md, no progressive disclosure. Move variant detail to reference files when the body exceeds 100 lines.
- **Trigger info in the body**: Burying "when to use" context in the skill body instead of the description field where it actually drives activation.
- **Generic template structure**: Following a template (Overview, Setup, Configuration, Usage) instead of leading with the specific defaults that need overriding for this particular domain.

The process for identifying convergent defaults in any domain is covered in the [creation workflow](references/creating.md#before-you-write-anything).

## Sizing & Structure

**SKILL.md body: target ~200 lines.** The official limit is 500, but real-world testing shows ~200 lines is the sweet spot for agent scanning efficiency (4.8x better token efficiency vs bloated skills). If approaching 200 lines, split into reference files.

**Reference files: 200–300 lines each.** One concern per file. This keeps total loaded context to 400–500 lines of highly relevant material instead of 1,000+ lines of mixed relevance.

**References one level deep only.** SKILL.md → reference file. Never reference file → another reference file. Agents may only partially read nested references.

**Table of contents for files over 100 lines.** Agents can see scope even with partial reads.

## Skill Scoping

**Group by workflow capability, not by tool.** Don't create one skill per tool (cloudflare, docker, gcloud). Create one skill per workflow (`deploying-infrastructure` covering all three). This prevents loading 3 skills when only 1 capability is needed.

**Ask:** "What is the agent *doing* when this skill triggers?" The answer should be an activity (deploying, analyzing, processing) not a tool name.

**Naming:** Use gerund form — `processing-pdfs`, `deploying-infrastructure`, `analyzing-data`. This naturally frames skills as capabilities.

## Writing Quality

- **Description is the trigger.** All "when to use" information belongs in the description, not the body. The body loads only after triggering.
- **The description states triggers, not workflow.** Don't summarize the skill's process in the description — agents will follow the description and skip the body. Triggers and symptoms only.
- **The agent is already smart.** Only add context it doesn't already have. If you're explaining what a PDF is, delete that paragraph.
- **One default, not many options.** Don't list five libraries. Pick one and provide an escape hatch for edge cases.
- **Imperative form.** "Extract text with pdfplumber" not "You can use pdfplumber to extract text."
- **Concrete examples over abstract explanations.** A 5-line code snippet teaches more than a paragraph of prose.

## Discipline-Enforcing Skills Need Extra Care

Skills that enforce a rule the agent will be tempted to skip — TDD, verification-before-completion, "do X before Y" gates — get rationalized away under pressure (sunk cost, time, "this case is different") unless they're explicitly bulletproofed and tested.

For these skills:

- **Bulletproof the writing** → see [references/bulletproofing.md](references/bulletproofing.md) for the Iron Law, rationalization tables, red flags, spirit-vs-letter framing, and persuasion-principle leverage.
- **Test under pressure** → see [references/pressure-testing.md](references/pressure-testing.md) for RED-GREEN-REFACTOR with multi-pressure subagent scenarios.

Skip both for technique skills, pattern skills, or pure reference skills (API docs, syntax guides) — they have no rule to violate. Use [references/testing.md](references/testing.md) for general iteration instead.

## SKILL.md as an Open Standard

SKILL.md is an open specification for portable agent skills, defined at [agentskills.io](https://agentskills.io) and adopted by 30+ tools including Claude Code, Cursor, Gemini CLI, OpenAI Codex, VS Code, and JetBrains Junie. The complementary AGENTS.md standard has been adopted by 20,000+ projects and donated to the Linux Foundation.

**Anatomy:** YAML frontmatter (`name`, `description`) + markdown body + optional `references/` directory. This is the format used throughout this skill.

**SKILL.md vs AGENTS.md:** Skills are specialized playbooks — portable, on-demand, scoped to one workflow. AGENTS.md is a repo-wide rulebook — always loaded, project-specific conventions. Use both: AGENTS.md for repo rules, SKILL.md for transferable capabilities.

**Universal principles:** The Goldilocks zone, defaults-first, token efficiency, imperative voice, and progressive disclosure apply to every agent instruction format — not just SKILL.md. Whether you're writing `.cursor/rules/*.mdc`, `GEMINI.md`, or `AGENTS.md`, the same principles produce better results.

For platform-specific file conventions and portability guidance, see [references/ecosystem.md](references/ecosystem.md).

## Workflows

**Creating a new skill?** → See [references/creating.md](references/creating.md) for the step-by-step creation workflow.

**Testing & iterating a skill?** → See [references/testing.md](references/testing.md) for the Agent A/B method and evaluation-driven development.

**Refactoring an existing skill?** → See [references/refactoring.md](references/refactoring.md) for the audit checklist and consolidation patterns.

**Bulletproofing a discipline-enforcing skill?** → See [references/bulletproofing.md](references/bulletproofing.md) for Iron Law framing, rationalization tables, and red flags.

**Pressure-testing a discipline-enforcing skill?** → See [references/pressure-testing.md](references/pressure-testing.md) for TDD-style subagent testing.

**Understanding the skill ecosystem?** → See [references/ecosystem.md](references/ecosystem.md) for platform conventions and portability.

**Anthropic's official authoring guide?** → See [references/anthropic-best-practices.md](references/anthropic-best-practices.md).

**Persuasion psychology behind effective skills?** → See [references/persuasion-principles.md](references/persuasion-principles.md) for Cialdini's seven principles and which to apply by skill type.

**Visualizing flowcharts?** Use `render-graphs.js` at the skill root to render `.dot` diagrams to SVG. Style rules: `references/graphviz-conventions.dot`.
