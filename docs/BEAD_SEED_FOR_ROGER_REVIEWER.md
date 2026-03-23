# Bead Seed for Roger Reviewer

This document is the pre-`br` seed structure derived from
[`PLAN_FOR_ROGER_REVIEWER.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/PLAN_FOR_ROGER_REVIEWER.md).
It is intentionally still in markdown so the architecture can survive one more
review pass before bead import.

## Epic 1: Repo Foundation

### 1.1 Create package and app layout

- Objective: establish the monorepo structure for CLI, TUI, extension, and
  shared packages.
- Depends on: none.
- Acceptance: directories and workspace configuration exist; shared package
  boundaries are explicit.

### 1.2 Define coding and operating docs

- Objective: add README, AGENTS, and any initial architecture decision records.
- Depends on: 1.1.
- Acceptance: a fresh contributor can understand product intent, repo layout,
  and core constraints.

### 1.3 Run architecture risk spikes and ADRs

- Objective: reduce uncertainty around the OpenCode boundary, browser launch
  bridge, and artifact storage strategy before implementation fans out.
- Depends on: 1.2.
- Acceptance: spike outcomes are documented and any package-shaping decisions
  are captured in ADRs.

## Epic 2: Domain and Storage

### 2.1 Define core domain schema

- Objective: specify the first-class entities for sessions, runs, findings,
  fingerprints, artifacts, outbound drafts, posted actions, and config layers.
- Depends on: 1.1.
- Acceptance: schema and invariants are documented and represented in code.

### 2.1.1 Define finding fingerprint and state model

- Objective: prevent refresh noise by defining how findings persist or change
  across reruns.
- Depends on: 2.1.
- Acceptance: triage state and outbound state are both explicit, and refresh can
  classify findings predictably.

### 2.2 Implement local storage and migrations

- Objective: create the SQLite-backed persistence layer and migration flow.
- Depends on: 2.1.
- Acceptance: review sessions and findings can be created, queried, and migrated
  safely.

### 2.3 Add full-text search

- Objective: make findings and review artifacts searchable locally with low
  latency.
- Depends on: 2.2.
- Acceptance: search returns relevant results across test data and survives
  restarts.

## Epic 3: OpenCode Session Orchestration

### 3.1 Define Roger-to-OpenCode linkage

- Objective: decide the exact mapping between Roger sessions and underlying
  OpenCode sessions.
- Depends on: 1.3, 2.1.
- Acceptance: fallback and resume rules are explicit and testable.

### 3.2 Implement session persistence and resume

- Objective: persist review metadata while keeping OpenCode session continuity.
- Depends on: 2.2, 3.1.
- Acceptance: a review can be resumed from Roger and from plain OpenCode.

### 3.3 Add compaction recovery support

- Objective: reinsert the minimum high-signal context after compaction or reload.
- Depends on: 3.2.
- Acceptance: selected artifacts and prior findings can be restored into an
  active session reliably.

## Epic 4: Prompt Pipeline and Review Engine

### 4.1 Encode staged review prompts

- Objective: implement exploration, deep-review, and optional follow-up passes
  as structured stages.
- Depends on: 3.2.
- Acceptance: stages can run sequentially and persist outputs independently.

### 4.2 Persist structured findings

- Objective: convert review outputs into findings with explicit states and
  evidence links.
- Depends on: 2.2, 4.1.
- Acceptance: findings are queryable, mutable, and linked to sessions and
  artifacts.

### 4.3 Implement refresh behavior

- Objective: support fresh-eyes reruns after new commits while carrying forward
  relevant prior findings.
- Depends on: 2.1.1, 4.2.
- Acceptance: refresh updates findings predictably instead of duplicating noise.

## Epic 5: Session-Aware CLI

### 5.1 Implement `rr review` and `rr resume`

- Objective: start and resume reviews from the shell using repo context
  inference where possible.
- Depends on: 3.2, 4.2.
- Acceptance: a user can initiate and resume a usable review workflow without
  the extension.

### 5.2 Implement `rr findings`, `rr status`, and `rr refresh`

- Objective: expose core review state and refresh behavior through the CLI.
- Depends on: 5.1, 4.3.
- Acceptance: core review operations can be driven entirely from CLI.

## Epic 6: TUI

### 6.1 Build the review shell

- Objective: create the main TUI frame, session selector, and current review
  overview.
- Depends on: 5.1.
- Acceptance: users can enter and navigate an active review session.

### 6.2 Build the findings workflow

- Objective: implement itemized findings list, detail panel, and state
  transitions.
- Depends on: 4.2, 6.1.
- Acceptance: findings can be triaged quickly and accurately from the TUI.

### 6.3 Add outbound draft approval

- Objective: let users inspect, edit, and approve proposed GitHub outputs.
- Depends on: 6.2, 8.1.
- Acceptance: nothing is posted until the approval step is completed.

## Epic 7: GitHub Integration and Extension

### 7.1 Implement GitHub adapter

- Objective: fetch PR context and prepare outbound actions through explicit
  adapter boundaries.
- Depends on: 1.3, 5.1.
- Acceptance: Roger can resolve a PR target and prepare local outbound drafts.

### 7.2 Validate daemonless browser bridge

- Objective: prove the extension can launch or resume Roger locally without
  introducing a persistent daemon as the system center.
- Depends on: 1.3, 7.1.
- Acceptance: a browser-initiated launch path and a fallback path both work,
  and the v1 status story is explicitly bounded.

### 7.3 Implement extension UI

- Objective: inject PR actions and status into GitHub pages.
- Depends on: 7.2.
- Acceptance: users can start or resume reviews from GitHub and see unresolved
  findings status.

## Epic 8: Approval and Posting Flow

### 8.1 Model outbound drafts

- Objective: define the local representation for comments, questions, and
  suggestions awaiting approval.
- Depends on: 2.1, 4.2, 7.1.
- Acceptance: outbound drafts are linked to findings and tracked locally.

### 8.1.1 Snapshot posted actions

- Objective: preserve an audit trail for the exact GitHub payload that was
  approved and posted.
- Depends on: 8.1.
- Acceptance: local state records the posted payload, outcome, and remote
  identifier.

### 8.2 Implement explicit posting flow

- Objective: post approved drafts back to GitHub only after confirmation.
- Depends on: 8.1.
- Acceptance: outbound actions are auditable and cannot run accidentally.

## Epic 9: Worktrees and Named Instances

### 9.1 Implement worktree preparation

- Objective: create isolated local review environments when needed.
- Depends on: 5.1.
- Acceptance: a review can prepare and track its worktree context safely.

### 9.2 Implement named-instance storage strategy

- Objective: support multiple local Roger instances with conservative state
  sharing.
- Depends on: 2.2, 9.1.
- Acceptance: two local instances can coexist without corrupting one another.

## Epic 10: Search, Memory, and Skills

### 10.1 Add prior-review lookup

- Objective: surface related historical findings during review and refresh.
- Depends on: 2.3, 4.3.
- Acceptance: refresh flows can pull in prior high-signal findings quickly.

### 10.2 Add semantic search layer

- Objective: improve recall once the base search path is stable.
- Depends on: 10.1.
- Acceptance: semantic search improves practical retrieval quality without
  compromising responsiveness.

### 10.3 Capture failure patterns into reusable skills

- Objective: turn repeated review or suggestion failures into explicit prompts or
  skills.
- Depends on: 10.1.
- Acceptance: at least one failure pattern can be encoded and reused.

## Epic 11: Safety and Validation

### 11.1 Enforce review-safe defaults

- Objective: ensure review mode stays read-mostly and approval-gated.
- Depends on: 4.2, 8.1.
- Acceptance: posting and mutation paths are visibly elevated and test-covered.

### 11.2 Add end-to-end validation matrix

- Objective: cover CLI launch, TUI review, GitHub launch, refresh, posting, and
  OpenCode fallback.
- Depends on: 5.2, 6.3, 7.3, 8.2.
- Acceptance: the main user workflows are executable and repeatable.

### 11.3 Validate refresh identity behavior

- Objective: ensure reruns classify old and new findings correctly instead of
  duplicating or losing them.
- Depends on: 2.1.1, 4.3.
- Acceptance: test scenarios cover carried-forward, resolved, superseded, and
  stale findings.

## Critical Dependency Spine

The likely critical path for v1 is:

1. repo foundation
2. domain schema
3. storage
4. OpenCode linkage
5. prompt pipeline
6. structured findings
7. session-aware CLI
8. TUI findings workflow
9. outbound draft model
10. explicit posting flow
11. GitHub adapter
12. extension bridge and UI

This order keeps the extension off the critical path until the local review core
is real.
