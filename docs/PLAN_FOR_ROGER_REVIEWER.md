# Plan for Roger Reviewer

## Project Statement

Roger Reviewer is a local-first pull request review system that combines a
session-aware CLI, a TUI-first review interface, and a GitHub browser
extension. Its job is not to silently auto-fix code. Its job is to drive
high-quality review loops, keep review state durable and searchable, and let a
human approve what gets sent back to GitHub.

The core differentiator is continuity. Every finding, prompt pass, artifact,
and follow-up should map back to a durable local session that can still be
resumed in plain OpenCode if Roger-specific layers are unavailable or compacted.

## Naming

Canonical product name: `Roger Reviewer`

Working CLI shorthand for the plan: `rr`

This matches the existing
[`roger-reviewer-brain-dump.md`](/Users/cdilga/Documents/dev/roger-reviewer/roger-reviewer-brain-dump.md)
and is the name the repo should optimize around unless a later branding pass
changes it deliberately.

## Goals

- Deliver a durable local review workspace centered on findings, artifacts, and
  session continuity rather than one-shot prompt runs.
- Make the TUI the primary power-user interface for triage, follow-up, and
  approval.
- Let a GitHub extension inject context-aware review actions directly into PR
  pages without making the extension the source of truth.
- Preserve a lowest-common-denominator fallback where the underlying OpenCode
  session remains usable and resumable without Roger-specific UI.
- Keep all review knowledge local, searchable, and fast to retrieve, including
  prior findings, indexed PR artifacts, and relevant cached context.
- Support additive configuration: global defaults plus repo-specific overlays,
  with explicit override behavior instead of hidden mutation.
- Use isolated worktrees and named instances so multiple review sessions can run
  safely side by side.
- Require explicit approval before comments, questions, or suggestions are sent
  back to GitHub.

## Non-Goals

- Automatically fixing bugs by default.
- Automatically posting review comments or suggestions without confirmation.
- Requiring a long-running daemon as the architectural center of the system.
- Building a GitHub-only product with no useful local/TUI workflow.
- Depending on a single UI surface; the extension and TUI should be peers over a
  shared application core.
- Solving every future editor integration in v1. VS Code support is a later
  extension point, not a launch requirement.

## Product Principles

- Local-first beats cloud-first. Local state is the source of truth.
- Findings are first-class objects, not transient text blobs.
- Sessions must degrade gracefully back to plain OpenCode.
- Review is read-heavy and latency-sensitive, so local indexing matters.
- Additive config is safer than opaque replacement.
- Human approval gates must be obvious and hard to bypass accidentally.
- Architecture should isolate adapters from the review domain so GitHub, TUI,
  CLI, and future editors can share the same core.

## Critical Assumptions

These assumptions need to be validated early rather than silently baked into the
implementation plan:

- OpenCode exposes a stable-enough boundary for Roger to link sessions and
  recover context without invasive coupling.
- A browser extension can launch or resume local Roger flows using an on-demand
  mechanism that does not turn into a hidden daemon.
- FrankenTUI can be integrated without forcing a second, conflicting application
  architecture.
- SQLite plus FTS is enough for the first usable search layer if schemas and
  indexing are disciplined.

## Primary Users

### Human reviewer

Wants to launch a review from a PR page or from the shell, inspect findings in a
single place, decide what matters, ask follow-up questions, and explicitly
approve what should be posted.

### Agent operating inside a review session

Needs a structured way to explore the codebase, run staged review prompts,
consult local memory and prior findings, and persist outputs without losing the
ability to resume in plain OpenCode.

### Future automation surfaces

Need an application core with stable commands and data contracts so a browser
extension, TUI, CLI, or later VS Code extension can all reuse the same review
model.

## Core User Workflows

### Workflow 1: Launch a review from GitHub

1. User opens a GitHub PR in Chrome or Brave.
2. Extension injects a Roger action button and status indicator.
3. User chooses a review action such as start review, resume review, or refresh
   findings.
