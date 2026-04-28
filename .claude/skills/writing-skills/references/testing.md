# Testing & Iterating Skills

## Contents
- [The Agent A/B Method](#the-agent-ab-method)
- [What to Watch For](#what-to-watch-for)
- [Evaluation-Driven Development](#evaluation-driven-development)
- [Common Iteration Patterns](#common-iteration-patterns)
- [Example Test Session](#example-test-session)
- [Cross-Platform Testing Notes](#cross-platform-testing-notes)

## The Agent A/B Method

Effective skill development uses two agent sessions in alternation:

**Agent A** (the author session) — helps design, write, and refine the skill. Understands agent instruction patterns and can reason about what another session needs.

**Agent B** (the test session) — a fresh session with the skill loaded, testing it on real tasks. Reveals gaps through actual behavior, not assumptions.

### Workflow

1. **Complete a task without the skill first.** Work through the problem with Agent A using normal prompting. Notice what context you repeatedly provide — this is what the skill should capture.

2. **Have Agent A draft the skill.** It understands the skill format natively. Focus the review on conciseness: "Remove the explanation about X — the agent already knows that."

3. **Test with Agent B on real tasks.** Not test scenarios — actual work. Observe:
   - Does it find the right reference files?
   - Does it apply the rules correctly?
   - Does it miss anything?
   - Does it load unnecessary context?

4. **Bring observations back to Agent A.** Be specific: "Agent B forgot to filter test accounts even though the skill mentions it. Maybe it's not prominent enough?"

5. **Iterate.** Each cycle improves the skill based on observed behavior, not assumptions.

## What to Watch For

- **Unexpected exploration paths**: The agent reads files in an order you didn't anticipate → structure isn't intuitive
- **Missed references**: The agent doesn't follow links to important files → links need to be more explicit
- **Overreliance on one section**: The agent reads the same file repeatedly → that content should be in SKILL.md
- **Ignored files**: The agent never accesses a bundled file → it's unnecessary or poorly signaled
- **Context overflow**: Tasks that used to work start degrading → skill has grown too large

## Evaluation-Driven Development

1. **Identify gaps**: Run the agent on representative tasks with no skill. Document failures.
2. **Create evaluations**: Build 3+ scenarios that test these gaps.
3. **Establish baseline**: Measure performance without the skill.
4. **Write minimal instructions**: Just enough to address the gaps and pass evaluations.
5. **Iterate**: Run evaluations, compare against baseline, refine.

## Common Iteration Patterns

**Skill triggers but output is wrong** → The body instructions need refinement. Check if guidance is too vague or too prescriptive.

**Skill doesn't trigger when it should** → The description is missing key terms or trigger contexts. Add them.

**Skill triggers when it shouldn't** → The description is too broad. Narrow the scope with specific conditions.

**Output quality varies by model** → Test with smaller and larger models. What works for frontier models may need more detail for smaller ones. Aim for instructions that work across all target models.

**Skill works but fills too much context** → SKILL.md is too large. Move content to reference files. Target ~200 lines for the entry point.

## Example Test Session

### Round 1: Test with Agent B

Prompt to Agent B (fresh session with skill loaded):

> "Create a skill for processing CSV data in Python. The skill should help agents clean, validate, and transform CSV files."

Observe:
- Agent B wrote a 400-line SKILL.md explaining what pandas is → **documentation dump**
- Used passive voice throughout ("You can use pandas to...") → **passive voice default**
- Listed 4 CSV libraries with pros/cons → **multiple options instead of one default**
- No reference files — everything flat → **flat structure default**
- Did identify the right workflows (cleaning, validation, transform) → **good, keep this**

### Round 2: Report to Agent A

Prompt to Agent A:

> "Agent B wrote a 400-line skill that explains what pandas is, lists 4 libraries, and uses passive voice. It did correctly identify 3 workflows. The skill's defaults-first principle wasn't followed — there's no section identifying what the agent does wrong by default with CSV processing. Fix these issues."

Agent A produces a 60-line SKILL.md with a "Defaults to Override" section and 3 reference files for the workflows.

### Round 3: Retest

Test the revised skill with Agent B on the same task. Compare: Is the output shorter? Does it lead with defaults? Does it use imperative form? Each round narrows the gap between intended and actual behavior.

## Cross-Platform Testing Notes

When writing skills intended for use across multiple agent platforms:

**"Fresh session" varies by platform.** In Claude Code, start a new conversation. In Cursor, open a new composer. In VS Code Copilot, start a new chat. The key requirement: the agent has no memory of prior iterations, only the skill file.

**Verify skill loading.** Not all platforms load SKILL.md the same way. Confirm the agent actually sees and applies the skill by asking it to summarize what instructions it has. If the skill isn't loading, check the platform's documentation for the correct file path and format.

**Model capability variance.** Frontier models (GPT-4o, Claude Opus, Gemini Ultra) handle terse, principle-based instructions well. Smaller models may need more explicit guidance. If targeting broad compatibility, test with at least one smaller model and consider whether key instructions need more detail.

**Test with at least two different agents.** A skill that works perfectly on one platform but fails on another isn't portable. Testing with two agents catches language or assumptions that are platform-specific.

**Watch for platform-specific tool assumptions.** Some skills reference tool names or capabilities that only exist on one platform. Use generic descriptions ("read the file", "search the codebase") rather than platform-specific tool names in the skill body.
