# Repo Onboarding and Discovery Prompts

Status: reusable workflow prompt pack. This document is for studying an
existing repo before planning, implementation, or adversarial review begins.
It is intentionally more generic than
[`PLANNING_WORKFLOW_PROMPTS.md`](./PLANNING_WORKFLOW_PROMPTS.md)
so it can be adapted for other repositories.

Use this when the problem is not "write the final plan from scratch" but
"understand what already exists, what is authoritative, what is broken or
unclear, and what questions must be resolved before planning or implementation
can proceed well."

## Why this exists

The standard planning workflow assumes you already understand the repo well
enough to produce a strong plan. In practice, many projects need an explicit
repeatable discovery phase first.

This workflow is for:

- onboarding to an unfamiliar repo
- re-onboarding after a long gap
- handing a repo to a fresh agent or a fresh machine
- creating a reliable current-state brief before planning
- preventing agents from free-associating from stale docs or old critique rounds

## Recommended outputs

The discovery flow should usually produce:

- a current-state brief
- an authority map for docs and commands
- a list of validated tooling and environment assumptions
- a list of unresolved questions
- a recommendation on whether the repo is ready for planning, review, or implementation

## Prompt 1: Discovery and Current-State Brief

Use this first. It is for understanding the repo as it exists today.

```text
Study this repository deeply and produce a current-state onboarding and discovery brief.

Your job is not to brainstorm a new architecture yet. Your job is to understand
what already exists, what is authoritative, what is stale, what is missing, and
what would confuse a fresh agent or developer entering this repo.

Please inspect:
- AGENTS.md or equivalent repo operating docs
- README and onboarding docs
- planning docs, architecture docs, ADRs, and critique/revision docs
- task-tracking or issue-graph artifacts if present
- package layout, app layout, and exploration/reference folders
- available tooling, CLIs, MCP integrations, and local skills if relevant
- test setup, smoke tests, and validation commands if present

Deliver:
1. Repo purpose and current maturity level
2. Canonical document hierarchy and reading order
3. Current architecture or planned architecture
4. Tooling and environment assumptions that appear required
5. What has already been decided
6. What remains unresolved
7. Risks, inconsistencies, stale docs, or likely confusion points
8. Recommended next step:
   - continue discovery
   - move into planning
   - run adversarial review
   - polish task graph
   - begin implementation

Important constraints:
- distinguish observed facts from your own recommendations
- prefer the current canonical docs over historical revision artifacts
- call out conflicts explicitly rather than averaging them together
- do not silently upgrade historical docs into current truth

Repo-specific context:
<PASTE ANY PROJECT-SPECIFIC CONTEXT HERE>
```

## Prompt 2: Discovery Critique and Gap Analysis

Use this after Prompt 1 when you want a frontier model to challenge the current
discovery brief rather than the final plan.

```text
Carefully review this repository discovery brief and current-state summary.

Your task is to attack it constructively:
- find missing assumptions
- find stale or conflicting authority boundaries
- find underexplored workflows
- find tooling or environment risks
- find ambiguous architecture seams
- find missing onboarding or repeatability steps

For each major issue:
1. explain what is missing, weak, or likely wrong
2. explain why it matters in practice
3. propose a concrete revision to the discovery brief, onboarding workflow, or
   authority model

Focus especially on:
- whether a fresh agent would know what to read first
- whether canonical versus historical docs are clearly separated
- whether the repo’s commands, tools, and validation steps are actually repeatable
- whether the current state is clear enough to support planning or implementation
- whether there are missing prompts or playbooks for future reuse in other repos

<PASTE THE COMPLETE DISCOVERY BRIEF HERE>
```

## Prompt 3: Discovery Integration and Canonicalization

Use this in Codex or Claude Code after getting a critique of the discovery
brief.

```text
Integrate these discovery and onboarding revisions into the repository docs in-place.

Priorities:
- strengthen the canonical reading order
- mark historical docs clearly
- make onboarding and repeatable discovery less ambiguous
- preserve the real current state instead of flattening disagreement
- keep the repo-specific docs concise but operationally useful

Please update the relevant files and then summarize:
1. what changed
2. what still remains unresolved
3. whether the repo is now ready for the next step

Here is the critique output:

<PASTE DISCOVERY CRITIQUE OUTPUT HERE>
```

## Optional Prompt 4: Repo-to-Repo Reuse Adaptation

Use this when you want to adapt the workflow to a different repo while keeping
the same shape.

```text
Adapt this onboarding and discovery workflow to a different repository.

Preserve the same overall structure:
- current-state discovery
- critique and gap analysis
- integration and canonicalization

But rewrite the prompts so they fit this new repo’s shape, tooling, and likely
confusion points.

Please produce:
1. a repo-specific prompt pack
2. a short note on what is universal versus repo-specific
3. any placeholders that should remain customizable

Source workflow:
<PASTE THE EXISTING PROMPT PACK HERE>

Target repo context:
<PASTE THE TARGET REPO CONTEXT HERE>
```