4. Extension passes PR context to a local Roger launcher using a daemonless
   bridge.
5. Roger creates or reuses a local review instance, prepares a worktree if
   needed, and opens the TUI or CLI flow.
6. Review progress and unresolved findings become visible both locally and, at a
   minimum, as extension-readable status.

### Workflow 2: Launch a review from the shell

1. User runs a session-aware CLI command such as `rr review`, `rr resume`, or
   `rr findings`.
2. Roger infers repo and branch context from the current working directory when
   possible.
3. Roger resolves the related PR if one exists remotely or accepts explicit PR
   input.
4. Roger resumes or starts the underlying OpenCode-backed session.
5. Roger opens the TUI or prints actionable CLI output depending on mode.

### Workflow 3: Conduct the review

1. Roger stages prompts in a deliberate sequence: explore first, then deep
   analysis, then further passes only if they still produce value.
2. Findings are captured as structured records rather than free-form terminal
   output.
3. Each finding can be marked accepted, ignored, needs follow-up, ask-in-GitHub,
   or similar explicit states.
4. Clarifying questions can be attached to findings in a structured way.
5. Review artifacts, prompts, and intermediate outputs are retained locally for
   later resume, refresh, or audit.

### Workflow 4: Refresh after new commits

1. User refreshes a review after a PR changes.
2. Roger pulls new metadata and diffs, then runs a fresh-eyes pass.
3. Prior high-signal findings are selectively reintroduced so the system does
   not start from zero.
4. Findings that remain relevant are carried forward; resolved or obsolete ones
   are marked accordingly.

### Workflow 5: Approve outbound actions

1. Roger prepares proposed GitHub comments, questions, or suggestions in local
   draft form.
2. User reviews and edits them in the TUI or another local interface.
3. Only after explicit approval does Roger use `gh` CLI or another adapter to
   post them.
4. Roger stores the mapping between local finding state and remote review
   action.

## Agent Workflows

- Every agent session begins by loading review context, prior relevant findings,
  and project-specific prompts or skills.
- Agents operate primarily in read/review mode unless the user explicitly
  authorizes mutation-oriented work.
- Agents write structured findings and artifacts back into the Roger store
  rather than relying on chat transcript recovery alone.
- Agents can recursively continue through multiple review passes, but must stop
  when marginal value is low or human intervention is required.
- If Roger-specific state is unavailable, the agent should still be able to
  continue from the plain OpenCode session with enough context reinserted.

## System Architecture

### Architectural shape

Use a modular local application core with shared domain logic and multiple
presentation/adaptation layers:

- shared review domain and orchestration layer
- storage and indexing layer
- session adapter layer over OpenCode
- Git and GitHub adapter layer
- TUI frontend
- browser extension frontend
- session-aware CLI frontend

This is effectively a ports-and-adapters design. The review domain owns
findings, review sessions, prompt stages, and approval state. UI surfaces should
never reimplement those rules independently.

### Why this shape

- It preserves tool-agnostic behavior.
- It makes TUI and extension coexist cleanly.
- It keeps OpenCode fallback feasible.
- It reduces the chance that the extension becomes a special-case control plane.

## Proposed Repository Structure

The exact build tooling can still change, but the plan assumes a monorepo-style
layout because the product has multiple surfaces sharing one domain model.

```text
.
├── apps/
│   ├── cli/
│   ├── extension/
│   └── tui/
├── packages/
│   ├── app-core/
│   ├── config/
│   ├── github-adapter/
│   ├── prompt-engine/
│   ├── session-opencode/
│   ├── storage/
│   └── worktree-manager/
├── docs/
│   ├── PLAN_FOR_ROGER_REVIEWER.md
│   ├── BEAD_SEED_FOR_ROGER_REVIEWER.md
│   ├── ALIEN_ARTEFACTS_FOR_ROGER_REVIEWER.md
│   └── PLANNING_WORKFLOW_PROMPTS.md
└── roger-reviewer-brain-dump.md
```

