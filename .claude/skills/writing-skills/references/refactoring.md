# Refactoring Existing Skills

Apply this workflow when a skill is too large, underperforming, or not triggering correctly.

## When to Trigger

- SKILL.md exceeds ~200 lines
- The skill loads too much context for simple tasks
- Output quality has degraded or is inconsistent
- The skill triggers when it shouldn't, or doesn't trigger when it should
- Multiple skills overlap in scope and compete for activation

## Audit Checklist

Evaluate the skill against these criteria:

- [ ] **Size**: Is SKILL.md over 200 lines? → Split into reference files
- [ ] **Documentation vs workflow**: Does it explain what tools *are* or how to *use* them? → Rewrite as active workflow knowledge
- [ ] **Convergent defaults**: Does it target specific bad defaults, or just describe everything generically? → Identify and override defaults
- [ ] **Goldilocks zone**: Is it over-prescriptive (brittle rules) or too vague (no signal)? → Find the principle behind each rule
- [ ] **Progressive disclosure**: Is all content in SKILL.md, or does it use reference files? → Move detail to references
- [ ] **Reference depth**: Are references nested (file → file → file)? → Flatten to one level from SKILL.md
- [ ] **Scope**: Is it organized by tool or by workflow capability? → Regroup by what the agent is *doing*
- [ ] **Description**: Does it contain all trigger conditions? → Move "when to use" out of the body and into the description
- [ ] **Options**: Does it offer multiple approaches where one default would suffice? → Pick one, add escape hatch for edge cases
- [ ] **Redundancy**: Is information duplicated between SKILL.md and references? → Single source of truth

## Refactoring Workflow

1. **Measure.** Count lines in SKILL.md and each reference file. Note total loaded context for a typical task.
2. **Classify every section.** For each block of content, ask: "Is this active workflow knowledge or passive documentation?" Delete or heavily condense passive documentation.
3. **Identify convergent defaults.** Test the task without the skill. What does the agent get wrong? Those are the defaults to target. Remove guidance for things the agent already does well.
4. **Extract to references.** Move variant-specific details, examples, and schemas into reference files. SKILL.md should be a map, not the territory.
5. **Tighten the description.** Ensure all trigger conditions are in the description field, not buried in the body.
6. **Test with Agent B.** Run real tasks with the refactored skill. Compare output quality and context usage against the old version.

## Consolidating Multiple Skills

If several skills overlap (e.g., separate skills for cloudflare, docker, gcloud):

1. Identify the shared workflow capability (e.g., "deploying infrastructure")
2. Create one skill with the workflow in SKILL.md
3. Move tool-specific details into reference files (one per tool)
4. The agent loads only the relevant reference for the current task
