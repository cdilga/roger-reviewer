# Archived Plan for Flywheel Stack Bootstrap

This file is an archived example from an earlier repo-local planning
localization. It is no longer the canonical target for this repository.

Current canonical planning artifacts:

- [`PLAN_FOR_ROGER_REVIEWER.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/PLAN_FOR_ROGER_REVIEWER.md)
- [`BEAD_SEED_FOR_ROGER_REVIEWER.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/BEAD_SEED_FOR_ROGER_REVIEWER.md)
- [`ALIEN_ARTEFACTS_FOR_ROGER_REVIEWER.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/ALIEN_ARTEFACTS_FOR_ROGER_REVIEWER.md)
- [`PLANNING_WORKFLOW_PROMPTS.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/PLANNING_WORKFLOW_PROMPTS.md)

The remainder of this document is preserved as historical methodology context.

## Intent

Create a swarm-ready project foundation before implementation starts. The immediate objective is not to write product code; it is to establish a clean, durable operating environment for:

- `br` / beads task management
- `bv` graph-aware task triage
- Agent Mail coordination
- `cass` session search
- `cm` procedural memory
- `ms` skill management
- `dcg` destructive command safety
- optional orchestration later with `ntm`

This plan assumes a planning-first workflow: create the markdown plan first, refine it hard, then convert the plan into beads, then launch agents.

## Current Baseline on This Machine

Verified on 2026-03-19 in `/Users/cdilga/Documents/dev/gemini-reviewer`:

- Installed: `br`, `bv`, `cass`, `cm`, `ms`, `dcg`, `claude`, `codex`, `gemini`
- Present but not a normal CLI: `agentmail`
- Not currently present: `ntm`
- Installed into Codex skills: `planning-workflow`

Verified configuration:

- `~/.claude/settings.json` already contains a `PreToolUse` hook for `dcg`
- `~/.claude/settings.json` already contains an MCP server entry for Agent Mail
- `ms doctor` passes locally

Important implication:

- This is no longer a cold-install problem
- The next phase should focus on project-local standardization, validation, and workflow scaffolding
- `ntm` is the main missing flywheel component if you want automated multi-pane swarm orchestration later

## Strategic Decision

Use the existing local machine as the primary environment.

Do not spend time reinstalling tools that are already present unless a health check fails. ACFS-style full-environment bootstrapping is only worth doing if you want:

- a clean VPS environment
- reproducible infra separate from your laptop
- one-command onboarding for future machines

For this repo, the higher-leverage path is:

1. verify existing tools
2. establish project-local conventions
3. create the markdown plan
4. convert that plan into beads
5. only then consider `ntm` and larger swarms

## Target End State

Before implementation starts, this repo should contain:

- a git repository
- a project README
- an `AGENTS.md` file that explains the toolchain and operating protocol
- a `docs/` directory with the canonical markdown plan
- a `.beads/` workspace initialized by `br`
- a project-local `.ms/` workspace for skills and indexing
- an initialized `cm` playbook or confirmed shared/global memory strategy
- explicit verification notes for `dcg`, Agent Mail, and the core CLIs
- a bead graph that fully covers the plan

## Phase 0: Freeze Scope and Operating Contract

Decide these items before touching beads:

- Project goal: what exactly this repo is for
- Tool scope: which tools are required in v1 versus optional later
- Coordination model: shared workspace, no worktrees by default
- Agent model mix: which tools do planning, which do implementation, which do review
- Safety baseline: `dcg` required for Claude-driven shell execution

Outputs for this phase:

- one paragraph project statement
- one paragraph non-goals statement
- one list of required tools
- one list of optional tools

Exit gate:

- you can explain in five sentences why each tool is in the stack

## Phase 1: Baseline Verification

Run lightweight health checks before writing project docs.

Suggested checks:

```bash
br version
bv --help
cass --version
cm --version
cm quickstart --json
ms --version
ms doctor
dcg doctor
codex --version
claude --version
gemini --version
```

Manual config checks:

- confirm `~/.claude/settings.json` still contains the `dcg` hook
- confirm `~/.claude/settings.json` still contains the Agent Mail MCP server block
- confirm Claude can actually see Agent Mail tools at runtime

If any check fails:

- fix the broken tool before proceeding
- do not create beads for tools you have not successfully verified

Exit gate:

- every required tool is either `ready`, `missing but intentionally deferred`, or `broken and explicitly blocked`

## Phase 2: Project-Local Scaffolding

Initialize the repo as the unit of coordination.

Recommended sequence:

```bash
git init
br init
ms init
cm init
```

Then create these files:

- `README.md`
- `AGENTS.md`
- `docs/PLAN_FOR_<PROJECT>.md`
- `docs/STACK_BOOTSTRAP_CHECKLIST.md`
- optionally `skills/` if you want project-local custom skills indexed by `ms`

Recommended directory shape:

```text
.
├── .beads/
├── .ms/
├── AGENTS.md
├── README.md
├── docs/
│   ├── PLAN_FOR_<PROJECT>.md
│   ├── STACK_BOOTSTRAP_CHECKLIST.md
│   └── DECISIONS.md
└── skills/
```

Project-local `ms` bootstrap:

```bash
ms config skill_paths.project '["./skills","~/.codex/skills"]'
ms index
```

Exit gate:

- repo exists
- `.beads/` exists
- `.ms/` exists
- `README.md` and `AGENTS.md` exist
- `ms index` completes successfully

## Phase 3: Memory and Skill Layer

This is where the stack becomes compounding instead of merely installed.

### `cm`

Use `cm` as the pre-task memory layer.

Initial actions:

- create or confirm starter playbook
- decide whether the repo uses only global `cm` memory or also project-specific memory rituals
- define the rule that every agent runs `cm context "<task>" --json` before non-trivial work

Good first commands:

```bash
cm starters
cm context "bootstrap flywheel stack for this repo" --json
cm onboard status
```

### `ms`

Use `ms` for durable skills, prompt packs, and eventually project-local operational knowledge.

Good first commands:

```bash
ms list
ms search "planning"
ms search "agent mail"
ms search "beads"
ms search "dcg"
```

Target outcome:

- project-local skills folder exists
- `planning-workflow` is visible to your agent environment
- later, your own repo-specific bootstrap skill can be added under `skills/`

Exit gate:

- `cm context` returns useful output
- `ms search` and `ms load` work as expected

## Phase 4: Author the Master Markdown Plan

This is the highest-leverage phase.

The plan should answer:

- what the final environment looks like
- what workflows the human and agents follow
- what each tool is responsible for
- what failure modes are unacceptable
- how safety, memory, and task routing interact
- what gets deferred to v2

Minimum sections:

- goals
- non-goals
- user workflows
- agent workflows
- tool-by-tool architecture
- repo structure
- config and secrets strategy
- validation gates
- rollout order
- risk register
- future extensions

Strong recommendation:

- use the prompt pack in `docs/PLANNING_WORKFLOW_PROMPTS.md`
- do at least 3 refinement rounds before turning anything into beads

Exit gate:

- the plan is detailed enough that a fresh agent could convert it into beads without guessing at architecture

## Phase 5: Convert Plan to Beads

Only start this after the markdown plan stabilizes.

Principles:

- every major tool setup step becomes a bead or epic
- every bead must be self-contained
- every bead must include rationale, expected output, and validation
- dependencies must reflect real blockers, not cosmetic ordering

Suggested initial epic layout:

1. Repo foundation and documentation
2. Safety and shell guardrails
3. Agent Mail validation and coordination norms
4. `cm` memory workflow
5. `ms` skill indexing and project-local skills
6. `br` and `bv` operating workflow
7. Optional `ntm` orchestration
8. Swarm launch readiness review

Validation rule:

- if a bead does not tell a fresh agent exactly how to know it is done, the bead is incomplete

Exit gate:

- `br ready` shows a sensible frontier
- `bv` can prioritize meaningful next work

## Phase 6: AGENTS.md Contract

The `AGENTS.md` file should be treated as operational law for the swarm.

It should contain:

- required first steps for every agent
- mandatory `cm context` behavior
- mandatory Agent Mail registration and inbox checking
- how to use `bv` for next-task selection
- what `dcg` protects and what not to do
- expectations around file ownership, reviews, and status updates
- compaction recovery rule: re-read `AGENTS.md`

This file matters more than people usually think. It is the recovery surface after context loss.

Exit gate:

- a fresh agent can read `AGENTS.md` and behave correctly without verbal babysitting

## Phase 7: Swarm Readiness Review

Before launching multiple agents, ask:

- do we have a converged markdown plan?
- do we have a complete bead graph?
- does `AGENTS.md` encode the operating contract?
- is Agent Mail actually reachable by the agent runtime?
- is `dcg` active where shell actions are risky?
- do `cm` and `ms` return useful context, not empty ceremony?
- do we need `ntm`, or is manual multi-pane launch enough for v1?

Only after all seven answers are "yes" or "intentionally deferred" should you start a real swarm.

## Recommended Order of Work

Given the current machine state, the best sequence is:

1. project-local scaffolding
2. `AGENTS.md`
3. master markdown plan
4. refinement rounds
5. `br` initialization and bead creation
6. `cm` and `ms` project-local tuning
7. Agent Mail runtime validation
8. optional `ntm` install and automation
9. swarm launch

## Risks and Mitigations

### Risk: Premature bead creation

If you make beads before the markdown plan is mature, the swarm will implement vague or conflicting intent.

Mitigation:

- do not create beads until the plan survives multiple review rounds

### Risk: Confusing installed with ready

A binary being present does not mean the workflow is operational.

Mitigation:

- require health checks and one manual runtime proof for each critical tool

### Risk: Too much global state, not enough project-local state

If everything lives only in global configs, the repo is not portable and agents lack context.

Mitigation:

- localize key docs, localize skills where appropriate, encode norms in `AGENTS.md`

### Risk: Agent Mail configured but not exercised

Config drift is common.

Mitigation:

- perform one real messaging smoke test before swarm launch

### Risk: Memory systems become empty ritual

`cm` and `ms` can become decorative if nobody uses them at task boundaries.

Mitigation:

- make `cm context` and `ms search/load` part of the agent start protocol

### Risk: Safety gaps outside Claude

`dcg` protects Claude shell execution; other agents may not inherit the same controls automatically.

Mitigation:

- document tool-specific safety assumptions in `AGENTS.md`
- keep destructive operations manual until equivalent safeguards are confirmed

## Optional Phase: `ntm`

Install `ntm` only after the manual workflow works cleanly.

Why defer:

- orchestration automation hides problems if your beads, `AGENTS.md`, or Agent Mail assumptions are still wrong

When to add it:

- after you can run a small swarm manually and the coordination pattern is stable

## Definition of Done for This Planning Stage

This planning stage is complete when:

- the repo has scaffold files and local tool workspaces
- the master markdown plan exists and has survived multiple refinement rounds
- the bead graph exists and has meaningful dependencies
- `AGENTS.md` encodes the operating contract
- Agent Mail, `cm`, `ms`, and `dcg` are all verified in practice
- you can confidently start implementation without inventing system behavior on the fly