## Technology Direction

### Confirmed: split-language architecture

FrankenTUI has been explored directly (cloned at `_exploration/frankentui`). It
is 100% Rust with no TypeScript bindings, no RPC API, and no way to drive it
from an external process. The `Model` trait must be implemented in Rust. This
resolves the open question about FrankenTUI's runtime requirements and forces a
split-language architecture.

**TUI layer: Rust**
- Implements the FrankenTUI `Model` trait
- Diff-based rendering, diff-optimised widgets, inline mode
- Synchronous event loop (no tokio/async-std)
- Communicates with the application core via a local protocol (Unix socket or
  named pipe with a simple JSON envelope)

**CLI, session orchestration, prompt engine, GitHub adapter: TypeScript**
- Session-aware `rr` CLI commands
- OpenCode session linkage
- Prompt pipeline and finding persistence
- GitHub adapter (`gh` CLI wrapper)
- Shared domain types exposed to the TUI via the local protocol

**Browser extension: TypeScript/JavaScript**
- Standard WebExtension (Chrome/Brave)
- v1: custom URL protocol handler only (see Bridge section)
- v2: Native Messaging host written in Rust or TypeScript

### Rationale for the split

- Brain dump explicitly names FrankenTUI. Replacing it with a TypeScript TUI
  loses the rendering quality and inline-mode guarantees.
- CASS (also cloned at `_exploration/cass`) follows this same pattern: Rust
  binary using FrankenTUI with a TypeScript-compatible data layer. It is a
  working reference implementation.
- The split is bounded. The Rust TUI binary is a thin presentation layer. It
  does not own domain logic; it renders state and forwards user actions.
- Shared domain types can be codegen'd or duplicated at the protocol boundary
  rather than needing a monorepo language unification.

## Core Domain Model

The plan assumes first-class entities for:

- `ReviewSession`
- `ReviewRun`
- `Finding`
- `FindingFingerprint`
- `FindingState`
- `PromptStage`
- `Artifact`
- `ArtifactDigest`
- `GitHubReviewTarget`
- `WorktreeInstance`
- `ConfigLayer`
- `OutboundDraft`
- `PostedAction`

Key rule:

A finding is not just text. It has origin, evidence links, state, outbound draft
mapping, timestamps, and review-session lineage.

### Finding identity and lifecycle

Refresh behavior will fail unless findings have stable enough identity to match
or supersede prior findings across reruns.

Required invariants:

- each finding gets a deterministic or near-deterministic fingerprint derived
  from review target, evidence location, issue class, and a normalized summary
- refresh flows can mark a finding as carried forward, superseded, resolved, or
  stale rather than duplicating it blindly
- outbound drafts and posted GitHub actions retain lineage back to the finding
  snapshot that produced them
- user-facing states should distinguish triage state from posting state

Suggested state split:

- triage states such as `new`, `accepted`, `ignored`, `needs-follow-up`,
  `resolved`, `stale`
- outbound states such as `not-drafted`, `drafted`, `approved`, `posted`,
  `failed`

## Session Model

Roger should wrap an OpenCode session rather than replace it.

Required properties:

- every Roger review session maps to an underlying OpenCode session or transcript
  anchor
- Roger stores additional structured metadata outside that session
- if Roger UI state is unavailable, the user can still reopen the OpenCode
  session directly
- compaction recovery should be able to reinsert selected artifacts, prior
  findings, and prompt-stage summaries into a resumed session

This means Roger metadata must reference, not obscure, the underlying session.

## Integration Contracts

Before implementation spreads across multiple packages, Roger needs three clear
contracts.

### Contract 1: OpenCode session boundary

Roger must define exactly what it reads from and writes to the underlying
OpenCode session layer.

Minimum expectations:

- create or link to a session
- capture enough identifiers to reopen the same session later
- reinsert compact context bundles when resuming
- avoid depending on fragile internal implementation details if a stable CLI or
  file-level boundary exists

