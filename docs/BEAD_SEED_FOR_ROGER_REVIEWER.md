# Bead Seed for Roger Reviewer

This document is the markdown seed structure derived from
[`PLAN_FOR_ROGER_REVIEWER.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/PLAN_FOR_ROGER_REVIEWER.md).
The live beads workspace already exists under `.beads/`; this file must stay
aligned with that execution layer rather than acting like a stale pre-import
draft.

## Epic 1: Repo Foundation

### 1.1 Create package and app layout

- Objective: establish the monorepo structure for CLI, TUI, extension, and
  shared packages.
- Depends on: none.
- Acceptance: directories and workspace configuration exist; shared package
  boundaries are explicit; the local runtime is Rust-first and the browser
  extension is treated as a narrow browser-native exception.

### 1.2 Define coding and operating docs

- Objective: add README, AGENTS, and any initial architecture decision records.
- Depends on: 1.1.
- Acceptance: a fresh contributor can understand product intent, repo layout,
  and core constraints.

### 1.3 Run architecture risk spikes and ADRs

- Objective: reduce uncertainty around the supported-harness boundary with
  OpenCode primary and bounded live-CLI providers, the browser launch bridge,
  and the artifact storage strategy before implementation fans out.
- Depends on: 1.2.
- Acceptance: spike outcomes are documented and any package-shaping decisions
  are captured in ADRs, including the Rust-first local runtime stance and the
  extension dependency policy; the companion-binary target matrix and packaging
  responsibilities are explicit enough to hand off to a release/devops workstream later.

### 1.3.1 Define release artifact matrix and CI/CD ownership

- Objective: specify the supported local product targets, release artifact
  shapes, checksum/signing expectations, and which CI/CD jobs own build,
  packaging, and publication.
- Depends on: 1.3.
- Acceptance: macOS, Windows, and Linux artifact expectations are explicit;
  release publication is not an ad hoc manual process; browser-extension
  packaging is recognized as separate optional release work.

### 1.3.2 Define one-line install/update flow for the local product

- Objective: specify a simple artifact-driven install/update mechanism based on
  GitHub releases for the Roger CLI/local companion surface.
- Depends on: 1.3.1.
- Acceptance: the plan defines a one-line install path, a one-line update path,
  host-platform detection expectations, version/checksum behavior, and truthful
  fallback when the browser extension is not installed.

## Epic 2: Domain and Storage

### 2.1 Define core domain schema

- Objective: specify the first-class entities for sessions, runs, findings,
  fingerprints, artifacts, scopes, episodes, memory items, evidence links,
  index state, outbound drafts, posted actions, and config layers.
- Depends on: 1.1.
- Acceptance: schema and invariants are documented and represented in code.

### 2.1.1 Define finding fingerprint and state model

- Objective: prevent refresh noise by defining how findings persist or change
  across reruns.
- Depends on: 2.1.
- Acceptance: triage state and outbound state are both explicit, and refresh can
  classify findings predictably.

### 2.1.2 Define data model, write-ownership, and event-history rules

- Objective: define the canonical aggregates, hot/cold/derived storage split,
  session-level write ownership, append-only event history, and migration
  invariants.
- Depends on: 2.1.
- Acceptance: the data/storage contract is documented, same-session write
  conflicts are handled explicitly, and background indexing never mutates core
  review aggregates directly.

### 2.1.3 Model outbound draft batches and approval tokens

- Objective: define grouped outbound payloads, approval tokens, post-time
  invalidation, and per-item posted-action lineage.
- Depends on: 2.1.
- Acceptance: grouped review communication is a first-class domain concept, not
  UI glue around single drafts.

### 2.1.4 Define prompt invocation and outcome-event schema

- Objective: capture prompt preset usage, resolved prompt snapshots, finding
  decisions, approval events, and merge/usefulness signals in an
  analytics-second event model.
- Depends on: 2.1.
- Acceptance: `0.1.0` stores enough typed history for later usefulness analysis
  without turning analytics into the product center.

### 2.2 Implement local storage and migrations

- Objective: create the canonical SQLite-family persistence layer, artifact
  layout, and migration flow.
- Depends on: 2.1.
- Acceptance: review sessions, findings, scopes, and memory records can be
  created, queried, and migrated safely; large artifacts are stored outside the
  main tables without losing provenance.

### 2.3 Add hybrid search foundation

- Objective: make findings, summaries, docs, and related review artifacts
  searchable locally with lexical-first hybrid retrieval.
- Depends on: 2.2.
- Acceptance: repo-scoped search returns relevant results across test data,
  survives restarts, and supports lexical-first degraded mode if semantic
  indexing is unavailable.

### 2.3.1 Define scope, promotion, and index lifecycle rules

- Objective: specify repo/project/org overlays, searchable-versus-promotable
  material, promotion/demotion states, duplicate/conflict handling, and rebuild
  semantics.
- Depends on: 2.1, 2.3.
- Acceptance: scope bleed is prevented by design, candidate versus promoted
  retrieval lanes are explicit, `UsageEvent` outcome vocabulary is defined,
  change-aware demotion rules are explicit, and index rebuild/degraded-mode
  behavior is documented.

## Epic 3: Harness Session Orchestration

### 3.1 Define Roger-to-harness linkage

- Objective: decide the exact mapping between Roger sessions and underlying
  supported harness sessions.
- Depends on: 1.3, 2.1.
- Acceptance: fallback and resume rules are explicit and testable.

### 3.1.1 Implement OpenCode primary adapter

- Objective: implement the full primary harness path for OpenCode, including
  live session linkage, reopen by locator when possible, and ResumeBundle reseed
  fallback.
- Depends on: 3.1.
- Acceptance: OpenCode-backed reviews can resume from Roger and from plain
  OpenCode, and dropout/return flows are explicit and testable.

### 3.1.2 Implement bounded live-CLI provider adapter tranche

- Objective: implement the bounded `0.1.0` live-CLI provider tranche without
  forcing transcript-isomorphic parity with OpenCode.
- Depends on: 3.1.
- Acceptance: bounded-provider reviews can start through Roger, persist
  structured/raw outputs, and reseed from `ResumeBundle` truthfully even when
  native reopen semantics differ; each provider is claimed only to the
  capability tier it has actually validated.

### 3.2 Implement session persistence and resume

- Objective: persist review metadata while keeping OpenCode session continuity.
- Depends on: 2.2, 3.1.
- Acceptance: a review can be resumed from Roger and from plain OpenCode.

### 3.2.1 Define the Roger control bundle for bare-harness dropout

- Objective: specify the minimum Roger control context that must be reloaded when
  a user intentionally drops out to plain OpenCode and later returns.
- Depends on: 3.1.
- Acceptance: the control bundle includes review target, safety posture,
  relevant Roger-specific skills/instructions, and enough attention/finding
  context to keep the session Roger-compliant outside the TUI.

### 3.2.2 Define return-to-Roger behavior from bare harness sessions

- Objective: specify how a dropped-out harness session can jump back into the
  Roger TUI, including explicit helper commands and optional auto-return on
  harness exit when Roger launched the harness process itself.
- Depends on: 3.2.1.
- Acceptance: `rr return` or equivalent helper semantics are explicit, session
  binding from bare harness back to Roger is defined, and auto-return-on-exit is
  treated as a convenience path rather than the only supported return mechanism.

### 3.2.3 Define the harness-native Roger command contract

- Objective: specify the Roger-owned logical command IDs, command/result
  objects, and provider-binding rules for harnesses that support in-session
  commands.
- Depends on: 3.2.1, 3.2.2.
- Acceptance: `RogerCommand`, `RogerCommandResult`, and
  `HarnessCommandBinding` are explicit, supported command IDs are defined, and
  unsupported-command fallback to canonical `rr` flows is unambiguous.

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

### 4.1.1 Implement prompt preset registry and invocation snapshots

- Objective: support stable prompt preset ids, recent/frequent/favorite prompt
  reuse, and exact runtime prompt snapshots for audit and reuse.
- Depends on: 2.1.4, 4.1.
- Acceptance: `0.1.0` prompt reuse works without a heavyweight prompt versioning
  system, and each run preserves the exact resolved prompt it used.

### 4.2 Persist structured findings

- Objective: convert review outputs into findings with explicit states and
  evidence links.
- Depends on: 2.2, 4.1.
- Acceptance: findings are queryable, mutable, and linked to sessions and
  artifacts.

### 4.2.1 Define the structured findings pack and validator

- Objective: define `StructuredFindingsPack v1`, its required fields, validation
  rules, and normalization boundary into Roger-owned finding rows.
- Depends on: 2.1, 4.1.
- Acceptance: the first stable findings-pack schema exists, valid packs can be
  normalized deterministically, and raw output remains linked alongside the
  structured artifact.

### 4.2.2 Define repair taxonomy and retry budget

- Objective: specify how Roger classifies malformed or partial findings packs,
  what can be salvaged, when repair feedback is sent back to the LLM, and when
  Roger stops retrying and surfaces a degraded state.
- Depends on: 4.2.1.
- Acceptance: stage-result states, repairable error classes, salvage rules, and
  bounded retry policy are explicit and testable.

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

### 5.1.2 Make extension-free local usage a first-class path

- Objective: ensure Roger's core CLI/TUI review loop is explicitly usable and
  documented without any browser integration installed.
- Depends on: 5.1.
- Acceptance: local-first launch, resume, findings, and review progression are
  coherent without the extension; docs and CLI affordances do not treat this as
  a degraded or second-class mode.

### 5.1.1 Define local launch profiles and terminal/muxer selection

- Objective: specify how Roger chooses between local launch surfaces such as VS
  Code integrated terminal plus NTM, bare WezTerm windows, or WezTerm splits,
  including reuse and fallback rules.
- Depends on: 3.2.
- Acceptance: `LocalLaunchProfile` fields, supported launch environments, muxer
  strategies, and truthful fallback behavior are explicit and reusable across
  CLI and extension-initiated launches.

### 5.2 Implement `rr findings`, `rr status`, and `rr refresh`

- Objective: expose core review state and refresh behavior through the CLI.
- Depends on: 5.1, 4.3.
- Acceptance: core review operations can be driven entirely from CLI.

### 5.2.1 Add session resolution rules and global session finder

- Objective: let Roger resume the right session from the current repo when the
  match is clear, and provide a global session finder when it is not.
- Depends on: 3.2, 5.1.
- Acceptance: repo-local reinvocation resolves a single strong match safely,
  ambiguous cases open a session picker rather than guessing, and users can jump
  across repos or attention states through a global session-finder surface.

### 5.2.2 Add `rr return` and bare-harness re-entry routing

- Objective: let a user jump back into the correct Roger session from a dropped-
  out harness session without manually reconstructing session identity.
- Depends on: 3.2.2, 5.2.1.
- Acceptance: `rr return` resolves the bound Roger session when present, falls
  back truthfully when not, and interoperates cleanly with ordinary `rr resume`
  and session-finder flows.

### 5.2.3 Add harness-command parity over Roger core operations

- Objective: route supported in-harness Roger commands through the same core
  operations as `rr` rather than bespoke provider glue.
- Depends on: 3.2.3, 5.2.
- Acceptance: supported harness commands for help, status, findings, refresh,
  clarification, draft opening, and return share Roger-owned semantics and
  degrade cleanly to CLI guidance when a provider lacks the needed capability.

## Epic 6: TUI

### 6.1 Build the review shell

- Objective: create the main TUI frame, session selector, attention queue, and
  current review overview.
- Depends on: 5.1.
- Acceptance: users can enter an active review session and immediately see
  review status, pending attention, and the main navigation surfaces.

### 6.1.1 Add global session finder and re-entry UX

- Objective: make the TUI review home usable as a global jumping-off point for
  recent, active, and attention-requiring sessions, not just the current repo.
- Depends on: 5.2.1, 6.1.
- Acceptance: the TUI can search and switch between sessions across repos,
  distinguish ambiguous same-target sessions, and reopen the selected session
  into the correct local workspace.

### 6.2 Build the findings workflow

- Objective: implement the findings queue, detail inspector, evidence drilldown,
  and state transitions.
- Depends on: 4.2, 6.1.
- Acceptance: findings can be filtered, grouped, inspected, and triaged quickly
  and accurately from the TUI.

### 6.2.1 Add non-mutating clarification in finding detail

- Objective: let a user ask bounded clarifying questions about a finding without
  changing its triage or outbound state.
- Depends on: 4.2, 6.2.
- Acceptance: clarification responses are linked to the finding, remain local,
  and do not implicitly mutate finding state or create outbound drafts.

### 6.3 Add outbound draft approval

- Objective: let users inspect, edit, batch, and approve proposed GitHub
  outputs in a dedicated draft queue.
- Depends on: 6.2, 8.1.
- Acceptance: nothing is posted until explicit local approval is completed, and
  posted or failed drafts remain inspectable with audit lineage.

## Epic 7: GitHub Integration and Extension

### 7.1 Implement GitHub adapter

- Objective: fetch PR context and prepare outbound actions through explicit
  adapter boundaries.
- Depends on: 1.3, 5.1.
- Acceptance: Roger can resolve a PR target and prepare local outbound drafts.

### 7.2 Validate daemonless browser bridge

- Objective: prove the extension can launch or resume Roger locally without
  introducing a persistent daemon as the system center, and define the capability
  boundary between launch-only and status-aware companion behavior.
- Depends on: 1.3, 7.1.
- Acceptance: Native Messaging is validated as the primary v1 bridge, a
  browser-initiated launch path and bounded fallback path both work, the v1
  status/readback contract is explicit rather than implied, and the required
  companion-binary packaging/install story is known for the supported macOS,
  Windows, and Linux targets.

### 7.2.1 Keep extension packaging separate from core local installation

- Objective: ensure the browser-extension packaging story does not become a
  hidden dependency of Roger's local install/update path.
- Depends on: 1.3.2, 7.2.
- Acceptance: Roger can be installed and updated as a local CLI/TUI product
  independently of the extension, while extension packaging remains a separate
  optional distribution surface.

### 7.3 Implement extension UI

- Objective: inject PR-local Roger actions and any bridge-supported bounded
  status affordances into GitHub pages.
- Depends on: 7.2.
- Acceptance: users can start, resume, or refresh reviews from GitHub, pass a
  small objective or preset, and access only those in-page affordances supported
  by the chosen daemonless bridge without turning the extension into a state
  owner; the extension remains dependency-light and avoids framework-heavy
  frontend stacks unless a capability proves they are necessary.

## Epic 8: Approval and Posting Flow

### 8.1 Model outbound drafts

- Objective: define the local representation for comments, questions, and
  suggestions awaiting approval, including grouped batches and approval tokens.
- Depends on: 2.1, 4.2, 7.1.
- Acceptance: outbound drafts and grouped batches are linked to findings,
  tracked locally, and safe against cross-review retargeting.

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

- Objective: create isolated local review environments only when the selected
  flow actually needs them.
- Depends on: 5.1.
- Acceptance: a review can prepare and track its worktree context safely
  without making worktrees the default tax on ordinary read-mostly review.

### 9.2 Implement named-instance storage strategy

- Objective: support multiple local Roger instances with one canonical Roger
  store per profile and explicit repo-local resource isolation.
- Depends on: 2.2, 9.1.
- Acceptance: two local instances can coexist without corrupting one another;
  single-repo mode remains the default path, copied env files, ports,
  docker/container naming, caches, and artifact dirs are explicit, and v1 does
  not depend on DB-copy synchronization; setup resolution and preflight
  classification are covered by unit tests as well as integration tests.

### 9.2.1 Handle same-PR instance selection and launch routing

- Objective: make multiple Roger instances or worktrees for the same PR an
  explicit selection problem rather than a hidden auto-routing guess.
- Depends on: 5.1.1, 9.2.
- Acceptance: CLI and extension-initiated launches can disambiguate same-PR
  targets safely, including launch-profile compatibility and stale-target
  recovery.

## Epic 10: Search, Memory, and Skills

### 10.1 Add prior-review lookup

- Objective: surface related historical findings and evidence during review and
  refresh with explicit scope and provenance.
- Depends on: 2.3, 2.3.1, 4.3.
- Acceptance: refresh flows can pull in prior high-signal findings quickly,
  with repo-first behavior and explicit project/org overlays when enabled.

### 10.2 Add memory promotion, decay, and overlay policies

- Objective: implement observed/candidate/established/proven/deprecated/
  anti-pattern handling, change-aware demotion, and explicit scope overlays.
- Depends on: 2.3.1, 10.1.
- Acceptance: only curated/promoted material enters reusable memory by default,
  broader scopes require explicit enablement, candidate memory cannot silently
  behave like promoted memory, and stale or harmful guidance can be demoted.

### 10.3 Capture failure patterns into reusable skills

- Objective: turn repeated review or suggestion failures into explicit prompts or
  skills.
- Depends on: 10.2.
- Acceptance: at least one validated procedure and one anti-pattern can be
  encoded with evidence and reused.

### 10.4 Evaluate optional structured-context packaging

- Objective: compare compact JSON against optional TOON packing for large,
  tabular prompt payloads without making TOON a core dependency.
- Depends on: 4.2, 10.1.
- Acceptance: model-specific smoke tests determine whether TOON is worth
  enabling for any backend, and JSON remains the default if not.

## Epic 11: Safety and Validation

### 11.1 Enforce review-safe defaults

- Objective: ensure review mode stays read-mostly and approval-gated.
- Depends on: 4.2, 8.1.
- Acceptance: posting and mutation paths are visibly elevated and test-covered.

### 11.2 Add end-to-end validation matrix

- Objective: cover CLI launch, TUI review, GitHub launch, refresh, posting, and
  supported-harness fallback, with OpenCode primary and bounded live-CLI
  providers as secondary paths.
- Depends on: 5.2, 6.3, 7.3, 8.2.
- Acceptance: the main user workflows are executable and repeatable.

### 11.2.1 Map flow IDs to beads and integration coverage

- Objective: use the review flow matrix as the traceability layer between user
  flows, implementation beads, and eventual integration-test selection.
- Depends on: 11.2.
- Acceptance: major beads reference relevant flow IDs, and each high-risk flow
  family has an explicit planned happy-path and degradation-path validation
  target.

### 11.2.2 Define fixture repo set and support matrix coverage

- Objective: make provider/browser/OS coverage, fixture repos, and blessed
  end-to-end paths explicit enough to drive CI, release smoke, and manual
  validation without guesswork.
- Depends on: 1.3.1, 11.2.
- Acceptance: the support matrix is documented, fixture repos are named, and the
  first blessed E2E plus provider-acceptance suites are explicit.

### 11.2.2.1 Stand up shared validation harness scaffold and artifact layout

- Objective: create the common validation harness skeleton before
  suite-specific work fans out, including shared directory layout, suite naming
  rules, metadata envelope, helper boundaries, and artifact tree.
- Depends on: 11.2.1, 11.2.2, 11.7.
- Acceptance: the shared harness layout is explicit, suite metadata is reusable
  across validation families, and failure-artifact handling plus structural
  snapshot rules are fixed before provider acceptance or heavier automation
  begins.

### 11.2.2.2 Create canonical validation fixture corpus and manifest

- Objective: turn the planned fixture families into one Roger-owned corpus with
  provenance, ownership, intended suite usage, and degraded-mode annotations.
- Depends on: 11.2.2.1.
- Acceptance: the compact review, monorepo, same-PR multi-instance, malformed
  findings, ResumeBundle reopen/reseed/dropout, GitHub draft/post payload,
  bridge transcript, and migration/artifact-integrity fixtures are all named,
  and each fixture declares its allowed suite families plus any intentional
  brokenness.

### 11.2.2.3 Wire suite metadata, CI tiers, and artifact retention entrypoints

- Objective: connect the shared harness to fast-local, PR, gated, nightly, and
  release entrypoints, plus the reusable suite metadata and artifact-retention
  behavior that later suites should inherit.
- Depends on: 11.2.2.2, 11.7.
- Acceptance: suite entrypoints are explicit for each tier, flow IDs and
  fixture-family ownership are declared through one shared metadata contract,
  and failure artifacts plus automated-E2E budget checks are wired into the
  harness instead of bespoke suite scripts.

### 11.2.3 Implement provider acceptance suites for OpenCode and bounded providers

- Objective: turn the supported-provider promises into repeatable adapter
  acceptance suites instead of leaving them as documentation-only claims.
- Depends on: 3.1.1, 3.1.2, 11.2.2, 11.2.2.3.
- Acceptance: OpenCode acceptance covers real reopen plus ResumeBundle fallback;
  bounded-provider acceptance covers Roger-owned session/run continuity,
  structured/raw capture, and truthful reseed without pretending native parity
  providers do not have.

### 11.3 Validate refresh identity behavior

- Objective: ensure reruns classify old and new findings correctly instead of
  duplicating or losing them.
- Depends on: 2.1.1, 4.3, 11.2.2.3.
- Acceptance: test scenarios cover carried-forward, resolved, superseded, and
  stale findings.

### 11.4 Validate search and memory safety

- Objective: ensure retrieval stays repo-first, exact-anchor-friendly, and safe
  under degraded indexing or conflicting history.
- Depends on: 2.3, 2.3.1, 10.2, 11.2.2.3.
- Acceptance: test scenarios cover scope-bleed suppression, exact-anchor recall,
  stale-memory suppression, conflict surfacing, abstention, and lexical-only
  degraded mode.

### 11.5 Validate structured findings repair and degraded modes

- Objective: ensure malformed, partial, or missing findings packs degrade
  truthfully without discarding valid data or hiding raw output.
- Depends on: 4.2.1, 4.2.2, 6.2, 7.3, 11.2.2.3.
- Acceptance: test scenarios cover valid pack, partial pack, raw-only fallback,
  repair-needed state, retry exhaustion, and extension/TUI state consistency for
  those outcomes.

### 11.6 Validate same-PR multi-instance and launch-profile routing

- Objective: ensure Roger can safely route launches when multiple instances or
  worktrees exist for the same PR and different launch profiles are configured.
- Depends on: 5.1.1, 9.2.1, 11.2.2.3.
- Acceptance: test scenarios cover same-PR disambiguation, unavailable muxer or
  terminal fallback, and truthful recovery when the originally requested target
  no longer exists.

### 11.7 Validate draft invalidation, no-status bridge mode, and partial post recovery

- Objective: ensure stale drafts, launch-only extension mode, and partial GitHub
  posting failures all degrade honestly and recover safely.
- Depends on: 4.3, 7.2, 8.2, 11.2.2.3.
- Acceptance: test scenarios cover draft invalidation after refresh or rebase,
  honest no-status behavior when the extension bridge cannot read back state,
  partial post success with safe retry, and `awaiting_user_input` as a durable
  review state across TUI, CLI, and extension surfaces.

### 11.8 Validate clarification-in-place and bare-harness dropout

- Objective: ensure users can interrogate a finding without mutating it and can
  intentionally leave Roger for bare OpenCode without losing Roger control
  context.
- Depends on: 3.2.1, 6.2.1, 11.2.2.3.
- Acceptance: test scenarios cover non-mutating clarification from the finding
  inspector, dropout to plain OpenCode with Roger control bundle loaded, and
  clean return to the same Roger review session afterward.

### 11.8.1 Validate return-to-Roger from dropped-out harness sessions

- Objective: ensure users can naturally get back into Roger after exiting or
  tiring of the bare harness session.
- Depends on: 3.2.2, 5.2.2, 11.8.
- Acceptance: test scenarios cover explicit `rr return`, auto-return on harness
  exit when Roger owns the parent process, and truthful fallback to `rr resume`
  or the session finder when direct return is unavailable.

### 11.8.2 Validate in-harness Roger commands and truthful CLI fallback

- Objective: prove supported harness-native Roger commands behave like thin
  adapters over canonical Roger operations rather than divergent provider
  features.
- Depends on: 3.2.3, 5.2.3, 11.8.1.
- Acceptance: test scenarios cover provider-native Roger commands for status,
  findings, clarification, and return, plus truthful degradation to `rr`
  guidance when command support is absent or partial.

### 11.9 Validate repo-local re-entry and global session finding

- Objective: ensure users can get back into the right Roger session whether they
  re-run Roger in the repo or return from elsewhere and need a global finder.
- Depends on: 5.2.1, 6.1.1, 11.2.2.3.
- Acceptance: test scenarios cover single strong match in the current repo,
  ambiguous same-repo or same-PR matches, global session search by attention
  state, and truthful recovery when the chosen session cannot be resumed
  directly.

## Critical Dependency Spine

The likely critical path for v1 is:

1. repo foundation
2. domain schema
3. storage
4. search/index foundation
5. supported-harness linkage
6. prompt pipeline
7. structured findings
8. session-aware CLI
9. TUI findings workflow
10. outbound draft model
11. explicit posting flow
12. GitHub adapter
13. extension bridge and UI

This order keeps the extension off the critical path until the local review core
is real.
