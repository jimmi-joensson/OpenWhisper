# Creating a New Skill

## Contents
- [Process](#process)
- [Before You Write Anything](#before-you-write-anything)
- [Drafting the Skill](#drafting-the-skill)
- [Structure Decisions](#structure-decisions)
- [Description Template](#description-template)
- [Minimum Viable Skill Example](#minimum-viable-skill-example)

## Process

1. Run the task with no skill. Observe what the agent gets wrong — those are the convergent defaults.
2. List the specific defaults you want to override.
3. Draft SKILL.md targeting those defaults. Start at ~50 lines.
4. Write the description with all trigger conditions.
5. Test with a fresh agent session on a real task.
6. Iterate: add what's missing, remove what's redundant.

## Before You Write Anything

Do the task manually with the agent first. Complete the entire workflow through normal prompting. Note every correction you make — those corrections ARE the skill content.

After each correction, ask the agent: "Don't regenerate. Explain why you chose X." The answers reveal which defaults need overriding and why.

Resist the urge to explain the domain. If the agent already knows it (what a PDF is, how React state works, what an API does), that paragraph wastes context and adds no value.

## Drafting the Skill

Start with a "Defaults to Override" section. This is the highest-value content in any skill — the specific things the agent gets wrong that you want it to do differently.

Add principles only when a default isn't self-explanatory. If "Use pdfplumber, not PyMuPDF" is sufficient, don't add a paragraph explaining why.

Use imperative form throughout. "Extract text with pdfplumber" not "You can use pdfplumber to extract text."

One code example is worth five paragraphs. Show the pattern you want, don't describe it.

Pick one default approach for every decision point. Don't list five libraries. Name one and add an escape hatch footnote for edge cases.

## Structure Decisions

When to stay in SKILL.md vs. extract to references:

| Condition | Action |
|-----------|--------|
| Body under 100 lines, single workflow | Everything in SKILL.md |
| Body approaching 200 lines | Extract variant workflows to references |
| Multiple distinct workflows (create, refactor, test) | One reference per workflow |
| Domain-specific templates or schemas | Reference file per template |
| Reference file exceeding 300 lines | Split into two files by concern |

References are one level deep only. SKILL.md links to references. References never link to other references.

Add a table of contents to any file over 100 lines so agents can see scope even with partial reads.

## Description Template

All trigger conditions go in the description field, not the body. The description controls when the skill activates. Pattern:

```
[Capability verb phrase]. Use when (1) [trigger condition], (2) [trigger condition],
or (3) [trigger condition]. [What it provides — differentiator from working without the skill].
```

The body loads only after triggering, so "when to use" info in the body is invisible at activation time.

## Minimum Viable Skill Example

A skill for CSV data processing — **without** this skill, the agent writes 400-line documentation dumps explaining what pandas is and listing four CSV libraries.

**Bad skill (documentation dump, 30 lines shown):**
```yaml
---
name: csv-processing
description: Process CSV files with Python.
---
# CSV Processing

CSV (Comma-Separated Values) is a common data format...

## Libraries
- pandas: Full-featured data analysis...
- csv module: Built-in Python module...
- polars: Fast DataFrame library...

## Reading CSVs
You can read CSV files using pandas.read_csv()...
```

**Good skill (defaults-first, 30 lines shown):**
```yaml
---
name: processing-csvs
description: Clean, validate, and transform CSV data in Python. Use when (1) building data pipelines that ingest CSVs, (2) cleaning messy CSV exports, or (3) transforming CSVs between schemas. Provides opinionated pandas patterns and validation workflow.
---
# Processing CSVs

## Defaults to Override
- Read with `pd.read_csv()` using `dtype=str` first. Infer types after inspection, not during load.
- Validate before transforming. Check row counts, null percentages, and dtype distributions before any mutations.
- Never silently drop rows. Log every filtered row with reason to stderr.
- Use `.pipe()` chains for transforms. Each step is a named function, not inline lambda logic.

## Validation Pattern
[3-line code example of the validation workflow]

## Workflows
**Cleaning raw exports?** → See references/cleaning.md
**Schema transformation?** → See references/transforms.md
```

The good skill is the same length but targets specific defaults (silent type inference, silent row drops, inline lambdas) instead of explaining what CSV files are.