### Contract 2: Browser-to-local launch boundary

The extension should pass a small launch payload, not own ongoing process state.

Minimum payload:

- repo identifier
- PR identifier or URL
- requested action such as `start`, `resume`, or `refresh`
- optional prompt override or launch mode

Minimum behavior:

- one-shot launch or resume
- predictable fallback when the bridge is unavailable
- no architectural dependence on a long-lived background service

### Contract 3: Outbound posting boundary

Roger must separate finding generation from GitHub mutation.

Minimum expectations:

- outbound drafts are materialized locally first
- approval is explicit and reviewable
- the exact payload posted to GitHub is snapshotted for audit
- local state records success, failure, and remote identifiers

## Storage and Indexing Strategy

### Source of truth

Use a local SQLite-family database as the canonical store for review sessions,
findings, artifacts, status, and index metadata.

### Required capabilities

- transactional local writes
- schema migration support
- fast relational lookup
- full-text search over findings, prompts, comments, and selected artifacts
- room for semantic search over review history

### Recommendation

Use Tantivy from day one. Do not start with SQLite FTS and migrate later.

Reasoning:

- The TUI is already Rust. Tantivy lives naturally in the same binary alongside
  FrankenTUI, exposed to TypeScript via the Unix socket protocol. This adds no
  new language boundary.
- SQLite FTS → Tantivy migration is an annoying and inevitable reindex. Skip
  the intermediate step.
- CASS (`_exploration/cass`) is a working Tantivy integration for an identical
  use case. The patterns are directly copyable.
- Tantivy gives prefix matching, edge n-grams, and a proper query language from
  day one. SQLite FTS5 does not.
- SQLite remains the relational store for sessions, findings, and config — only
  the full-text index moves to Tantivy. These are complementary, not competing.

Semantic search (FastEmbed embeddings) remains a v1.5 addition once the Tantivy
base is stable. The indexing architecture supports it without schema changes.

### Artifact strategy

- Store metadata and normalized excerpts in the database.
- Store larger raw artifacts in a local content-addressed artifact directory if
  they become too large for comfortable inline DB storage.
- Keep database rows small enough that the TUI remains responsive.
- Define artifact budget classes early so prompt transcripts, diff chunks, and
  large reference payloads do not bloat the primary tables accidentally.

## Search and Memory Strategy

### v1 — Tantivy + FastEmbed (full hybrid from day one)

CASS (`_exploration/cass`) is the reference implementation. Copy it, don't
reinvent it.

- **Tantivy** (BM25, edge n-grams, prefix matching) for full-text search
- **FastEmbed** (ONNX, AllMiniLML6V2, 384-dimensional, CPU-only, ~23MB) for
  semantic embeddings
- **FSVI binary format** for vector storage (F16 quantization, memory-mapped,
  content-addressed dedup)
- **Hybrid search via RRF**: 3x candidate fetch from each mode, score
  `Σ(1 / (K + rank))` where K=60
- **Incremental indexing**: streaming mode + automatic segment merging at 4+
  segments
- Lives in the Rust binary alongside FrankenTUI, queried by TypeScript over
  the Unix socket

Roger Reviewer semantic doc ID encoding:
`pr|number|finding_id|review_round|role|timestamp|[content_hash]`

Rationale for building this from day one:
- FastEmbed is a Rust crate in the same binary — no new language boundary
- Designing the doc ID schema without embedding slots means a migration later
- Marginal implementation cost over Tantivy-alone is small given CASS as a reference
- Sequencing note: implement the core review loop first within implementation,
  but the search infrastructure ships complete rather than in two passes

## Prompt Pipeline

Roger should encode a staged review loop rather than a single monolithic prompt.

### Baseline sequence

1. exploration pass
2. deep review pass
3. follow-up or recursive pass only if value remains

### Required behavior

- automatic advancement between prompt stages when safe
- explicit flags when human review is needed before continuing
- structured capture of outputs and findings for each stage
- ability to rerun a stage after refresh without corrupting prior findings