## Practical guidance

For most repos, the right sequence is:

1. run Prompt 1 to establish the current-state brief
2. run Prompt 2 if the repo is complex, stale, or politically messy
3. use Prompt 3 to fix the authority model and onboarding docs
4. only then move into the normal planning or adversarial review loop

This keeps discovery separate from planning. That separation matters because a
bad discovery pass contaminates everything downstream: planning, beads,
implementation, and review.

## Minimal 3-Prompt Frontier Workflow

If you prefer the much simpler "strong AGENTS.md plus three prompts" model for
frontier web models such as Gemini, use this shape instead.

This works best when:

- the repo already has a strong `AGENTS.md`
- `README.md` clearly explains the repo purpose
- the repo has real code to inspect, not only planning docs
- you want a reusable lightweight workflow instead of a larger prompt pack

Important note:

- Prompt 1 is broadly useful for any repo
- Prompts 2 and 3 are mainly for implementation repos with real code
- in a planning-only repo, Prompt 1 is the main one that matters

### Prompt A: Mandatory Onboarding and Architecture Understanding

```text
First read ALL of the `AGENTS.md` file and `README.md` file super carefully and
understand ALL of both.

Then use your code investigation mode to fully understand the code, technical
architecture, workflows, and purpose of the project.

Important constraints:
- obey ALL instructions in `AGENTS.md`
- treat `AGENTS.md` and the canonical docs it names as authoritative
- do not skip straight to coding or brainstorming
- identify which docs are canonical, which are historical, and which are only
  supporting context

Please deliver:
1. what this repo is for
2. what the current architecture is
3. what the current maturity/status is
4. what is already decided
5. what is still unresolved
6. any obvious documentation, tooling, or workflow confusion points
```

### Prompt B: Fresh-Eyes Deep Code Exploration and Review Sweep

```text
I want you to sort of randomly explore the code files in this project, choosing
code files to deeply investigate and understand and trace their functionality
and execution flows through the related code files which they import or which
they are imported by.

Once you understand the purpose of the code in the larger context of the
workflows, I want you to do a super careful, methodical, and critical check
with fresh eyes to find any obvious bugs, problems, errors, issues, silly
mistakes, or reliability problems.

Default mode is review and diagnosis, not automatic fixing. Produce a
prioritized set of findings, root-cause explanations, and recommended next
actions. If you believe something should be changed, describe the proposed fix
or diff shape, but do not make code changes unless the user explicitly tells
you to switch into a fix-oriented mode.

Important constraints:
- comply with ALL rules in `AGENTS.md`
- do not write code, mutate state, or post externally unless explicitly asked
- do not make shallow changes based on isolated snippets; understand the wider
  execution path first
- explain the root cause of each important issue you identify
- present findings first, ordered by severity or importance
```

### Prompt C: Broad Code Review of Prior Agent Work

```text
Now turn your attention to reviewing the code written by your fellow agents and
checking for any issues, bugs, errors, problems, inefficiencies, security
problems, or reliability issues.

Do not restrict yourself to the latest commits. Cast a wider net and go super
deep.

For each important issue:
1. diagnose the root cause using first-principle analysis
2. explain why it matters
3. recommend the safest revision path without making changes by default

Important constraints:
- comply with ALL rules in `AGENTS.md`
- prefer substantive findings over superficial churn
- do not assume the newest code is the only risky code
- keep the review grounded in actual workflows and failure modes
- default output is review findings, not code edits
- if the repo has approval gates or review-only rules, respect them explicitly
```

### Roger-specific compliance adjustments

To make the minimal 3-prompt workflow Roger-compliant, apply these defaults:

- treat review and diagnosis as the default mode; do not auto-fix
- do not make code changes, config changes, or environment mutations unless the
  user explicitly asks for a fixing pass
- do not post to GitHub or any external system
- if a repo is still in planning, focus on docs, authority boundaries, task
  graph, and architecture questions rather than pretending there is code to fix
- if you recommend changes, present findings, rationale, and proposed fix shape
  first; only execute edits in a separate explicitly approved step
- if the repo uses issue graph tools such as `br` or `bv`, inspect current work
  state as part of discovery rather than guessing what is current
- when reviewing prior agent work, prefer findings-first output ordered by
  severity, then open questions, then optional remediation proposals
- preserve the canonical-vs-historical doc split defined in `AGENTS.md`

### When to use which workflow

Use the minimal 3-prompt workflow when:

- the repo already has a strong AGENTS contract
- you mainly want investigation, bug-finding, and broad review
- you do not need a full planning artifact pack yet

Use the fuller discovery/critique/integration workflow above when:

- the repo is ambiguous or stale
- the authority model is weak
- planning docs need canonicalization
- you want a durable onboarding/discovery brief that can be reused by many
  agents over time
