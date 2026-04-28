# Agent Skill Ecosystem

## Contents
- [SKILL.md as an Open Standard](#skillmd-as-an-open-standard)
- [SKILL.md vs AGENTS.md](#skillmd-vs-agentsmd)
- [Platform-Specific Instruction Files](#platform-specific-instruction-files)
- [Cross-Platform Principles](#cross-platform-principles)
- [Portability Checklist](#portability-checklist)

## SKILL.md as an Open Standard

SKILL.md is an open specification for portable AI agent skills, defined at [agentskills.io](https://agentskills.io). It has been adopted by 30+ tools including Claude Code, Cursor, Gemini CLI, OpenAI Codex, VS Code, and JetBrains Junie.

### Format Definition

Every SKILL.md follows this anatomy:

```yaml
---
name: skill-name
description: Trigger conditions and capability summary.
---

# Skill Title

[Markdown body — active workflow knowledge, not documentation]

## Workflows

[Links to reference files for progressive disclosure]
```

**Required elements:**
- **YAML frontmatter** — `name` (kebab-case identifier) and `description` (trigger conditions + differentiator)
- **Markdown body** — principles, defaults to override, and workflow entry points

**Optional elements:**
- `references/` directory — progressive disclosure for variant workflows, templates, schemas
- `scripts/` directory — automation scripts the skill can invoke
- `assets/` directory — images, templates, or data files

### Why a Standard Matters

A skill written to this spec works across any agent that supports SKILL.md loading. Write once, use everywhere — the same skill works in Claude Code, Cursor, Gemini CLI, and any other compliant tool.

## SKILL.md vs AGENTS.md

These serve complementary roles. Confusing them leads to bloated skills or underpowered repo instructions.

| | SKILL.md | AGENTS.md |
|---|---|---|
| **Role** | Specialized playbook | Repo-wide rulebook |
| **Scope** | One workflow capability | Entire repository |
| **Portability** | Moves between projects | Lives in one repo |
| **When loaded** | On-demand, when triggered | Always, for every task |
| **Analogy** | A recipe card | Kitchen house rules |

**SKILL.md** = "When processing CSVs, use pdfplumber, validate before transforming, never silently drop rows."

**AGENTS.md** = "This repo uses TypeScript strict mode, runs tests with vitest, and deploys to Cloudflare Workers."

Use AGENTS.md for conventions any agent working in the repo needs. Use SKILL.md for specialized workflows that transfer across repos.

## Platform-Specific Instruction Files

Different platforms use different file conventions for agent instructions. Skills written to the SKILL.md spec are portable across all of them.

| Platform | Repo-Level Instructions | Skill Locations (SKILL.md) | Also Reads `.claude/skills/`? |
|---|---|---|---|
| Claude Code | `CLAUDE.md` | `.claude/skills/*/` | Yes (native) |
| VS Code (Copilot) | `.github/copilot-instructions.md` | `.github/skills/*/`, `.claude/skills/*/`, `.agents/skills/*/` | Yes (native) |
| GitHub Copilot CLI | — | `.github/skills/*/`, `.claude/skills/*/` | Yes (native) |
| Gemini CLI | `GEMINI.md` | `.gemini/skills/*/`, `.agents/skills/*/` | No |
| OpenAI Codex | `AGENTS.md` | `.agents/skills/*/` | No |
| Windsurf | `.windsurf/rules/*.md` | `.windsurf/skills/*/`, `.agents/skills/*/` | Yes (with config flag) |
| JetBrains Junie | `.junie/AGENTS.md`, `AGENTS.md` | `.junie/skills/*/` | Detects and offers import |
| Cursor | `.cursor/rules/*.mdc` | `.cursor/rules/*.mdc` (own format) | No |
| Aider | `--read CONVENTIONS.md` | No SKILL.md support | No |

The principles in this skill — Goldilocks zone, defaults-first, token efficiency, imperative voice — apply regardless of which file format you target.

## Cross-Platform Principles

These principles hold true across every agent instruction format:

- **Token efficiency matters everywhere.** Every file an agent reads consumes finite context. Bloated instructions degrade performance on all platforms.
- **Examples beat descriptions.** A 5-line code snippet teaches more than a paragraph of prose, regardless of the agent.
- **Modular beats monolithic.** Progressive disclosure (small entry point + reference files) works better than one massive file on every platform.
- **One default, not many options.** Agents across all platforms perform worse when given multiple choices without a clear recommendation.
- **Imperative voice.** "Extract text with pdfplumber" works better than "You can use pdfplumber" for every agent, not just one.
- **Trigger info in metadata, not body.** Whether it's a YAML `description`, a frontmatter block, or a file header — activation context belongs where the platform looks for it.

## Portability Checklist

Use this when writing skills intended to work across multiple agents or platforms:

- [ ] **Agent-neutral language.** Use "the agent" not a specific product name. Principles should apply universally.
- [ ] **No platform-specific APIs in the body.** Reference platform-specific features (like skill loading paths) in reference files, not the core SKILL.md.
- [ ] **Test with 2+ agents.** Verify the skill produces good output on at least two different platforms before calling it portable.
- [ ] **Minimal script dependencies.** If the skill uses `scripts/`, ensure they work cross-platform or document requirements.
- [ ] **Standard markdown only.** Avoid platform-specific markdown extensions. Stick to CommonMark.
- [ ] **Self-contained references.** Reference files should not assume platform-specific context loading behavior.