This is one of the most important areas to keep deterministic enough that the
TUI can show coherent status rather than raw prompt chaos.

## TUI Requirements

The TUI is the default power-user workspace.

It should provide:

- session list and resume entrypoints
- current review overview
- itemized findings list
- finding detail view with linked artifacts and evidence
- state transitions such as accept, ignore, follow-up, ask-in-GitHub
- outbound draft review and approval
- history or audit trail for refreshes and prior passes

The TUI must prioritize scan speed. The main view should answer:

- what changed
- what matters
- what still needs a decision
- what is already drafted for outbound action

## Browser Extension Requirements

The browser extension exists to reduce friction on GitHub PR pages, not to own
core state.

### v1 scope — launch button only

v1 delivers a single "Launch Roger" button injected into GitHub PR pages. That
is the entire v1 extension surface.

- one button, visible on PR pages
- clicking it invokes a local Roger command with the PR URL as context
- no status indicator, no dropdown, no keybindings, no PR-aware injection
- fallback if the bridge is unavailable: copy a ready-to-run local command to
  clipboard

This scope decision is intentional. Deep Chrome integration is the area most
likely to require a hidden daemon, fight the browser security model, or require
ongoing maintenance as GitHub updates its DOM. Keeping v1 minimal validates the
bridge mechanism without betting the launch on it.

### v2 scope — deep integration

Deferred to v2:

- status indicator for unresolved or unapplied findings
- PR-aware dropdown with review actions and prompt overrides
- ability to add prompts or review actions directly from the PR page
- GitHub-specific shortcut integration
- live status reflection without a persistent daemon

### Bridge strategy

The extension-to-local bridge must stay daemonless in steady state. Two
mechanisms have been evaluated:

**v1: Custom URL protocol handler**

Register `roger://` as a URL scheme on the host OS (macOS: `LSURLTypes` in an
app bundle `Info.plist`, or a lightweight helper registered with
`NSWorkspace`). The extension navigates to:

```
roger://launch?repo=owner/repo&pr=123&action=start
```

The OS launches the `rr` CLI with those args. No daemon. No manifest. One-shot.
Fallback: copy the equivalent `rr review --pr 123 --repo owner/repo` command to
clipboard if the handler is not installed.

This is the simplest possible daemonless bridge and requires zero changes to
the Chrome extension security model.

**v2: Native Messaging**

Chrome's Native Messaging API launches a registered native executable on demand
via stdin/stdout JSON messages. It is bidirectional, daemonless (Chrome spawns
and owns the process lifetime), and supports the status-sync and action-routing
needed for deep integration.

Requires:
- a native messaging host manifest registered in the OS (one JSON file)
- a host executable (could be the `rr` CLI in a `--messaging` mode)
- no always-on background service

WebSocket / local HTTP is explicitly rejected for the bridge: it requires a
daemon and introduces a background service as the architectural center.

The v1 spike needs to prove only that the custom URL protocol handler registers
and launches correctly on macOS. Native Messaging is a v2 spike.

## CLI Requirements

The CLI is the glue layer between local repo context, the TUI, and automation.

Candidate commands:

- `rr review`
- `rr resume`
- `rr findings`
- `rr refresh`
- `rr post`
- `rr status`
- `rr mark-all-accepted`

CLI behavior should be session-aware. If invoked from a repo directory, it
should infer the likely review target when possible rather than forcing
redundant flags.

## Worktree and Named Instance Model

Use worktrees as the default isolation unit for active reviews or review-driven
follow-up work.

Requirements:

- each review instance can have its own worktree when needed
- multiple local copies of the app can coexist as named instances
- instances should be able to copy relevant DB state from a primary instance
  efficiently rather than cloning everything blindly
- environment-specific writes should remain disabled by default

Open question:

The fast-diffing DB copy strategy needs concrete design work. For v1, it may be
enough to start with conservative snapshot export/import semantics before
optimizing incremental transfer.

