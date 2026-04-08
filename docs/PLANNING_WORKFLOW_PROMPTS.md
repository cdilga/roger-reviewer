# Planning Workflow Prompts

These prompts adapt the local planning workflow to `Roger Reviewer`, using
[`roger-reviewer-brain-dump.md`](/Users/cdilga/Documents/dev/roger-reviewer/roger-reviewer-brain-dump.md)
as the raw source document and
[`PLAN_FOR_ROGER_REVIEWER.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/PLAN_FOR_ROGER_REVIEWER.md)
as the canonical plan. Use
[`ALIEN_ARTEFACTS_FOR_ROGER_REVIEWER.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/ALIEN_ARTEFACTS_FOR_ROGER_REVIEWER.md)
when you want a smaller artifact pack for external critique rounds.

## Method Summary

The local planning pack points to the same core loop:

1. write a complete markdown plan before implementation
2. run multiple critique and integration rounds until the plan stabilizes
3. convert the stabilized plan into a granular bead graph
4. only then start implementation agents

Additional Roger-specific planning rule:

- during new feature creation and plan shaping, explicitly ask which user
  actions are truly required versus which can be inferred safely from Roger's
  local state
- surface that question early in the plan and bead graph rather than discovering
  it late during implementation polish
- inference is allowed only for read-safe and launch-safe flows; approval,
  posting, code mutation, and other elevated actions remain explicit

The key adaptation for this repo is that the old flywheel bootstrap example is
no longer the target problem. The actual product problem is Roger Reviewer: a
local-first PR review system spanning a TUI, session-aware CLI, and GitHub
browser extension.

## 1. Initial Plan Prompt

Use this if you want a frontier model to regenerate the plan from scratch.

```text
I want to build Roger Reviewer, a local-first pull request review system.

Core product intent:
- TUI-first review experience using FrankenTUI for the main interface
- backend/session layer that is a drop-in wrapper over an OpenCode session, while always preserving the ability to resume in plain OpenCode
- Chrome/Brave GitHub extension that injects rich PR actions and can launch or reconnect local review workflows
- shared architecture that can power the TUI, extension, and session-aware CLI without introducing a long-running daemon
- local storage using a SQLite-family database with extremely fast lookup of reviews, findings, PR artifacts, and cached context
- semantic and keyword search over prior reviews and indexed PR data
- review workflow that explores first, then goes deep, can recurse through multiple review passes, and only stops when there is no more value
- findings-centric UX with approval, ignore, follow-up, and “ask questions in GitHub” states
- additive configuration model with global templates plus repo-specific overlays
- isolated worktree-based execution, named instances, and safe local environment handling

Important constraints:
- Default mode is review and recommendation, not automatically fixing bugs
- Posting comments or suggestions back to GitHub must require explicit approval
- Writes to dev/test environments should be disabled by default
- Architecture should stay tool-agnostic and daemonless
- Resume and compaction recovery must be strong enough that local context can be reinserted as needed
- Undefined or domain-specific items from the brain dump, such as “FPs” and “SA,” should be preserved as open questions rather than hand-waved away

Please create a detailed markdown plan that covers:
- goals
- non-goals
- user workflows
- agent workflows
- system architecture
- package/repo structure
- storage and indexing strategy
- session model
- extension integration strategy
- safety and approval model
- config layering
- rollout phases
- validation gates
- risk register
- open questions
- plan-to-beads conversion strategy

Also include an explicit section or subsection answering:
- which user actions should remain explicit
- which actions or choices Roger can infer safely
- where reducing clicks is a primary UX goal
- where click reduction must yield to safety or ambiguity

Make it self-contained, specific, realistic, and explicit about tradeoffs.
```

## 2. Plan Review Prompt

Use this after the first real plan exists.

```text
Carefully review this entire Roger Reviewer plan and propose your strongest revisions. Focus on architecture, execution order, local-first UX, safety, GitHub integration, storage design, session durability, and any missing workflows or risks.

For each proposed change:
1. explain the reasoning clearly
2. explain what problem it solves
3. provide git-diff style changes relative to the original markdown plan

Pay special attention to:
- whether the daemonless requirement is actually satisfied
- whether the extension-to-local-app bridge is realistic
- whether the OpenCode fallback is preserved
- whether the rollout order properly defers risky integrations until after the core review loop works
- whether the plan turns fuzzy brain-dump ideas into explicit decisions versus open questions

<PASTE THE COMPLETE PLAN HERE>
```

## 3. Integration Prompt

Use this in Codex or Claude Code after getting a review output.

```text
Integrate these Roger Reviewer planning revisions into the canonical markdown plan in-place. Be meticulous. Preserve the strongest ideas, reject weak or redundant ones, and tighten anything that is still ambiguous.

At the end, tell me:
1. which changes you strongly agree with
2. which changes you only partially agree with
3. which changes you reject and why

Here is the review output:

<PASTE REVIEW OUTPUT HERE>
```

## 4. Multi-Model Blend Prompt

Use this once you have competing full plans or competing reviews.

```text
I asked multiple frontier models to propose architecture and rollout plans for Roger Reviewer. Compare them honestly, identify where each one is stronger, and then produce the strongest possible hybrid version of the plan.

The merged plan should remain:
- local-first
- daemonless in steady state
- TUI-first but not TUI-only
- compatible with a GitHub browser extension
- compatible with plain OpenCode fallback
- explicit about approval gates and safety defaults
- explicit about which items are true decisions versus open questions

Please provide:
1. a concise comparative analysis
2. git-diff style changes against the base plan
3. any new sections that should be added to the final plan

<PASTE THE COMPETING PLANS OR REVIEWS HERE>
```

## 5. Plan-to-Beads Prompt

Use this only once the markdown plan has stabilized.

```text
Convert this entire Roger Reviewer markdown plan into a comprehensive and granular bead graph. Every bead must be self-contained so that a fresh agent can execute it without rereading the full plan.

For each bead include:
- objective
- rationale
- dependencies
- acceptance criteria
- validation or smoke-test steps
- v1 versus later status

Seed the graph around epics for:
- repo and package scaffolding
- shared domain and storage schema
- OpenCode session orchestration
- prompt pipeline and review engine
- TUI shell and findings workflow
- GitHub extension bridge
- worktree and named-instance management
- approval and posting flow
- semantic search and memory hooks
- safety and policy enforcement

If the environment is ready, use `br` to create the beads. Otherwise, emit the bead structure in markdown first.
```

## 6. Bead Polishing Prompt

Use this after the first bead import.

```text
Review the Roger Reviewer bead graph carefully. Improve any bead that is underspecified, overbroad, or missing validation, dependencies, rationale, or policy constraints.

Be especially critical about:
- hidden blockers between storage, session orchestration, TUI, and extension work
- places where posting or mutation could accidentally happen before approval
- places where the architecture drifts away from the daemonless/local-first requirement
- missing smoke tests for OpenCode fallback and resume behavior
- places where the current plan still forces unnecessary user clicks even though
  Roger already has enough state to infer the next safe step
```

## 7. Readiness Review Prompt

Use this before launching implementation work.

```text
Assess whether Roger Reviewer is actually ready to move from planning into implementation.

Please evaluate:
- whether the markdown plan is complete and internally consistent
- whether the bead graph fully covers the plan
- whether open questions have been isolated enough that they will not block early implementation
- whether the rollout order is realistic
- whether the safety and approval model is precise enough to avoid accidental GitHub writes or local environment mutations
- whether the first implementation slice can be built without needing the browser extension immediately

If the answer is “not ready,” list the missing pieces in priority order.
```
