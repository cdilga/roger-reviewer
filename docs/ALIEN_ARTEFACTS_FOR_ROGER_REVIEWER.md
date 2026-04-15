# Alien Artefacts for Roger Reviewer

This file is a compact planning packet for external model critique rounds. It
is deliberately smaller and sharper than the full
[`PLAN_FOR_ROGER_REVIEWER.md`](./PLAN_FOR_ROGER_REVIEWER.md),
while still carrying the high-signal constraints that should not be lost when
another model reviews the project.

## Artefact 1: Product Brief

Roger Reviewer is a local-first PR review system with three surfaces:

- a session-aware CLI
- a TUI-first findings workflow
- a GitHub browser extension for launch/resume/status

It wraps, but does not replace, an underlying OpenCode session. Every review
must remain resumable in plain OpenCode if Roger-specific layers are unavailable
or compacted.

The product is review-first, not auto-fix-first. Its primary job is to generate
high-quality findings, organize them durably, and route approved outputs back to
GitHub only after human confirmation.

## Artefact 2: Hard Constraints

- local state is the source of truth
- no long-running daemon should become the architectural center
- the browser extension cannot become the canonical state owner
- findings must be structured objects, not loose transcript snippets
- nothing posts to GitHub without explicit approval
- bug-fixing behavior is out of scope by default
- writes to dev/test environments should be disabled by default
- config must be additive and inspectable

## Artefact 3: Minimum Viable v1

The smallest useful v1 should include:

- local review session persistence
- explicit linkage to underlying OpenCode sessions
- staged review prompt execution
- structured finding capture and state transitions
- a session-aware CLI
- a usable TUI for triage and approval
- local draft preparation for GitHub outputs

The browser extension is important, but it should not block proving the core
local review loop first.

## Artefact 4: Architecture Thesis

Use a shared application core with ports-and-adapters boundaries:

- review domain
- storage and indexing
- OpenCode session adapter
- GitHub adapter
- CLI surface
- TUI surface
- browser extension surface

The domain layer owns findings, review runs, prompt stages, approval state, and
reconciliation logic. UI layers should be thin.

## Artefact 5: Critical Risks to Challenge

These are the areas an external model should attack hardest:

- whether the extension-to-local bridge can stay truly daemonless
- whether OpenCode fallback is concrete or only aspirational
- whether the proposed data model is rich enough for finding lifecycle tracking
- whether semantic search is being overvalued relative to FTS-backed v1 search
- whether worktree and named-instance sync is prematurely complex
- whether automatic reconciliation and fresh-eyes passes can remain coherent as
  findings evolve

## Artefact 6: Open Questions That Should Stay Explicit

- What exact runtime and component model does FrankenTUI expect?
- What is the best browser-to-local launch mechanism on the target platforms?
- What do the brain-dump terms `FPs` and `SA` refer to operationally?
- What is the stable integration boundary with OpenCode?
- Which credential actions need Keychain in v1 rather than later?

These are open questions, not places to bluff with fake certainty.

## Artefact 7: ADR Candidates

The next architecture decision records should likely cover:

- ADR 1: shared-language/runtime choice
- ADR 2: OpenCode session boundary
- ADR 3: browser extension launch bridge
- ADR 4: SQLite/FTS baseline and semantic-search deferral
- ADR 5: outbound approval and posting model
- ADR 6: worktree and named-instance isolation strategy

## Artefact 8: Review Directions for Frontier Models

When another model critiques this project, it should focus on:

- better architecture
- sharper rollout order
- hidden blockers
- under-modeled user workflows
- missing validation gates
- unsafe approval or mutation paths
- where the plan is pretending an uncertainty is already solved

## Artefact 9: What `cass` Contributed

I used the Dicklesworthstone `cass` skill to mine prior agent history for
similar patterns around TUI-led local tooling, findings workflows, and
browser-to-local launch patterns.

Current result:

- the history index is healthy after refresh
- directly analogous prior sessions are limited
- the most useful takeaway is methodological rather than product-specific:
  keep the local interactive surface primary, keep risky control-plane behavior
  explicit, and avoid drifting into hidden background services

This means the current plan is grounded mainly in the brain dump plus the
planning workflow, not in a rich body of exact prior-art matches.

## Artefact 10: Next Loop Prompt

Use this compact prompt when you want a fast external critique pass:

```text
Review this Roger Reviewer planning packet critically. I do not want vague praise. I want the strongest possible objections, revisions, and architecture improvements.

Focus on:
- the daemonless requirement
- the browser extension to local-app bridge
- OpenCode fallback realism
- structured finding lifecycle
- approval and posting safety
- rollout order and hidden blockers

For each proposed change, explain the problem, your reasoning, and the git-diff style change you would make to the canonical plan.

<PASTE ALIEN_ARTEFACTS_FOR_ROGER_REVIEWER.md AND/OR PLAN_FOR_ROGER_REVIEWER.md HERE>
```