## Configuration Model

Configuration should be layered and additive by default.

Layers:

- built-in defaults
- user-global templates
- repo-specific templates
- per-review overrides

Rules:

- later layers may add or override explicitly
- hidden replacement behavior is not acceptable
- effective config should be inspectable
- prompt templates and skills should be inherited in a controlled way

## Safety and Approval Model

These controls are not optional.

### Required defaults

- no automatic GitHub posting
- no automatic bug-fixing mode
- no writes to dev/test targets unless explicitly enabled
- clear approval state before outbound actions execute
- audit trail for what was posted and why

### GitHub write path

- drafts are prepared locally first
- user reviews or edits them
- Roger posts via adapter only after confirmation
- posted state is persisted locally and linked back to the finding

### Local environment protection

- runtime should distinguish read-only review flows from mutation-capable flows
- mutation-capable flows should be visibly elevated, not implicit

## GitHub Integration

Use GitHub as a target surface, not as the canonical state store.

Capabilities for v1:

- resolve PR metadata
- fetch diff and review context
- draft comments/questions/suggestions locally
- post approved outputs back through `gh` CLI or another explicit adapter

Anything that mutates GitHub should be behind the same approval model as the TUI
and CLI.

## Testing Strategy

Testing needs to match the product shape.

### Required test layers

- unit tests for shared domain and config layering
- storage tests with migration coverage
- prompt pipeline tests at the orchestration layer
- TUI interaction tests for key findings flows
- extension tests for injection, action wiring, and fallback behavior
- CLI smoke tests for repo-context inference and session resume

### Manual validation

- end-to-end launch from a GitHub PR page into a local review
- end-to-end launch from CLI in a repo
- refresh after new commits
- explicit approval before posting
- plain OpenCode fallback and resume

## Rollout Plan

### Phase 0: Converge scope and unknowns

- lock the canonical name as Roger Reviewer
- define minimum v1 surface area
- isolate undefined terms from the brain dump as open questions
- decide what absolutely must ship before extension work starts

### Phase 0.5: Run architecture risk spikes

- validate the OpenCode session boundary
- validate the browser extension launch bridge
- validate the artifact storage split between DB rows and artifact blobs
- write ADRs for any decision that materially changes the package layout

### Phase 1: Foundation and domain

- set up repo structure and package boundaries
- define domain schema and storage migrations
- build basic review session persistence
- build session linkage to underlying OpenCode sessions

### Phase 2: CLI and prompt engine

- implement session-aware CLI
- implement review-stage orchestration
- persist structured findings and artifacts
- prove that a local review loop works without the extension

### Phase 3: TUI

- implement TUI shell
- add findings list/detail/action flows
- add outbound draft approval UX
- validate refresh and resume behavior

### Phase 4: GitHub integration and extension (v1)

- finalize GitHub adapter behavior
- validate the daemonless one-shot launch bridge
- implement the single "Launch Roger" button injection
- prove launch from a PR page invokes a local review correctly
- clipboard fallback when bridge is unavailable

### Phase 4.5 (v2): Deep extension integration

- status indicator for unresolved findings on PR pages
- PR-aware dropdown with review actions and prompt overrides
- GitHub-specific shortcut integration
- live status reflection without a persistent daemon

### Phase 5: Search, memory, and polish

- add fast full-text search across reviews and findings
- layer in semantic search if it helps materially
- add review-memory workflows and failure-pattern capture
- harden multi-instance and worktree workflows

This order intentionally defers the extension until the local review loop is
real. Otherwise the project risks optimizing the entrypoint before the product
core exists.

## Validation Gates

Do not advance phases casually.

### Gate A: Domain viability

- storage schema exists
- review session and finding lifecycle are explicit
- OpenCode session linkage is implemented
- finding identity and refresh semantics are explicit enough to avoid duplicate
  finding explosions

### Gate B: Core review loop viability

- CLI can start and resume a review
- prompt stages persist outputs cleanly
- findings survive restart

### Gate C: TUI usability

- user can review, filter, and change finding states quickly
- outbound drafts are reviewable locally

### Gate D: GitHub bridge realism

- extension "Launch Roger" button can invoke a local review from a PR page
  without requiring a persistent daemon
- clipboard fallback works when the bridge is unavailable
- v1 makes no claims about live status sync — that is explicitly v2

### Gate E: Safe outbound actions

- nothing posts without explicit approval
- posted outputs are tracked back to local findings

## Risks and Mitigations

### Risk: extension bridge forces a hidden daemon

Mitigation:

- treat bridge validation as an early spike
- define a fallback launch path up front
- reject designs that quietly move core state into a background service

### Risk: OpenCode fallback becomes fake

Mitigation:

- require every review session to maintain an underlying OpenCode mapping
- test resume in plain OpenCode explicitly

### Risk: findings degrade into unstructured text dumps

Mitigation:

- define the finding schema early
- make all surfaces operate on structured findings
- add explicit finding fingerprint and refresh rules before implementing refresh

### Risk: search ambitions stall the product

Mitigation:

- ship SQLite plus FTS first
- defer semantic search behind stable interfaces

### Risk: worktree and multi-instance sync becomes overengineered

Mitigation:

- start with conservative copy/snapshot flows
- optimize only after the base isolation workflow works

### Risk: v1 silently depends on unresolved integration contracts

Mitigation:

- run focused risk spikes before package-level implementation
- write ADRs when a spike changes assumptions about runtime, bridge design, or
  OpenCode coupling

### Risk: unsafe mutations slip into the review path

Mitigation:

- keep review mode read-mostly by default
- make posting and code-changing behavior explicit opt-ins

## Open Questions

- **OpenCode boundary**: Should Roger interface with OpenCode through stable CLI
  or file-based boundaries only, or is some internal coupling acceptable? This
  determines how fragile session linkage is in practice and needs a spike before
  the session-orchestration package starts.

- **TUI ↔ TypeScript protocol**: What is the exact message protocol between the
  Rust TUI binary and the TypeScript application core? Unix socket + JSON
  envelope is the working assumption but the schema needs to be defined before
  implementation starts.

### Resolved questions

- ~~FrankenTUI runtime~~: Confirmed Rust-native. Split-language architecture
  adopted. See Technology Direction section.
- ~~Daemonless bridge mechanism~~: v1 = custom URL protocol (`roger://`). v2 =
  Native Messaging. WebSocket/HTTP rejected as requiring a daemon.
- ~~Semantic search slice~~: Tantivy + FastEmbed following CASS pattern. SQLite
  FTS for v1, Tantivy upgrade for v1.5.
- ~~Credential flows~~: Non-issue. `gh` CLI already handles GitHub auth and
  stores tokens in the OS keychain. Roger inherits this; no separate Keychain
  integration needed for v1.
- ~~`FPs` and `SA` in brain dump~~: Business-specific terms from an unrelated
  project context. Illustrative only, not architectural requirements for Roger.

## Plan-to-Beads Strategy

Convert the plan into beads only after one critique/integration loop confirms
the architecture is stable enough.

The first bead graph should be organized around:

- repo foundation
- shared domain and storage
- OpenCode session orchestration
- prompt pipeline
- CLI
- TUI
- GitHub adapter and extension bridge
- worktree and instance management
- approval/posting flow
- search and memory
- testing and validation

Each bead must include:

- rationale
- dependencies
- exact acceptance criteria
- smoke tests
- whether it is v1-critical or later

## Definition of Done for the Planning Stage

Planning for Roger Reviewer is complete when:

- the canonical markdown plan is internally consistent
- open questions are isolated enough not to block phase 1
- the first bead seed is ready to import into a task system
- the rollout order reflects real technical dependency rather than wishful UI
  ordering
- the safety model is explicit enough to prevent accidental GitHub or local
  environment mutations
