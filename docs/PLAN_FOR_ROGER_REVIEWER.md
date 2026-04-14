# Plan for Roger Reviewer

## Status

Planning, bead polishing, and readiness review completed on 2026-03-30.
Implementation of the local-core-first `0.1.0` slice is now active.

Authoritative readiness artifacts:

- [`READINESS_REVIEW_FIRST_IMPLEMENTATION_SLICE_WITHOUT_EXTENSION.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/READINESS_REVIEW_FIRST_IMPLEMENTATION_SLICE_WITHOUT_EXTENSION.md)
- [`READINESS_IMPLEMENTATION_GATE_DECISION.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/READINESS_IMPLEMENTATION_GATE_DECISION.md)

Current plan-maintenance rule:

- this document is the canonical product and implementation plan
- active hardening directions such as provider-parity truthfulness, explicit
  outbound productization, and user-flow/command-surface completion should be
  folded back into this file once the direction is accepted
- narrower plan docs may still exist for bounded sub-lanes, but they should not
  become long-lived alternative sources of product truth

## Agentic-First Planning And Documentation Doctrine

Roger should follow an agentic-first planning posture:

- planning tokens are cheaper than implementation tokens
- a large, self-contained canonical plan is cheaper and safer than letting
  implementation truth fragment across many small overlapping briefs
- agent execution quality is highest when the core product shape, workflows,
  constraints, and rollout order fit inside one authoritative planning packet
- side plans are useful as temporary synthesis artifacts, but they are not
  allowed to become shadow sources of truth

This means Roger should prefer:

- one dense canonical plan for product shape, workflows, architecture, rollout,
  and major current-scope hardening directions
- narrow support contracts for stable implementation-facing seams
- ADRs for decision records
- the bead seed and live beads for decomposition and proof tracking
- explicitly marked historical critique docs for rationale only
- explicitly marked operator/runbook docs for repo process only

Roger should avoid:

- multiple live planning docs that each partially redefine the product
- leaving accepted design corrections trapped in a side brief instead of
  folding them into the canonical plan
- treating historical critique material as if it were the current spec
- mixing operator swarm/runbook content into the same authority layer as the
  product plan

### Documentation classes

Every Markdown document under `docs/` should fit one of these classes:

1. canonical plan
2. support contract
3. bounded side-plan
4. bead seed or bead/process support
5. historical critique or reconciliation record
6. operator/runbook/process document
7. raw intent or archive artifact

Each class has a different job:

- canonical plan: current product truth
- support contract: narrow implementation obligations that derive from the
  canonical plan rather than compete with it
- bounded side-plan: temporary planning synthesis for one active lane; expected
  to merge back into the canonical plan or a support contract
- bead seed/process support: decomposition and execution guidance, not product
  redefinition
- historical critique: rationale for why the plan changed
- operator/runbook/process doc: repo-operation truth, not product truth
- raw intent/archive: context only

### Documentation cleanup program

Roger's docs cleanup should follow this order:

1. inventory every Markdown doc and assign one documentation class
2. decide one action per doc: keep, merge, archive, move, or delete
3. expand the canonical plan until active product truth is mostly present there
4. keep support contracts narrow and implementation-facing rather than
   duplicating narrative product planning
5. merge accepted side-plan content back into the canonical plan or the
   relevant support contract
6. explicitly mark historical and operator docs so agents do not treat them as
   live product truth
7. only then convert the stabilized direction into beads or implementation work

### Documentation cleanup acceptance criteria

The docs cleanup is successful only when:

- the canonical plan is self-contained enough that an agent can start from
  `AGENTS.md`, this file, and one relevant support contract without needing a
  scavenger hunt across side briefs
- every doc in `docs/` has a clear role and is no longer ambiguous about
  whether it is live truth, bounded support, or historical rationale
- active product rules are not stranded only in temporary side-plan docs
- historical and operator docs are visibly fenced from the product authority
  layer
- broken links, missing doc references, and stale support claims are treated as
  bugs

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
[`docs/roger-reviewer-brain-dump.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/roger-reviewer-brain-dump.md)
and is the name the repo should optimize around unless a later branding pass
changes it deliberately.

## Goals

- Deliver a durable local review workspace centered on findings, artifacts, and
  session continuity rather than one-shot prompt runs.
- Make the TUI the primary power-user interface for triage, follow-up, and
  approval.
- Support a fully usable local-first CLI/TUI workflow even when the browser
  extension is not installed; the extension is an accelerator, not a product
  prerequisite.
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
- The browser extension is optional. Roger must remain a coherent product when
  used entirely from the shell and local TUI.
- Review is read-heavy and latency-sensitive, so local indexing matters.
- Searchability and promotability are different; broad evidence storage does not
  imply broad reusable memory.
- Scope is a hard boundary, not just a ranking hint; `repo` is default and
  `project` / `org` are explicit overlays.
- Memory freshness must never block review; degraded lexical-only behavior is
  acceptable when semantic or promoted memory is unavailable.
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
- The Rust TUI runtime can integrate cleanly without forcing a second,
  conflicting application architecture.
- A narrow, local-only hybrid retrieval slice can ship from the first Roger
  search milestone without turning embeddings into a blocker for the core review
  loop.

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

1. User opens a GitHub PR in Chrome, Brave, or Edge.
2. Extension injects a Roger entry point appropriate to the chosen bridge and
   v1 scope.
3. User chooses a review action such as start review, resume review, or refresh
   findings.
4. Extension passes PR context to a local Roger launcher using a daemonless
   bridge.
5. Roger creates or reuses a local review session, records the repo snapshot,
   prepares a worktree only if the chosen flow needs isolation, and opens the
   TUI or CLI flow.
6. Review progress and unresolved findings become visible locally and, only if
   the chosen bridge supports it cleanly, through explicit extension actions or
   status reads.

### Workflow 2: Launch a review from the shell

1. User runs a session-aware CLI command such as `rr review`, `rr resume`, or
   `rr findings`.
2. Roger infers repo and branch context from the current working directory when
   possible.
3. Roger resolves the related PR if one exists remotely or accepts explicit PR
   input.
4. Roger resumes or starts the underlying supported-harness session.
5. Roger opens the TUI or prints actionable CLI output depending on mode.

This workflow is not a fallback-only path. Roger should remain directly useful
without the browser extension installed.

### Workflow 3: Launch from an external collaboration signal

1. A user sees an external business event, such as a team notification that a
   PR is ready for review.
2. That external surface should stay thin: ideally it deep-links to the GitHub
   PR and, where practical, offers a one-click Roger launch action.
3. Roger should not treat external collaboration tools as the canonical review
   queue or state store.
4. If the external surface launches Roger directly, it should pass the same
   structured launch intent used by the CLI and extension rather than inventing
   a separate workflow.
5. If richer notification routing is added later, it should subscribe to
   Roger-owned attention events rather than forcing Roger to poll or centralize
   another team's workflow system.

### Workflow 4: Conduct the review

1. Roger stages prompts in a deliberate sequence: explore first, then deep
   analysis, then further passes only if they still produce value.
2. Findings are captured as structured records rather than free-form terminal
   output.
3. Each finding can be marked accepted, ignored, needs follow-up, ask-in-GitHub,
   or similar explicit states.
4. A user may ask clarifying questions about a finding without changing its
   triage or outbound state.
5. Clarifying questions can be attached to findings in a structured way.
6. Review artifacts, prompts, and intermediate outputs are retained locally for
   later resume, refresh, or audit.

### Workflow 4.1: Clarify a finding without mutating it

1. User opens a finding but is not yet confident whether it is real or how the
   codebase context should be interpreted.
2. User asks a local clarifying question from the TUI such as "explain why this
   is likely a bug", "show the surrounding call path", or "what assumption is
   this finding making?"
3. Roger runs a bounded clarification step linked to the existing finding and
   session.
4. The answer returns as local explanatory material, not as an automatic state
   transition or outbound draft.
5. The user can then triage the finding, ask another question, or leave the
   finding untouched.

### Workflow 5: Refresh after new commits

1. User refreshes a review after a PR changes.
2. Roger pulls new metadata and diffs, then runs a fresh-eyes pass.
3. Prior high-signal findings are selectively reintroduced so the system does
   not start from zero.
4. Findings that remain relevant are carried forward; resolved or obsolete ones
   are marked accordingly.

### Workflow 6: Respond to review attention events

1. Roger may reach a state where it is waiting on the user rather than on more
   analysis.
2. Canonical attention states should include: review launched, review awaiting
   user input, outbound approval required, review completed with findings ready,
   refresh recommended after new commits, and review failed or needs manual
   recovery.
3. The TUI and CLI should expose these states directly.
4. Other surfaces such as a browser extension, a desktop notification, or a
   future collaboration hook may mirror them, but should not redefine them.
5. The design goal is local clarity without turning Roger into a polling-based
   workflow platform.

### Workflow 7: Approve outbound actions

1. Roger prepares proposed GitHub comments, questions, or suggestions in local
   draft form.
2. User reviews and edits them in the TUI or another local interface.
3. Only after explicit approval does Roger use `gh` CLI or another adapter to
   post them.
4. Roger stores the mapping between local finding state and remote review
   action.

## Current User-Flow Hardening Priorities

The target workflows above remain correct, but the live repo still has several
current-scope productization gaps that must be treated as implementation work,
not as optional polish.

### U1. Review launch must become a real review flow

- `rr review` is expected to mean more than provider/session bootstrap
- the active scope is to finish the default stage pipeline, structured findings
  intake, and handoff into the local decision workspace

### U2. Repo-local re-entry needs a clear workspace path

- `rr resume`, `rr status`, `rr findings`, and `rr sessions` should separate
  quick probe behavior from true workspace entry
- ambiguity must fail closed into an explicit session picker instead of hidden
  guessing

### U3. Dense local workspace entry needs an explicit contract

- Roger still needs one clearly blessed way to enter the dense local review
  workspace, whether through `rr review` / `rr resume` handoff or a dedicated
  command such as `rr tui` / `rr open`
- the TUI shell model is not, by itself, sufficient proof of an operator-usable
  runtime path

### U4. The full local review loop must be command- and UI-driven

- a truthful local review loop means: launch review, run stages, materialize
  findings, triage findings, review drafts locally, approve explicitly, and
  post explicitly
- this flow must be defended without manual store seeding in the proving
  acceptance path

### U5. Refresh must reconcile findings and approval state

- `rr refresh` must evolve from continuity relink plus run recording into a
  real refresh/reconciliation workflow
- draft invalidation and reconfirmation after refresh are part of the same flow
  rather than separate optional cleanup

### U6. Browser launch must dispatch real Roger commands

- the extension may stay bounded and launch-focused, but the serious bridge path
  must invoke the real `rr` command and return canonical Roger state or fail
  closed
- launch-only honesty is acceptable; placeholder success is not

### U7. Return must land back in Roger truthfully

- `rr return` should not stop at provider continuity rebinding
- the product flow needs a clear local workspace target after return, while
  keeping provider support claims narrow and literal

### U8. The TUI shell must become a real operator cockpit

- the current shell proves useful structure, but current-scope implementation
  still needs a real operator cockpit with durable home, findings, drafts,
  sessions, search/history, bounded composer, and prompt/discoverability
  affordances
- first-release TUI work should optimize for low navigation cost, stable
  selection, explicit mutation elevation, and truthful degraded states rather
  than merely increasing pane count

### U9. Product correctness must include interrupted and degraded real-world use

- current-scope correctness includes long-lived sessions, refresh churn,
  approval invalidation, stale evidence anchors, bridge/setup drift, posting
  failures, partial findings repair, and dropout/return continuity
- Roger should not treat "works in the clean happy path" as sufficient proof for
  UX or support claims when ordinary real-world operator interruptions produce a
  different outcome

### U10. Validation and proof must track real operator promises, not only nominal success

- each user-facing surface needs the cheapest truthful proof layer for both the
  nominal path and the failure/degraded path that materially affects the same
  promise
- a single generic end-to-end succeeds-only test is not an acceptable
  substitute for narrower proof of fail-closed launch, refresh invalidation,
  recovery UX, and operator-visible repair states

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
- That fallback path should include Roger-control context such as the active
  review target, review mode, loaded skills or instructions needed to keep the
  session Roger-compliant, and the minimum attention/finding state needed to
  avoid drifting into an unrelated session.

### Evidence collection posture

Roger is agent-first, not static-analysis-only.

That means:

- when the active safety posture allows it, Roger may collect evidence from
  runtime inspection, debugger traces, test execution, local service
  interaction, or controlled data setup rather than limiting itself to static
  code inspection
- those capabilities exist to improve review quality, not to blur Roger into an
  autonomous mutation framework by default
- the outbound GitHub surface for ordinary PR review should still compress down
  to a small number of human-reviewable comments, questions, or suggestion
  blocks rather than dumping the full investigative trace into the PR

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
│   ├── adr/
│   ├── PLAN_FOR_ROGER_REVIEWER.md
│   ├── BEAD_SEED_FOR_ROGER_REVIEWER.md
│   ├── ALIEN_ARTEFACTS_FOR_ROGER_REVIEWER.md
│   ├── PLANNING_WORKFLOW_PROMPTS.md
│   └── roger-reviewer-brain-dump.md
```

## Technology Direction

### Confirmed: Roger needs a Rust TUI layer

Exploration of Rust-native TUI options resolved the runtime question for the
presentation layer: Roger's local cockpit should stay Rust-native for `0.1.x`.
`FrankenTUI` is the current TUI dependency, but the reason for the choice is
Roger's own runtime shape rather than loyalty to an external project.

It also establishes the default bias for the rest of Roger: favor Rust for the
local runtime unless a platform constraint clearly justifies another language.
The browser extension is the main expected exception because it is inherently
web-native.

**Confirmed TUI layer: Rust**
- Roger-owned Rust TUI runtime
- currently implemented on top of FrankenTUI
- diff-oriented terminal rendering and inline interaction are in scope
- synchronous foreground UI loop (no async runtime on the hot path)
- Runs in-process with Roger app-core in `0.1.x`; external surfaces and future
  out-of-process adapters use Roger-owned versioned contracts instead

**Default direction for non-web local layers: Rust**
- Session-aware `rr` CLI commands should default to Rust
- App-core, storage, search, and local orchestration should default to Rust
- Harness integration (OpenCode first, not OpenCode-only) should sit behind a
  Roger-owned boundary regardless of provider
- GitHub adapter logic may shell out to `gh`, but only behind Roger-owned
  adapter boundaries; agent-facing review communication should stay Roger
  mediated, not raw-`gh` driven

**Expected exception: browser extension**
- Keep the browser extension in TypeScript/JavaScript because the platform is
  browser-native
- Keep it as close to zero dependencies as practical
- Prefer browser APIs, direct DOM integration, and small hand-rolled code over
  framework-heavy frontend stacks
- Any JS/TS dependency must justify its vulnerability and churn surface, not
  just developer convenience

### Architecture implication

- The original planning direction toward a Rust TUI holds, but Roger should
  describe the cockpit in Roger-owned terms rather than by naming external
  implementations as if they were product primitives.
- The key decision is the ownership boundary within a Rust-first local runtime,
  not whether Roger should chase a balanced multi-language split as an end in
  itself.
- Roger's Rust-default app-core must expose a stable harness boundary so
  OpenCode is one provider, not the only possible one.

### TUI runtime and concurrency boundary

Decision for `0.1.x`:

- keep the TUI and Roger app-core in-process in the same Rust runtime rather
  than splitting them into separate local processes by default
- treat the Roger TUI's synchronous foreground event loop as the UI authority
  for a given TUI process
- keep one primary `rr` binary with internal mode boundaries for TUI, CLI,
  bridge-host, robot-facing commands, and helper flows rather than assuming a
  small fleet of cooperating binaries
- route the TUI hot path through typed in-process Rust router/domain calls, not
  through mandatory local IPC or an internal message-bus abstraction
- run harness I/O, GitHub/bridge traffic, and other I/O-bound work on a
  dedicated async executor thread behind Roger-owned channels
- run indexing, embeddings, and other heavier compute work on bounded worker
  threads or a small CPU-worker pool
- achieve multi-entrypoint concurrency through the canonical store, append-only
  event history, and per-session conflict/lease rules rather than through a
  resident app-core daemon or a mandatory TUI-to-core IPC layer
- allow multiple Roger processes such as TUI, CLI, bridge host, and agent-owned
  invocations to operate concurrently against one canonical store, with
  same-session writes serialized by Roger's session-level conflict rules
- use immediate local wake signals for same-process background completions plus
  bounded event-stream polling for cross-process TUI refresh

Why this is the right default:

- Roger needs concurrent entrypoints, not a split local-service architecture
- in-process TUI keeps the hot path simpler, faster, and easier to reason about
- the hard concurrency problem is cross-process coordination through the store,
  not UI-to-core remoting inside one local session
- daemonless behavior stays truthful because Roger does not need a standing core
  broker just to let the TUI function

Escalation rule:

- extract a stronger cross-process app-core boundary only if a later editor
  client, crash-isolation requirement, or proven operational bottleneck justifies
  the added complexity

The concrete `0.1.0` defaults for queue classes, queue limits, cancellation,
same-process wake, and cross-process refresh now live in
[`TUI_RUNTIME_SUPERVISOR_POLICY.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/TUI_RUNTIME_SUPERVISOR_POLICY.md).

### Minimum external envelope family

Roger should freeze one small versioned envelope family for real external
boundaries in `0.1.0`. This is for browser bridge, robot-facing outputs where
committed, and later cross-process adapters. It is not a required internal
serialization boundary between the TUI and app-core.

Recommended fields:

- `protocol_version`
- `kind`: `request` | `response` | `event`
- `name`
- `correlation_id`
- `source_surface`: `tui` | `cli` | `bridge` | `harness_command` | `agent`
- `session_id` when bound
- `run_id` when bound
- `instance_id` when relevant
- `ts`
- `payload`
- `ok` for responses when relevant
- `error` for failures

Recommended initial logical names:

- requests: `resume_session`, `refresh_review`, `show_findings`,
  `ask_clarification`, `open_drafts`, `return_to_roger`
- events: `session_updated`, `findings_updated`, `drafts_updated`,
  `attention_state_changed`, `background_job_changed`

Rules:

- stable external edges use ordinary JSON
- internal hot paths stay typed and in-process
- TOON, protobuf, or an internal IPC transport are not required parts of the
  `0.1.x` TUI/core boundary
- `_exploration/asupersync` remains a future reference for richer event fabrics,
  not a `0.1.x` requirement

## Dependency Philosophy

Dependencies must earn their keep. Roger should prefer a small number of
high-leverage dependencies over a wide convenience tree.

Principles:

- prefer standard library, OS facilities, browser APIs, and SQLite before
  adding third-party wrappers
- every dependency should have a clear written reason, bounded ownership
  surface, and an obvious removal path if it stops paying rent
- reject dependencies whose main value is saving a small amount of routine code
  while introducing a large transitive tree
- prefer battle-tested infrastructure dependencies with narrow scope over
  framework-style dependencies that want to become the architecture center
- isolate non-trivial dependencies behind Roger-owned adapters so replacement is
  possible later
- for the browser extension specifically, prefer browser APIs plus minimal
  hand-rolled TS/JS over framework stacks or large bundler trees

Current dependency stance:

- **Likely earned**: FrankenTUI as the current Rust TUI dependency, SQLite,
  Tantivy, FastEmbed if semantic search remains part of the first Roger search
  slice, `gh` CLI as an external runtime dependency, and browser-native APIs
  such as Native Messaging
- **Under active scrutiny**: any heavy TypeScript application framework, broad
  GitHub SDK usage if `gh` CLI is sufficient, local web server stacks for the
  bridge, and convenience wrappers around storage or search that do not add
  durable value
- **Default rejection for extension work**: React/Vue/Svelte-style UI stacks,
  large build pipelines, and dependency-heavy helper libraries unless the
  browser platform forces a capability Roger cannot reasonably implement with
  plain TS/JS and browser APIs

Implementation consequence:

- if an agent swarm can credibly build the narrow subset Roger needs in-repo
  without creating long-term maintenance pain, prefer that over pulling in a
  dependency with a deep vulnerability and compliance surface

Operational caveat for FrankenSQLite:

- if Roger adopts `fsqlite` / FrankenSQLite, treat it as active infrastructure
  rather than mature commodity plumbing
- pin the Rust toolchain deliberately, rebuild and test frequently, and keep the
  storage adapter thin so upstream breakage is isolated
- contribute measured bug reports with minimal repros rather than baking local
  assumptions deep into Roger

## Engineering Posture For `0.1.0`

Roger should prefer common-sense engineering over broad but shallow support
claims.

Rules:

- choose one blessed path per major surface before widening the matrix
- make the primary path excellent before claiming breadth on secondary paths
- favor explicit contracts and typed adapters over ambient magic
- keep mutable review state small, auditable, and fail closed
- keep provider-specific behavior behind Roger-owned boundaries
- prefer append-only history plus materialized current state over lossy
  overwrite-only models
- make degraded modes truthful rather than pretending parity with stronger paths
- treat packaging, install, and upgrade paths as product work, not release
  cleanup
- keep the release/test matrix explicit so unsupported combinations are honest

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
- `Scope`
- `Source`
- `Episode`
- `MemoryItem`
- `EvidenceLink`
- `MemoryEdge`
- `UsageEvent`
- `IndexJob`
- `IndexState`
- `GitHubReviewTarget`
- `WorktreeInstance`
- `ConfigLayer`
- `OutboundDraft`
- `OutboundDraftBatch`
- `PostedAction`

Key rule:

A finding is not just text. It has origin, evidence links, state, outbound draft
mapping, timestamps, review-session lineage, and optionally one or more
normalized code-evidence locations that Roger can show in the TUI and hand off
to a local editor.

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
- when a finding relies on repo code as evidence, Roger should preserve
  normalized code-location anchors separately from generic artifact links so the
  same evidence can support TUI inspection, refresh reconciliation, and local
  editor handoff

Suggested state split:

- triage states such as `new`, `accepted`, `ignored`, `needs-follow-up`,
  `resolved`, `stale`
- outbound states such as `not-drafted`, `drafted`, `approved`, `posted`,
  `failed`

### Code evidence and editor handoff

Roger should treat code-backed evidence as a first-class part of the finding
model, not as an afterthought inside free-form markdown.

Required behavior:

- a finding may carry zero or more normalized code-evidence locations in
  addition to generic evidence links
- each code-evidence location should capture repo-relative path, line/column
  range when available, a bounded excerpt or excerpt artifact, and an evidence
  role such as `primary`, `supporting`, or `contradicting`
- runtime findings that are grounded in logs, screenshots, repro steps, or
  debugger state may omit code locations, but Roger should distinguish
  "non-code evidence" from "code evidence missing or invalid"
- invalid or stale code anchors should not destroy the rest of the finding;
  Roger should mark anchor validity explicitly and preserve the surviving
  evidence set

Editor handoff rule:

- the TUI remains the primary review workspace and source of truth for triage,
  clarification, drafting, and approval
- Roger may expose a thin local `open evidence in editor` action that opens the
  selected finding's primary code location or the full evidence set in a local
  editor such as VS Code
- this editor open path is a convenience affordance over Roger-owned finding
  objects, not a second review client or a replacement for Roger's state model
- if the editor integration is unavailable, Roger should fall back truthfully to
  copyable file/range references rather than pretending parity

### Memory objects and provenance

Roger's durable memory/search layer should be evidence-weighted rather than
transcript-first.

Important consequences:

- `Scope` models repo, project, and org namespaces explicitly
- `Source` records the raw provenance object, version/hash, and origin scope
- `Episode` captures durable review events such as findings snapshots, review
  checkpoints, commit summaries, or policy imports rather than entire transcript
  blobs
- `MemoryItem` holds extracted semantic or procedural lessons with explicit
  state, trust, and normalized identity
- `EvidenceLink`, `MemoryEdge`, and `UsageEvent` explain why a memory exists,
  how it relates to others, and whether it later proved useful or harmful
- raw prompt/tool transcripts remain audit artifacts and cold searchable
  material by exception, not Roger's default reusable memory corpus

## Session Model

Roger should wrap a supported harness session rather than replace it, with
OpenCode as the canonical first-class path in `0.1.0`.

Required properties:

- every Roger review session maps to an underlying supported harness session or
  transcript anchor
- Roger stores additional structured metadata outside that session
- Roger distinguishes harness-specific reopen data from Roger-owned continuity
  data
- if Roger UI state is unavailable, the user can still reopen the underlying
  supported harness session directly when that provider supports it, with
  OpenCode remaining the strongest required fallback in `0.1.0`
- compaction recovery should be able to reinsert selected artifacts, prior
  findings, and prompt-stage summaries into a resumed session

This means Roger metadata must reference, not obscure, the underlying session.

Recommended durability split:

- `SessionLocator`: harness-specific reopen information such as provider,
  session id, and invocation context
- `ResumeBundle`: harness-neutral Roger continuity packet with review target,
  stage summaries, surviving findings, unresolved follow-ups, and bounded
  artifact references
- cold artifacts: raw transcripts, tool traces, prompt logs, and larger payloads
  kept for audit or selective reinjection

Resume should first attempt reopen via `SessionLocator`. `ResumeBundle` exists
to continue the Roger review coherently when the original harness session is
gone, compacted, or no longer useful enough. It is not a promise of full
transcript-isomorphic cross-agent migration.

For deliberate dropout into plain OpenCode, Roger should treat the Roger control
bundle as the operational handoff profile of the same `ResumeBundle`, not as a
separate ad hoc mechanism.

## Harness Support Matrix

Roger should own one harness contract and track provider support explicitly.
Not every provider needs equal status in `0.1.0`.

`0.1.0` support matrix:

| Provider | Roger role | `0.1.0` drop-in support | `0.1.0` deeper integration | Direction |
|----------|------------|-------------------------|----------------------------|-----------|
| OpenCode | Primary review harness | Yes | Yes | The canonical first implementation and fallback path |
| Codex | Secondary bounded review harness | Yes | Bounded | Exposed via `rr review --provider codex`; Tier A only today (no locator reopen or `rr return`) |
| Claude | Secondary bounded review harness | Yes | Bounded | Exposed via `rr review --provider claude`; Tier A only today (no locator reopen or `rr return`) |
| Gemini harness | Secondary bounded review harness | Yes | Bounded | Exposed via `rr review --provider gemini`; keep Tier A live-CLI claims truthful and do not imply locator reopen or `rr return` |
| GitHub Copilot CLI | Active current-scope provider | Not yet | Planned Tier B target | Must land through the same verified-lifecycle and support-claim rules as every other provider |
| Pi-Agent | Future review harness | No | No | Same as Codex |
| GitHub CLI (`gh`) | GitHub adapter, not review harness | N/A | N/A | Read/write adapter for GitHub operations only |

Rules:

- `0.1.0` only needs first-class review-harness support for OpenCode
- Codex, Claude, and Gemini may ship as bounded Tier A live-CLI paths without
  implying Tier B reopen/dropout parity
- GitHub Copilot CLI is active implementation scope, but it should remain out
  of live support claims until the verified launch and continuity path are real
- other providers should influence the adapter shape, not the `0.1.0`
  implementation commitment
- GitHub CLI belongs in the GitHub adapter boundary, not the review-harness
  matrix
- every supported provider should still map into Roger's own durable
  session/run/finding model
- in-harness Roger command support is an optional harness capability, not the
  core product boundary; when present, it should map onto Roger-owned core
  operations rather than provider-specific bespoke behavior

### Harness capability tiers

Roger should classify harnesses by capability tier rather than by brand
reputation.

Scope rule for this plan:

- anything described in this plan is in current implementation scope unless it
  is explicitly marked `v2`, optional, experimental, or otherwise out of scope
- sequence in this document controls dependency and proof order, not whether a
  feature is allowed to slip into an undefined “later”
- if a feature requires enabling hardening, that hardening is part of the same
  feature scope

`0.1.0` capability tiers:

- **Tier A: bounded supported harness**
  - can start a Roger-owned review session
  - can seed from `ResumeBundle`
  - can capture raw stage output durably
  - can feed Roger's structured-findings normalization or repair path
  - can bind the run to a review target explicitly
  - can report continuity quality truthfully enough for Roger to choose reopen
    versus reseed
- **Tier B: continuity-capable harness**
  - everything in Tier A
  - `reopen_by_locator`
  - `open_in_bare_harness_mode`
  - `return_to_roger_session`
- **Tier C: ergonomic harness**
  - everything in Tier B
  - `supports_roger_commands`
  - `describe_roger_command_bindings`
  - `invoke_roger_command`
  - `attach_artifact_reference` when useful

`0.1.0` provider intent:

- OpenCode should reach Tier B and may reach selected Tier C affordances
- Codex, Claude, and Gemini currently expose bounded Tier A paths in the live
  CLI surface and should be documented literally as such
- GitHub Copilot CLI is in current scope as the first serious post-OpenCode
  provider and must land through the same contract as every other provider,
  with verified launch, transaction boundaries, and support-claim discipline as
  inseparable parts of that provider slice
- future providers should be admitted by capability tier, not by one-off
  exceptions

### Support claim rule

Roger should only claim a harness is supported when the provider satisfies the
minimum capability tier Roger is actually promising.

Rules:

- Roger may claim **bounded support** only when the harness satisfies Tier A
- Roger may claim **direct-resume or dropout support** only when the harness
  satisfies Tier B
- Roger may claim **in-harness Roger command support** only when the harness
  satisfies the relevant Tier C affordances
- unsupported capabilities must fail clearly and route the user back to the
  canonical `rr` or TUI path rather than pretending parity
- new harnesses should extend the same capability table instead of adding
  provider-specific contract branches

### First-class provider admission rule

A provider is first-class only when all of the following are true:

- it appears truthfully in the canonical plan, onboarding, README support
  snapshot, and release/test matrix
- `rr review --provider <name>` is actually exposed and reflects the real
  supported tier in help and status output
- Roger records a real provider session identifier before it reports launch
  success
- Roger can defend the claimed continuity tier with live behavior rather than
  only adapter-contract tests or synthetic locators
- Roger controls the provider's write posture, GitHub posture, path scope, and
  audit trail in review mode
- deterministic doubles plus at least one support-appropriate smoke or
  acceptance path exist for the claim being made
- install/auth/policy drift and degraded modes are documented and fail closed

Immediate truthfulness rule:

- Roger must not describe a new provider as “matching OpenCode parity” unless
  the live path actually proves the same tier; planning intent alone does not
  widen a support claim

Future direction:

- cross-harness portability is desirable, but it should stay a v2 concern
- if Jeffrey Emanuel's cross-agent portability work such as CASR proves stable
  enough, evaluate it as an optional dependency behind Roger's own harness
  contract rather than a foundational `0.1.0` dependency
- Roger should still own its canonical session ledger even if later portability
  tooling is adopted

### Future protocol and editor integration methods

Roger should stay protocol-neutral in its core and own its canonical
session/run/finding model regardless of how later harnesses or editor surfaces
connect.

- ACP is a candidate future harness-control adapter once Roger adds a second
  serious non-OpenCode harness beyond the `0.1.0` baseline
- ACP is especially worth evaluating for later Codex, Claude, Gemini, and
  GitHub Copilot CLI/editor-hosted integration paths when those clients expose
  enough session and tool-call control to reduce adapter complexity
- MCP is a candidate future tool/context adapter for exposing Roger resources,
  search, helper commands, and bounded review context to external agents or
  editor hosts
- MCP must not replace Roger's canonical internal IPC, findings ledger, or
  repair loop; it is an edge integration method, not Roger's core architecture
- future editor/client surfaces such as VS Code, JetBrains, and GitHub Copilot
  should be treated as clients over Roger-owned contracts or later ACP/MCP
  adapters, not as the reason to make Roger protocol-first in `0.1.0`
- a thin local editor handoff for opening code-evidence locations is in scope
  for `0.1.x` and does not imply a full editor client or editor-owned state
- any ACP/MCP adoption should happen through focused architecture spikes after
  OpenCode and the initial bounded live-CLI provider paths have validated
  Roger's own harness contract

## Integration Contracts

Before implementation spreads across multiple packages, Roger needs three core
contracts plus one optional harness-command contract.

### Contract 1: Harness session boundary

Roger must define exactly what it reads from and writes to the underlying
supported-harness session layer.

Minimum expectations:

- create or link to a session
- capture enough identifiers to reopen the same session later
- reinsert compact context bundles when resuming
- avoid depending on fragile internal implementation details if a stable CLI or
  file-level boundary exists
- verify harness launch before Roger records a completed-looking review session
- keep lifecycle persistence atomic enough that crash or partial failure cannot
  leave a session looking launched when the harness binding never became real

`0.1.0` provider minima:

- **OpenCode** should support the full primary path: live session linkage,
  reopen by locator when possible, `ResumeBundle` reseed, bare-harness dropout,
  and `rr return`
- **Codex**, **Claude**, and **Gemini** should support the bounded
  live-CLI Tier A path: Roger-owned session/run linkage, prompt intake, raw or
  structured result capture as supported, and truthful `ResumeBundle` reseed
  without claiming locator reopen or `rr return`
- **GitHub Copilot CLI** is active current-scope provider work, but it must not
  be described as live support until the verified launch path, policy profile,
  and continuity story are actually implemented
- no provider should be allowed to bypass Roger's core session ledger, findings
  normalization, or approval model

Recommended capability table:

| Capability | OpenCode `0.1.0` | Bounded live-CLI providers `0.1.0` | Future-provider rule |
|------------|------------------|----------------|----------------------|
| `start_session` | Required | Required | Required for any support claim |
| `seed_from_resume_bundle` | Required | Required | Required for any support claim |
| `capture_raw_output` | Required | Required | Required for any support claim |
| `normalize_or_repair_findings_from_output` | Required | Required | Required for any support claim |
| `bind_review_target` | Required | Required | Required for any support claim |
| `report_continuity_quality` | Required | Required | Required for any support claim |
| `reopen_by_locator` | Required | Not claimed | Required for direct-resume claims |
| `open_in_bare_harness_mode` | Required | Not claimed | Required for dropout claims |
| `return_to_roger_session` | Required | Not claimed | Required for dropout claims |
| `supports_roger_commands` | Optional | Not required | Optional ergonomic layer only |

### Launch truth and transaction rule

Roger must not treat provider start, resume, refresh, or return as a sequence
of loosely related local writes.

Required rules:

- Roger should record a launch-attempt lifecycle distinct from the durable
  `ReviewSession` lifecycle for states such as `pending`, `verified_started`,
  `verified_reopened`, `verified_reseeded`, and explicit failure classes
- a durable `ReviewSession` must not be finalized until the adapter has
  returned a verified `SessionLocator` backed by a real provider session
  identifier
- review launch binding, run creation, continuity updates, and related
  attention-state changes should commit transactionally per user-visible
  lifecycle action
- if provider interaction starts but binding verification fails, Roger should
  retain failure evidence in the launch-attempt ledger rather than leaving a
  completed-looking review session behind

This rule is retroactive for the existing OpenCode/Codex/Gemini slices and is
part of the current provider-support scope. Roger must not treat provider
hardening as a separate optional precursor to provider delivery.

### Continuity-quality decision rule

Roger should classify provider continuity using only three outcomes:

- `usable`
- `degraded`
- `unusable`

Rules:

- `usable` means Roger can continue in the original provider session without
  lying about the review target, run binding, or operator control context
- `degraded` means Roger can continue truthfully only by reseeding from the
  latest `ResumeBundle`, or the reopened provider session exists but does not
  meet Roger's confidence bar for direct continuation
- `unusable` means the provider cannot reopen and Roger cannot reseed
  truthfully enough to continue the review

Roger should only keep using the original provider session when all of the
following are true:

- locator reopen succeeded
- the effective review target still matches
- the adapter reports `usable`
- the user did not explicitly request a fresh session

If any of those fail, Roger should reseed from `ResumeBundle` or fail closed.

### Contract 1A: Harness command boundary

For harnesses that support slash commands, subcommands, or equivalent in-session
command affordances, Roger should expose a thin Roger-owned command surface.

This command surface is optional per harness, but when it exists it must map to
the same canonical core operations as the `rr` CLI.

Recommended canonical operations:

- `resume_session`
- `return_to_roger`
- `show_status`
- `show_findings`
- `refresh_review`
- `ask_clarification`
- `open_drafts`
- `show_help`

Recommended command objects:

- `RogerCommand`
  - `command_id`
  - `review_session_id`
  - `review_run_id` when relevant
  - `args`
  - `invocation_surface` such as `cli`, `tui`, `harness_command`
  - `provider`
- `RogerCommandResult`
  - `status`
  - `user_message`
  - `next_action`
  - `session_binding`
  - optional payload or deep link target
- `HarnessCommandBinding`
  - `provider`
  - `command_id`
  - `provider_command_syntax`
  - `capability_requirements`

Rules:

- command IDs should be Roger-owned and stable even if command syntax differs by
  harness
- unsupported commands should fail truthfully and point the user to the
  equivalent `rr` path
- command handlers should live in Roger core/CLI routing, not in provider-
  specific prompt glue
- harness command support must never be the only way to access a Roger function

`0.1.0` command stance:

- no harness is required to support Roger-native commands in `0.1.0`
- OpenCode may expose a small safe subset if the adapter can do so cleanly
- Gemini is not required to expose any Roger-native in-harness commands in
  `0.1.0`

If a harness does expose Roger-native commands in `0.1.0`, the preferred first
subset is:

- `roger-help`
- `roger-status`
- `roger-findings`
- `roger-return`

The following remain optional even for capable harnesses:

- `roger-refresh`
- `roger-clarify`
- `roger-open-drafts`

Approval, posting, and other mutation-capable actions remain explicitly
elevated in the TUI or canonical `rr` flow.

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
- no “pretend success” response from the bridge before the real `rr` command
  succeeds and returns a canonical Roger session id

Bridge realism rule:

- the serious bridge path should validate preflight, invoke the real `rr`
  command in machine-readable mode, and return Roger-owned ids or fail closed
- success responses with missing or synthetic session ids are not acceptable
- setup/doctor flows are useful operator checks, but they do not substitute for
  a real host-process request/response proof

### Contract 2A: Bootstrap and doctor boundary

Roger should expose one truthful bootstrap/preflight surface across providers
instead of leaving onboarding guidance to drift.

Required rules:

- either `rr init` must exist as the canonical bootstrap command, or every doc
  and recovery path must route to the real bootstrap surface Roger actually
  ships
- `rr doctor` should become the cross-provider preflight/debug surface for
  install, auth, policy, and bridge health checks
- onboarding, quickstart text, and CLI help must name only command surfaces
  that actually exist in the product

### Contract 3: Outbound posting boundary

Roger must separate finding generation from GitHub mutation.

Minimum expectations:

- outbound drafts are materialized locally first
- approval is explicit and reviewable
- the exact payload posted to GitHub is snapshotted for audit
- local state records success, failure, and remote identifiers
- agents should not send review comments, questions, or suggestions through raw
  `gh` commands or other direct write tools; Roger owns that communication path
- the product surface must expose the draft -> approve -> post path explicitly
  rather than only implying it through storage state or planning prose

Required visibility rules:

- Roger should expose a first-class CLI or TUI command family for draft,
  approval, and posting transitions
- outbound state should be queryable as at least `drafted`,
  `awaiting_approval`, `approved`, `posted`, `superseded`, and `invalidated`
- refresh, retarget, or repo-snapshot drift must revoke stale approvals before
  posting is available again

### Cross-review posting safety invariants

Roger should make accidental cross-review posting structurally hard rather than
relying on UI caution alone.

Required invariants:

- every `OutboundDraft` must carry an immutable target tuple including at least
  `repo_id`, `review_session_id`, `review_run_id`, `provider`, remote review
  target, and the repo snapshot it was derived from
- approval must bind to the exact draft or batch payload hash plus that target
  tuple rather than to a loose "approved" flag
- the GitHub adapter must post only from explicit stored target identifiers, not
  from ambient "current PR" or "currently focused review" state
- if repo target, PR target, base/head commit window, thread anchor, or grouped
  draft membership changes, prior approval is revoked automatically and the
  draft returns to a reconfirmation-required state
- multiple findings may be grouped into one outbound batch, but the batch must
  still belong to exactly one review target and one owning review session
- any attempt to post from the wrong repo, wrong PR, stale anchor, or stale
  session binding should fail closed and surface a local repair/review path

Practical consequence:

- Roger should treat outbound approval as approval of a specific rendered payload
  for a specific target, not generic permission to post "something like this"
- refresh, rebase, rerun, or instance retargeting should invalidate affected
  approvals automatically before the posting path is available again

These invariants are expanded in
[`DATA_MODEL_AND_STORAGE_CONTRACT.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/DATA_MODEL_AND_STORAGE_CONTRACT.md).

## Execution Truth Rules

Roger already has planning and bead history showing a failure mode where the
graph can look healthy while the live product still relies on synthetic launch
semantics or partially productized flows. The plan must explicitly prevent that
pattern.

### Why prior provider/support work could look complete without being complete

The main failure modes were:

- contract beads and adapter beads closed on schema/routing/unit coverage before
  a separate proof unit existed for verified live launch and transactional
  persistence
- provider tests relied heavily on synthetic `SessionLocator` values, local
  store inserts, and stub binaries, which are useful but do not by themselves
  prove first-class support
- epics could close when child beads were all closed even if the remaining gap
  was a cross-cutting truthfulness issue rather than a missing single package
  implementation
- queue health became a misleading proxy for product health; an empty `br ready`
  frontier did not mean the support claim was truly defended

### Anti-gap rules for future plan-to-bead execution

- provider-admission work must include an explicit “live launch truth” bead that
  owns verified session binding, failure classes, and non-synthetic session-id
  evidence
- provider parity must be split into at least four independently provable
  slices: lifecycle truthfulness, transactionality/crash safety, outward
  product surface, and provider-specific integration
- a provider-support epic must not close on adapter/double coverage alone if
  the user-facing claim depends on real CLI exposure, bridge realism, or
  operator-facing launch proof
- docs and README support wording must narrow immediately when implementation
  proof regresses or remains bounded; the plan should never assume future proof
  will arrive “soon enough”
- if the graph reaches an empty or nearly empty frontier while a named support
  claim is still visibly unproven, shaping new proof beads is mandatory rather
  than optional cleanup

### Recommended implementation order for this gap

1. harden lifecycle truthfulness and transactional launch boundaries
2. make outbound approval/posting a visibly complete product surface
3. re-audit OpenCode, Codex, and Gemini support claims against the live CLI
4. land GitHub Copilot CLI through the same truth rules, not as a shortcut

## Storage and Indexing Strategy

### Source of truth

Use a local SQLite-family database as the canonical store for review sessions,
findings, scopes, memory items, artifacts, status, and index metadata. Keep one
canonical Roger store per user profile, with large raw artifacts in a sibling
content-addressed directory.

### Required capabilities

- transactional local writes
- session-level conflict detection or writer leasing for mutable review
  aggregates
- schema migration support
- fast relational lookup
- generation-aware lexical and vector search sidecars rebuildable from the
  canonical store
- explicit degraded-mode reads if semantic or index sidecars are unavailable

### Recommendation

Use Tantivy as the primary lexical engine and ship a narrow local semantic
sidecar from the first real Roger search slice. Do not start with SQLite FTS
and plan a later migration, but also do not make semantic indexing a gating
dependency for the basic review loop.

Reasoning:

- The TUI is already Rust. Tantivy and local embedding support live naturally in
  the same local runtime and do not force a new networked service boundary.
- SQLite FTS → Tantivy migration is an annoying and inevitable reindex. Skip
  the intermediate step.
- CASS (`_exploration/cass`) is the reference pattern for an authoritative local
  store plus fast lexical and semantic sidecars. Roger should copy that
  retrieval posture without inheriting CASS's broader flywheel or global-memory
  assumptions.
- Tantivy gives prefix matching, edge n-grams, and a proper query language from
  day one. SQLite FTS5 does not.
- SQLite remains the relational store for sessions, findings, and config — only
  the full-text index moves to Tantivy. These are complementary, not competing.
- The first shipped hybrid slice should keep the semantic corpus narrow:
  findings, session summaries, repo docs, commit/issue summaries, and promoted
  rules rather than raw code files or raw tool transcripts.
- Roger should not hard-couple v1 to a specialized vector-file format before
  measured need. A simple local vector sidecar is enough initially.

Roger should not plan a text-only search launch followed by a later semantic
retrofit. The implementation sequence can still keep search off the critical
path for the first end-to-end review loop, but the first Roger search slice
should include both lexical and semantic retrieval.

### Index generations and degraded mode

- foreground writes go to the canonical database first and mark dirty rows or
  dirty ranges
- same-process background workers handle lexical reindexing, embeddings,
  candidate extraction, dedupe, decay, and promotion evaluation
- the query path serves the committed index plus a small dirty overlay when
  needed
- if semantic search is unavailable, Roger returns lexical-only results
- if lexical/vector sidecars are missing or corrupt, Roger falls back to DB scan
  and file/doc search
- rebuilds create a fresh lexical/vector generation from the canonical DB
  snapshot and atomically swap it in

### Artifact strategy

- Store metadata and normalized excerpts in the database.
- Store larger raw artifacts in a local content-addressed artifact directory if
  they become too large for comfortable inline DB storage.
- Keep database rows small enough that the TUI remains responsive.
- Define artifact budget classes early so prompt transcripts, diff chunks, and
  large reference payloads do not bloat the primary tables accidentally.
- Keep raw prompt/tool traces in cold artifacts by default; promote excerpts or
  summaries only when they become durable evidence.

See
[`DATA_MODEL_AND_STORAGE_CONTRACT.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/DATA_MODEL_AND_STORAGE_CONTRACT.md)
for the hot/cold/derived storage split, aggregate ownership, and concurrency
rules.

## Search and Memory Strategy

### Day-one search posture: hybrid, narrow, and repo-first

Roger should ship hybrid retrieval in its first real search slice, but the
semantic corpus must stay narrow, local-only, and best-effort. Lexical
retrieval is primary. Semantic retrieval is supplemental over a curated corpus,
not a gating dependency for starting or resuming a review.

Initial curated semantic corpus:

- promoted semantic/procedural memory
- repo docs, ADRs, and policy excerpts
- session summaries, accepted findings, and user notes
- commit/issue summaries and compact path/symbol descriptors

Do not embed raw full code files or raw prompt/tool transcripts in v1.

### Memory classes

- **Working memory**: current PR, diff, open files, current task state, unsaved
  notes. Ephemeral and not a promotion target.
- **Episodic memory**: past review sessions, findings, approvals/dismissals,
  linked commits/issues, and outcome snapshots. Searchable immediately.
- **Semantic memory**: extracted facts and durable patterns. Candidate first,
  promoted later.
- **Procedural memory**: review playbook rules and policy constraints. Mostly
  human-authored or explicitly validated.

### Scope model

- default search scope is `repo`
- `project` and `org` scopes are explicit overlays, not automatic inheritance
- there is no automatic repo → project → org fallback when repo results are weak
- lexical and vector indices stay partitioned by scope and are unioned only when
  the session explicitly allows it
- broader-scope results should surface in separate buckets such as
  `repo_memory`, `project_overlay`, and `org_policy` so provenance is never
  flattened away

Architectural hooks to preserve now:

- tag indexed documents, findings, and artifacts with explicit scope identity
- keep search APIs scope-aware and approval-aware
- treat cross-repo/project/org sharing as an explicit workflow with audit trail
- keep the review domain agnostic to whether later sharing stays local-only or
  uses an explicit transport such as a mailbox-style flow

### Temporal and trust model

Memory items should carry both when Roger learned something and when it
actually applied. Store `observed_at`, `effective_from`, and `effective_to`
whenever the evidence allows it.

Trust rules:

- canonical checked-in docs outrank inferred rules
- negative evidence counts more than positive evidence for review heuristics
- stale or contradicted lessons should demote faster than helpful lessons
  promote

### Retrieval pipeline

Use a deterministic pipeline:

1. derive anchors from the current review target: file paths, symbols, tests,
   issue refs, dependency names, and finding classes
2. apply hard filters for allowed scopes, trust floor, memory type, and anchor
   overlap
3. run lexical retrieval in Tantivy as the primary retriever
4. run semantic retrieval over the curated local vector corpus
5. fuse with weighted RRF, biasing lexical above semantic
6. optionally expand one or two hops over typed edges such as `supports`,
   `same_module`, `same_issue`, `supersedes`, and `contradicts`
7. rerank by scope proximity, anchor overlap, canonicality, successful prior
   use, and recency
8. abstain if the result set is weak or out of scope

Initial weighting guidance:

- lexical dominates by default
- semantic is supportive rather than authoritative
- `repo` beats `project` beats `org`
- exact anchors beat generic similarity

### Searchable versus promotable material

Searchable immediately:

- repo docs, ADRs, and policy files
- Roger findings, decisions, notes, and session summaries
- commit summaries and linked issue summaries

Cold by default:

- raw prompt/tool transcripts and oversized artifacts remain stored for audit,
  but they are not first-class reusable memory unless explicitly pinned

Promotable:

- extracted facts, heuristics, and procedures backed by evidence links and later
  validation

Required retrieval lanes:

- `promoted_memory`: established/proven items eligible for ordinary retrieval
  and prompt injection
- `tentative_candidates`: candidate items surfaced only in high-anchor-overlap
  contexts or on explicit user request
- `evidence_hits`: searchable raw evidence such as findings, docs, and episodic
  history

Candidate memory must not silently behave like promoted memory.

### Promotion, decay, and conflict rules

Use explicit memory states:

- `observed`
- `candidate`
- `established`
- `proven`
- `deprecated`
- `anti_pattern`

Initial promotion rules:

- `observed -> candidate` after extraction produces a structured fact/rule with
  at least one evidence link
- `candidate -> established` after two independent supporting episodes, explicit
  human promotion, or conservative import from a bound canonical source
- `established -> proven` after repeated successful approved use, merged-fix
  backing, or import from a bound canonical policy set
- demote or deprecate on contradiction, repeated dismissal, harmful outcome, or
  major anchor change
- mark harmful lessons as `anti_pattern` so they surface only as warnings

Usage/outcome model:

- store atomic usage events such as `surfaced`, `cited`, `applied_to_finding`,
  `applied_to_draft`, `approved`, `posted`, `merged`, `dismissed`,
  `contradicted`, and `marked_harmful`
- derive labels such as `helpful`, `approved`, `merged`, and `harmful` from
  those events rather than storing only a single coarse outcome flag
- require a Roger-owned resolution link for `merged` validation; that link may
  come from a posted draft or from a traceable local-only Roger recommendation
- treat allowlisted canonical policy sources differently from ordinary docs:
  repo `AGENTS.md`, Roger policy files, and explicitly bound ADR/policy
  directories may auto-seed high-trust memory; generic `README.md` and
  `CONTRIBUTING.md` should not auto-promote by default

Decay must be change-aware, not just time-based. Candidate heuristics can expire
quickly. Established and proven items decay more slowly unless canonical.
Episodic history tied to audit should be archived or cold-ranked rather than
hard-deleted. Repo epochs, dependency major-version changes, and policy
revisions should trigger reevaluation.

### Duplicate suppression and index lifecycle

- deduplicate within the same scope and memory type using normalized text,
  anchors, and near-duplicate similarity
- across scopes, link duplicates as aliases rather than merging them
- represent conflicts explicitly with `contradicts` or `supersedes` edges
- if two high-trust items still conflict, show both with provenance rather than
  silently flattening them

Recommended trigger points:

- **finding created / edited / dismissed / approved**: write DB state, mark
  dirty rows, refresh candidate links
- **user note or manual rule edit**: write DB state, reindex the note/rule, and
  reevaluate duplicates
- **session checkpoint / end**: finalize episodic summary and outcomes, then
  extract candidates, dedupe, decay, and vectorize
- **new commit / rebase / merge-base change**: record commit metadata, reindex
  commit/path summaries, and invalidate affected anchors
- **repo doc / policy file change**: update the source version and revalidate
  dependent memory items
- **binding or scope change**: update allowed overlays and build any missing
  overlay indices
- **schema / tokenizer / embedding change or corruption**: rebuild the affected
  sidecar from the canonical DB snapshot

This design preserves day-one hybrid retrieval while keeping review safety,
provenance, and degraded-mode behavior explicit.

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

## Structured Context Packaging

Roger should keep canonical storage and internal IPC in ordinary typed rows and
compact JSON. TOON is only an optional prompt packer for specific large,
uniformly structured payloads.

Good TOON candidates:

- findings tables
- retrieved memory cards
- commit/issue summary tables
- evidence matrices

Default behavior:

- compact JSON remains the default prompt packaging format
- TOON should sit behind a feature flag and model-specific smoke tests for
  structural correctness
- TOON must not become Roger's canonical storage format or required IPC layer

## Structured Findings Contract and Repair Loop

Roger should treat LLM-generated structured findings as a first-class review
artifact rather than an optional decoration on top of raw text.

Rules:

- each review stage should attempt to produce a structured findings pack in a
  Roger-approved schema
- TOON is allowed as a model-facing findings format when it improves efficiency
  or reliability for a supported model/backend; compact JSON remains acceptable
- Roger canonical storage still normalizes the accepted findings into Roger-owned
  rows and linked artifacts
- raw model output must always be preserved and viewable, even when structured
  extraction fails

Failure-handling posture:

- validate the findings pack incrementally rather than all-or-nothing
- salvage any finding, evidence link, or artifact reference that is fully valid
- classify failures explicitly: missing pack, malformed pack, schema drift,
  partial pack, invalid anchors, contradictory state, or transport/runtime
  failure
- send concise machine-readable repair feedback back to the LLM when repair is
  likely to succeed
- retry with an explicit bounded budget rather than looping blindly
- surface degraded but truthful UI states such as `raw only`, `partial
  findings`, or `repair needed`

The guiding principle is browser-like tolerance with auditability: parse and use
whatever is provably valid, preserve the raw source, and ask the model to repair
the rest instead of discarding the whole review artifact.

External inspiration such as SARIF may inform fingerprints, locations,
rule/result metadata, and export adapters, but Roger's canonical findings model
must stay broader because Roger can also carry runtime evidence, clarification
threads, approval state, and repair lineage that are outside static-analysis
interchange formats.

## TUI Requirements

The TUI is the default power-user workspace.

It should provide:

- session list and resume entrypoints
- current review overview
- itemized findings list
- finding detail view with linked artifacts, evidence, and code-location anchors
- state transitions such as accept, ignore, follow-up, ask-in-GitHub
- outbound draft review and approval
- history or audit trail for refreshes and prior passes

The TUI must prioritize scan speed. The main view should answer:

- what changed
- what matters
- what still needs a decision
- what is already drafted for outbound action

### TUI surface boundary

The TUI is Roger's primary decision cockpit. If an interaction needs dense
evidence comparison, batch triage, refresh lineage, or outbound approval
editing, it belongs in the TUI first. Other surfaces may mirror entrypoints or
bounded summaries, but they should not become a second full review workspace.

### Schema alignment for TUI surfaces

The TUI must project canonical Roger entities rather than inventing shadow
objects.

Rules:

- the primary operator queue item is a `Finding`, not a generic issue row or UI
  card
- findings queues, filters, and selections operate on canonical `Finding`
  identity
- finding detail and evidence inspection project `Finding`,
  `CodeEvidenceLocation`, clarification history, and linked outbound-draft state
- session views project `ReviewSession`, recent `ReviewRun` history, and
  canonical `AttentionState`
- draft approval views operate on `OutboundDraft` items grouped into
  `OutboundDraftBatch`, because approval and posting happen at the batch level
- prompt palette and prompt reuse operate on `PromptPreset` and
  `PromptInvocation`
- transient UI selections, overlays, and command palettes are controller state,
  not new durable domain aggregates

### Default TUI information architecture

The default TUI should be organized around a small number of operator views:

- **Review Home**: active and recent sessions, attention queue, launch/resume
  entrypoints, refresh recommendations, and global session-finder access
- **Session Overview**: current PR snapshot, active run state, findings counts
  by triage/outbound state, and any blocking attention events
- **Findings Queue**: sortable, filterable, groupable findings table optimized
  for fast triage
- **Finding Inspector**: normalized summary, code-evidence set, generic
  evidence links, artifact previews, related prior findings, refresh lineage,
  and draft linkage
- **Draft Approval Queue**: outbound drafts grouped by file/thread/target with
  edit, approve, reject, and audit-preview actions
- **Timeline and History**: prior runs, refresh deltas, stage outputs, and
  posted-action lineage
- **Search and Recall**: scoped lookup across prior findings, artifacts,
  summaries, and promoted memory without leaving the review context

### Active review workspace shape

The active review workspace should normally expose three simultaneous regions:

- a top status strip with target identity, review/run state, refresh status, and
  pending-attention counts
- a primary working pane that flips between overview, findings queue, drafts,
  history, and search
- a secondary inspector pane for the currently selected finding, draft, artifact,
  or prior-run item

This is intentionally denser than a typical dashboard. Roger is a reviewer tool,
not a casual browsing UI.

### Required TUI interactions

- keyboard-first navigation with predictable shortcuts for switching queues,
  filtering, and changing state
- multi-select and batch triage for repetitive decisions
- fast grouping and filtering by file, severity, finding state, run, and draft
  status
- evidence-first drilldown: file/diff anchor, excerpt, artifact digest, and run
  provenance should be one action away from a finding
- open the current finding's primary code location or full code-evidence set in
  a configured local editor such as VS Code without leaving Roger's ownership of
  state
- explicit refresh comparison so users can tell which findings are carried
  forward, newly introduced, stale, or resolved
- a non-mutating clarification action from finding detail so the user can ask
  "help me understand this" without changing finding state
- local-only state transitions for `accept`, `ignore`, `needs-follow-up`,
  `resolved`, and `ask-in-GitHub`; `ask-in-GitHub` still creates a draft rather
  than posting anything

### Clarification and dropout behavior

The TUI should support two distinct "I need help" behaviors:

- **clarify in place**: ask a bounded question about the currently selected
  finding and keep the user inside Roger
- **drop out intentionally**: open or resume the underlying OpenCode session as
  the default escape hatch, with a compact Roger control bundle so the user or
  agent can continue outside the Roger shell without losing review discipline

The second case is not a failure fallback only. It is a legitimate operator move
when the user wants a more direct harness experience for a while.

Roger may also offer a secondary explicit handoff into equivalent local `rr`
command paths, but the default dropout target from the TUI should be the
underlying harness rather than a second Roger shell.

### Local chat lane inside the TUI

Roger should support a bounded local chat lane inside the TUI for `0.1.0`.

Required shape:

- support finding-bound clarification tied to one or more selected `Finding`
  objects
- support session-local chat tied to the active `ReviewSession`, with optional
  attached finding references and current working-set context
- use a Roger-owned reference syntax such as `@finding(<id>)` so operators can
  cite findings without manual copy and paste
- keep both modes bounded, auditable, and visibly distinct from raw harness use

Storage and lineage rules:

- finding-bound clarification should remain attached to Roger's clarification
  lineage for the selected finding set
- session-local chat should persist through ordinary `PromptInvocation` history
  and artifacts rather than introducing a second uncontrolled chat subsystem in
  `0.1.0`
- if the operator wants unconstrained harness behavior or the bounded chat lane
  becomes the wrong tool, Roger should make deliberate dropout to the
  underlying harness one action away

### Local editor handoff behavior

Roger should support a thin local handoff from the current finding to the local
editor without turning the editor into the review cockpit.

Required behavior:

- `open primary evidence` should focus the strongest code-evidence location for
  the selected finding
- `open all evidence` should open the finding's full code-evidence set when the
  configured editor supports multi-file open, with the primary anchor focused
  first and supporting anchors opened as additional tabs or locations
- Roger should prefer derived editor-open actions such as `code --goto` or an
  equivalent local launcher rather than introducing a new mandatory background
  service
- editor opens are read-only by default from Roger's perspective; triage,
  draft, and approval state changes still happen through Roger surfaces
- if a code-evidence location no longer resolves in the local repo/worktree,
  Roger should say so explicitly and still show the normalized stored anchor in
  the TUI

Required return behavior:

- Roger should expose an explicit return affordance from the bare harness
  session, such as a lightweight `rr return` command or equivalent helper bound
  to the current Roger session
- where the harness supports commands, the dropped-out session should also
  surface Roger-native command affordances for at least `roger-return`,
  `roger-status`, and `roger-findings`, mapped onto the same core Roger
  operations as the CLI
- if Roger launched the bare harness session itself and still owns the parent
  control flow, exiting that harness session may automatically reopen or refocus
  the Roger TUI for the same session
- automatic return on exit is a convenience path only; the explicit return
  command and ordinary `rr resume` / session-finder flows remain the durable
  fallback

### Draft approval workflow in the TUI

Outbound approval should be a distinct queue, not just a button inside finding
detail.

Required behavior:

- drafts remain inspectable as first-class local objects linked back to their
  source findings and review runs
- users can review drafts individually or as batches grouped by file or review
  thread
- draft editing, approval, rejection, and post-failure recovery happen locally
- the actual GitHub-posting action is visually elevated above ordinary triage so
  mutation never feels implicit
- posted actions remain visible with payload snapshot, remote identifiers, and
  outcome state

### TUI design rule

The TUI should answer two questions with almost no navigation cost:

- what requires a decision right now
- what can be safely postponed without losing important context

### First-release TUI creation focus

Roger should create the first serious TUI around a small set of durable
operator primitives rather than around an expanding list of disconnected views.

Required first-release primitives:

- **attention queue**: one durable answer to "what needs me now?"
- **focusable work queue**: findings, drafts, sessions, search hits, and
  history items behave like related queue objects rather than unrelated widgets
- **stable selection set**: one or many selected items stay stable across
  inspection and bounded actions
- **shared inspector**: one consistent detail region for the focused item
- **composer**: one bounded place to clarify, follow up, or draft from the
  current working set
- **prompt source model**: the operator can distinguish sticky session baseline
  context from one-run modifiers
- **elevated mutation gate**: approval and posting stay visibly separate from
  reading and triage
- **dropout and return bridge**: the operator can leave the cockpit
  deliberately and return without losing control context

Recommended first-release workspace reduction:

- keep five durable operator destinations: `Home`, `Findings`, `Drafts`,
  `Search/History`, and `Sessions`
- keep `Prompt Palette`, help, and the composer as overlays or drawers rather
  than full peer workspaces
- treat additional first-release surface proposals skeptically unless they
  materially strengthen one of the primitives above

### TUI discoverability and prompt control

The TUI should make its control model legible without forcing the operator back
to repo docs.

Required behavior:

- expose a first-class help overlay, conventionally via `?`, that covers
  keybindings, selection grammar, prompt entry, mutation-sensitive actions,
  dropout/return actions, and mouse affordances
- expose a real prompt palette in the TUI rather than burying preset selection
  in config or scattered commands
- render the session baseline separately from one-run modifiers so the operator
  can see what Roger is carrying forward versus what is being injected for the
  next invocation
- support bounded finding references with Roger-owned syntax such as
  `@finding(<id>)` across clarification and session-local chat flows

The prompt palette should remain a first-class operator tool, but not become a
second prompt-authoring product. Favorites, recent, frequent, scope-valid
presets, preview of the resolved prompt, and an optional short explicit
objective are in scope; arbitrary prompt-stack builders and unconstrained
browser-like prompt composition are not.

### TUI real-world robustness expectations

The cockpit must stay truthful under ordinary interrupted usage, not only under
clean demos.

Required expectations:

- selection and current focus should survive ordinary queue/inspector movement,
  refreshes, session switches, and recoverable background failures whenever the
  underlying target is still valid
- stale or invalid code anchors, partial findings packs, repair-needed runs,
  approval invalidation after refresh or retarget, and posting failures must
  surface as bounded operator states with visible next actions
- resize, long queues, multi-session ambiguity, and crash or restart recovery
  are product requirements, not optional polish
- support claims for the TUI require truth under these ordinary interrupted
  operator conditions, not only proof that a static shell or happy-path action
  exists

### Session re-entry and global session finder

Roger should support two complementary re-entry paths:

- **repo-local reinvocation**: if the user runs Roger in a repo directory,
  Roger should try to resume the most relevant session for that repo/PR/branch
- **global session finder**: if the user is not in the right directory, has
  several relevant sessions, or wants to jump across repos, Roger should expose
  a searchable global session picker

Recommended resolution order:

1. explicit session id or deep link wins
2. current working directory plus resolved PR/repo context
3. single strong active-session candidate for that target
4. if ambiguous or absent, open the session finder instead of guessing

The session finder should support at least:

- recent sessions
- active sessions
- sessions awaiting user input
- sessions awaiting outbound approval
- repo/PR search
- pinned or favorited sessions later if they prove useful

## Browser Extension Requirements

The browser extension exists to reduce friction on GitHub PR pages, not to own
core state.

### Implementation stance

The extension should stay low-dependency by design.

Rules:

- prefer browser-native APIs, direct DOM integration, and handwritten TS/JS
- avoid framework-heavy UI stacks by default
- browser runtime dependencies should be zero by default
- a small TypeScript-first build toolchain is acceptable if it materially
  improves contract safety, manifest correctness, or packaging consistency
- avoid large bundler pipelines if simple transpilation or a tiny bundle step is
  enough
- every npm dependency needs a written justification tied to a capability Roger
  cannot reasonably implement itself; runtime dependencies are held to a
  stricter bar than build-time tooling
- dependency count and vulnerability surface are product concerns, not just
  build concerns

### Surface boundary

The TUI is the cockpit. The extension is the PR-page launch, status, and
targeted-handoff surface.

The extension is optional. Roger must remain installable, updatable, and fully
usable as a local CLI/TUI product even when no browser integration is present.

By analogy to the TUI, the extension may expose lightweight versions of review
entrypoints, counts, and attention signals, but anything requiring dense
evidence reading, batch triage, history inspection, or outbound approval editing
stays local.

### Local companion app responsibilities

The extension should talk to a Roger-owned local companion surface. That
companion may be the `rr` CLI or local host binary running in different modes;
it does not need to imply a separate always-on desktop app.

Responsibilities:

- accept structured review-intake payloads from the browser
- resolve or create the matching `ReviewSession`
- open the correct local target (`tui`, `cli`, later another local surface)
- return a bounded session/status snapshot only if the chosen bridge supports
  readback cleanly
- focus a specific local destination such as a finding or draft queue when the
  bridge is strong enough
- enforce that browser-originated actions still land in Roger's normal local
  approval and audit paths

Packaging and platform requirements:

- the companion surface may be the `rr` binary in a dedicated mode or a small
  sibling host binary
- it must support on-demand one-shot launch flows and request/response companion
  flows without becoming a daemon
- release artifacts should target macOS `arm64` and `x86_64`, Windows
  `x86_64`, and Linux `x86_64` and `arm64` as the current truthful first
  shipped subset; Windows `arm64` remains an explicit follow-on lane and must
  stay narrowed in support wording until it is actually built and verified
- Roger `0.1.0` artifact classes should be explicit rather than inferred:
  versioned core binary archives, bridge registration assets for Native
  Messaging, optional browser-extension packages, and
  release metadata such as checksums and install instructions
- the release/devops flow should own checksums, versioned artifacts, and the
  platform-specific registration/install steps for Native Messaging manifests
- browser-extension packages and Native Messaging host assets should be treated
  as separate release lanes from the core local product so Roger can ship an
  honest CLI/TUI release without pretending extension publication is automatic
  or bundled into the one-line installer
- build, packaging, and publication ownership should live in explicit CI/release
  jobs rather than in ad hoc local maintainer steps; the detailed ownership
  split is part of the `0.1.0` release contract and belongs in
  [`RELEASE_AND_TEST_MATRIX.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/RELEASE_AND_TEST_MATRIX.md)

### Installation and update model

Roger should have a straightforward artifact-driven installation story for the
local product, independent of whether the browser extension is installed.

Requirements:

- support a one-line install path that fetches versioned GitHub release
  artifacts and installs the appropriate local binary/binaries for the host
  platform
- support a one-line update path that upgrades an existing local install from
  published release artifacts
- keep the install/update path focused on the CLI/local companion surface first;
  browser-extension installation may remain a separate optional workflow
- publish reproducible release artifacts, checksums, and practical install
  instructions for macOS, Windows, and Linux targets
- ensure CI/CD owns build, packaging, artifact naming, checksum generation, and
  release publication rather than relying on ad hoc local release steps
- treat install/update ergonomics as part of the product surface because Roger
  should be usable without the extension
- require a published checksum manifest for every stable release; platform
  signing expectations may vary by artifact class, but unsigned blessed release
  assets should be the exception rather than the norm

Accepted `0.1.0` contract:

- Roger's one-line install flow targets the local product only: the `rr`
  binary plus any minimal local companion assets needed for the CLI/TUI path
- the install flow must not silently install the browser extension, register
  Native Messaging, or claim browser-launch support as part of the base local
  product path
- the accepted bootstrap shape is a Roger-owned release installer script per
  shell family, with a Unix-style form such as
  `curl -fsSL https://.../rr-install.sh | sh` and a PowerShell form such as
  `irm https://.../rr-install.ps1 | iex`
- those installer entrypoints resolve the latest stable release by default,
  choose the correct archive for the host OS/architecture, download the
  versioned core companion archive plus release metadata, verify the published
  checksum before unpacking, and then install `rr` into the Roger-owned local
  path for that platform
- the installer may accept explicit version or channel arguments such as stable,
  rc, or a pinned tag, but it must never move a user onto prerelease bits
  implicitly
- the accepted one-line update path after install is `rr update`; any
  `rr self-update` phrasing is non-canonical shorthand and should not survive
  in help text, support contracts, or release instructions
- that update command must consult release metadata, stay on the current
  release channel by default, allow an explicit pinned target version, verify
  checksums before replacing the installed binary, and fail closed on mismatch
  or missing metadata
- if the current install was created from a local/unpublished build, the update
  command should refuse silent upgrade and tell the user to reinstall from a
  published release instead of guessing provenance
- `packages/cli` (`roger-cli`) is the canonical shipped CLI package in the
  current workspace because it owns the `rr` binary that release automation
  builds and publishes; `apps/cli` is not packaging authority unless it is
  later wired into the workspace and release pipeline explicitly
- the release/build lane must prove packaged-binary usability, not just
  successful compilation; before archiving or publishing target metadata, Roger
  should smoke the staged `rr` artifact with at least `rr --help`,
  `rr robot-docs`, and `rr update --dry-run --robot`
- truthful source-run support is part of the same delivery contract: Roger must
  not claim a local/dev Cargo path unless the workspace can at least load the
  manifest and render `rr --help` from source
- local installation paths, PATH guidance, and overwrite semantics may vary by
  OS, but they must be Roger-owned and documented in release instructions rather
  than delegated to Homebrew, winget, npm, or another external package manager
- Roger should add signed provenance for blessed release artifacts when the
  signing lane is wired; until then, unsigned targets must be surfaced
  explicitly in release notes and verification output rather than treated as
  silently normal
- the browser extension remains an optional guided setup lane after the local
  product is installed; browser launch onboarding should run through
  `rr extension setup` plus `rr extension doctor` with Native Messaging in the
  normal path, rather than requiring users to manually manage extension ids or
  host-binary paths
- `0.1.x` should keep the unpacked extension artifact as the truthful local
  setup surface while preserving a real path toward future packed/shippable
  extension artifacts that reuse the same setup and identity-registration
  contract
- `1.0.0` should aim to remove dev-labelled extension artifacts from published
  release output so release assets represent only user-facing packaged
  deliverables and their supporting metadata
- the intended follow-on shape is a guided command such as
  `rr extension setup [--browser edge|chrome|brave]` that prepares the
  unpacked/browser-loadable extension artifact, tells the user the one required
  manual browser step, learns the extension id through Roger-owned discovery or
  extension self-registration rather than asking the user to type it, completes
  Native Messaging/helper registration automatically using the installed `rr`
  binary in host mode rather than a normal-path separate `rr-bridge` binary,
  and verifies connectivity truthfully at the end of setup
- Roger should also expose a follow-on verification command such as
  `rr extension doctor` that checks the unpacked extension artifact, discovered
  extension identity, host registration, and bridge reachability without
  pretending browser launch support is healthy when one of those pieces is
  missing
- `rr extension setup` and `rr extension doctor` are not sufficient proof of
  browser-launch support on their own; Roger must also prove that the actual
  registered `rr` binary can complete a Native Messaging request/response round
  trip as a host process rather than relying only on in-process bridge helper
  calls or manifest presence
- `rr extension doctor` should fail closed when any of those checks fail and
  return bounded repair guidance (for example rerun `rr extension setup` for
  normal-path recovery, or use low-level bridge commands only for explicit
  development/repair workflows)
- the product-facing uninstall path for the browser lane is
  `rr extension uninstall`; any retained `rr bridge uninstall` path is a
  demoted repair/development alias rather than normal product help
- lower-level commands such as `rr bridge pack-extension`,
  `rr bridge install --extension-id <id> --bridge-binary <path>`, or
  host-registration subcommands may still exist for development and repair
  work, but they are not the intended primary user-facing setup flow

`0.1.x` implementation-facing PR-page extension entry contract (follow-on UX
hardening after `rr-r3dt`):

- the primary PR-page happy path should feel GitHub-native rather than like a
  floating foreign card
- the happy path should minimize clicks: the core 4-action set (`Start`,
  `Resume`, `Findings`, `Refresh`) should be available directly on the PR page
  without forcing a toolbar popup or intermediate launcher when page seams are
  healthy
- preferred placement order:
  1. attach Roger entry controls into stable PR-page action seams using GitHub-
     native button styling and spacing comparable to first-party actions
  2. when header-style action placement is not stable or not visually coherent,
     render a bounded Roger pane directly in the page DOM above the right-rail
     reviewers card
  3. when neither page seam can be attached truthfully because of DOM drift or
     incompatible layouts, fall back to a Roger modal launched from the page
- Roger entry controls must be additive and must not replace or evict first-
  party GitHub actions such as `New issue`; if no additive seam exists, Roger
  should use the right rail or modal fallback instead of stealing an existing
  action slot
- the browser-action popup remains an explicit manual fallback, not the normal
  first-class PR-page happy path
- the extension must not default back to a floating detached panel when the
  intended GitHub-native seams are unavailable; degraded entry must still feel
  deliberate and bounded
- Roger-owned extension controls should visually borrow from GitHub/Primer
  button patterns and rail-card structure rather than inventing a parallel
  chrome inside the page
- Roger should also develop a distinct brand layer inside those constraints:
  purposeful logo/wordmark work, repeatable accent treatment, and a coherent
  identity that can be embedded across popup, in-page entry, and future Roger
  surfaces without degrading GitHub-native usability
- the extension should expose first-class operator ergonomics rather than
  hiding them in repo docs: safe keyboard shortcuts for primary actions,
  explicit extension configuration, and an in-extension help surface that
  explains actions, shortcuts, setup state, and fallback paths
- where Roger already has enough local state to infer the next safe move, the
  extension and CLI should prefer that over avoidable extra clicks or rerun
  prompts
- practical `0.1.x` examples:
  - demote `Refresh` from an always-visible primary action into a contextual
    action shown when Roger state makes refresh relevant
  - infer one primary CTA from session and attention state instead of treating
    every action as equally likely
  - continue guided extension setup automatically when browser-side identity
    registration is observed rather than forcing an extra setup or doctor
    command when Roger can finish truthfully
  - narrow resume or refresh disambiguation prompts to cases of real ambiguity
    rather than blocking when a single strongest target is already known
- these inferences must remain read-safe and launch-safe only; posting,
  approval, code mutation, and other elevated actions remain explicitly
  user-triggered

`0.1.x` implementation-facing in-place update contract (`rr-5urd.1`):

- command surface: use `rr update` as the canonical self-update entrypoint for
  the installed binary in this repo
- default interaction: `rr update` must require explicit confirmation before
  mutating the installed binary when running interactively on a TTY
- non-interactive guard: when no TTY is present and `--yes`/`-y` is not
  provided, `rr update` must fail closed with machine-readable blocked output
  rather than guessing consent
- `--yes` / `-y` semantics: bypass only the confirmation prompt; they must not
  bypass release metadata, checksum, provenance, target-resolution, or install
  safety checks
- `--dry-run` semantics: perform metadata/provenance/target validation only and
  never mutate the installed binary
- `--robot` semantics: remain machine-readable and truthful; robot output must
  clearly distinguish `blocked`, `complete`, and `dry-run` without hidden
  interactive prompts
- install provenance boundary: in-place apply is allowed only for published
  release installs with embedded release metadata; local/unpublished binaries
  remain fail-closed and must report reinstall guidance
- apply strategy: download target archive, verify checksum, stage replacement in
  a temporary path, validate binary candidate, then perform explicit
  replace-with-rollback behavior so a failed apply does not leave a half-updated
  install
- rollback expectation: if replacement fails after backup/rename begins, Roger
  must attempt immediate restore of the prior binary and report final state
  truthfully
- migration posture for `0.1.x`: automatic local-state/schema migrations are
  deferred. `rr update` applies binary updates only and must not claim migration
  support. If update or first-run detects a migration-required boundary, Roger
  must fail closed with explicit backup/export + reinstall guidance instead of
  attempting unscoped in-place state mutation
- migration-envelope contract source: the concrete envelope fields, migration
  class boundaries, `rr update --dry-run` reporting obligations, and first-run
  auto-migration limits are defined in
  [`STORE_MIGRATION_COMPATIBILITY_AND_OPERATOR_CONTRACT.md`](STORE_MIGRATION_COMPATIBILITY_AND_OPERATOR_CONTRACT.md)
  for the `rr-1xhg` lane
- explicit out-of-scope for this slice: no silent cross-major upgrades, no
  package-manager handoff masquerading as in-place update, and no implicit data
  migration during apply

Non-goal for `0.1.0`:

- the browser extension does not need to be part of the one-line installer as
  long as Roger's local CLI/TUI product remains easy to install and update

### Minimum acceptable extension scope

At minimum, Roger must launch a targeted local review from a GitHub PR page.
That is the floor, not necessarily the full v1 ceiling.

For `0.1.0`, the accepted bridge choice is Native Messaging for serious
extension interaction. URL-scheme launch fallback is intentionally removed from
the supported `0.1.0` path; browser launch should fail closed when Native
Messaging is unavailable.

### Candidate v1/v2 split

Likely v1 minimum:

- PR-aware launch from the GitHub page into a specific local Roger target
- GitHub-native placement on PR pages: prefer inline buttons or cards attached
  to the PR header or action area rather than a free-floating overlay when the
  page DOM offers a stable seam
- explicit local handoff without a hidden daemon
- first-class support for Edge as well as Chrome/Brave
- a manual browser-action fallback that can open the same bounded launch surface
  when inline attachment is unavailable, delayed, or broken on a given GitHub
  DOM revision

Candidate features that may remain v2:

- PR-aware dropdown with review actions and prompt overrides
- ability to add prompts or review actions directly from the PR page
- GitHub-specific shortcut integration
- external deep-link handoff from collaboration surfaces such as Teams, if that
  can stay thin and daemonless

### Extension feature model by analogy to the TUI

The extension should mirror the TUI selectively rather than imitate it fully:

- **Review Home** becomes a PR-local launcher card with `start`, `resume`,
  `refresh review`, and `open in Roger`, styled to feel native to GitHub and
  attached inline to the page when a stable host seam exists
- **Session Overview** becomes a compact status badge or popover showing bounded
  counts such as `new`, `needs follow-up`, `drafted`, and `awaiting approval`
- **Findings Queue** becomes at most a short teaser list or counts plus a local
  handoff action, not a full in-browser triage grid
- **Finding Inspector** becomes a targeted deep link or focus action that opens
  the matching finding locally, with optional follow-on `open evidence in
  editor` from the local Roger surface rather than in-browser review ownership
- **Draft Approval Queue** becomes a pending-approval indicator and an `open
  drafts locally` action; approval and posting remain local-first
- **Prompt Ingress** becomes a preset selector plus a short explicit objective,
  not a second full prompt-authoring surface
- **Attention Events** become lightweight badges or banners only if the bridge
  can read status safely without introducing hidden infrastructure

### Operator-visible verbs must represent user intent

The extension must not expose transport plumbing as ordinary product actions.

Rules:

- ordinary PR-page actions should represent review intent such as `start`,
  `resume`, `refresh review`, `open findings locally`, or `open draft queue`
- `refresh` in product UI must mean refreshing the review against changed PR or
  repo state, never refreshing Native Messaging transport or bridge readback
- if the extension can attempt bridge readback or status refresh automatically,
  it should do so automatically rather than exposing a maintenance button
- manual transport controls such as `refresh bridge`, `ping host`, `reload
  status`, or similar mechanics belong only in setup, doctor, or explicit
  recovery surfaces
- when bridge readback is unavailable, the extension should degrade to truthful
  launch and open-local affordances rather than surfacing a cluster of plumbing
  commands

### Capability tiers driven by bridge strength

Plan extension features in two explicit capability tiers:

**Launch tier** (Native Messaging launch-only handoff):

- start, resume, or refresh a review from a PR page
- choose a bounded launch mode or preset
- pass a short objective and preferred local UI target
- expose a manual browser-action fallback entry when PR-page inline injection is
  unavailable or GitHub DOM drift prevents a clean attachment
- no promise of live status beyond successful handoff

**Companion tier** (works with Native Messaging or equivalent daemonless
readback):

- bounded active-session lookup and richer bridge-health feedback
- counts for pending findings, follow-ups, drafts, and approval-required state
- targeted local focus actions such as `open finding` or `open draft queue`
- clearer multi-instance disambiguation when more than one local Roger target is
  available

Any feature that requires state readback or local-target focusing should be
planned in the companion tier, not assumed to fit into the launch tier.

Decision for `0.1.0`: Roger should implement the companion tier in v1 via
Native Messaging only. URL-scheme launch fallback is out of the supported
product path and should fail closed with setup guidance.

### Contract and packaging discipline

- bridge messages should use Roger-owned versioned request/response envelopes
- Rust companion structs should be the source of truth and TS bridge types
  should be generated from them as part of Roger-owned tooling
- the chosen `0.1.0` extension toolchain should stay small: `typescript`,
  official `chrome-types`, and Roger-owned pack/install scripts
- avoid a framework-led bundler stack; if bundling later proves necessary, keep
  it to one narrow tool behind Roger-owned scripts
- the extension source base should generate installable artifacts for Chrome,
  Brave, and Edge
- release/devops work for multi-arch companion binaries and browser packages is
  part of the product surface, even if it is sequenced into a later delivery
  workstream

### Extension non-goals

The extension should not become:

- the canonical owner of review state
- a full findings-triage workspace
- the place where outbound approval is granted
- a hidden polling client against a local daemon
- a second general-purpose prompt-engine UI

### Bridge strategy

The extension-to-local bridge must stay daemonless in steady state.

Current `0.1.0` product contract: Native Messaging only.

Chrome's Native Messaging API launches a registered native executable on demand
via stdin/stdout JSON messages. It is bidirectional, daemonless (Chrome spawns
and owns the process lifetime), and supports the status-sync and action-routing
needed for deep integration.

Requires:
- a native messaging host manifest registered in the OS (one JSON file)
- a host executable (could be the `rr` CLI in a `--messaging` mode)
- no always-on background service

Chosen v1 direction:

- implement Native Messaging as the primary serious bridge
- start with the Rust `rr` binary as the first host executable unless packaging
  constraints later justify a tiny helper binary
- treat actual host-runtime execution as part of the product contract: Roger
  does not earn browser-launch support merely by writing a host manifest or
  passing `rr extension doctor`; the registered `rr` binary must answer a real
  Native Messaging launch intent over stdin/stdout without hanging
- remove browser URL-scheme launch from the supported product path; if Native
  Messaging is unavailable, return bounded setup guidance and fail closed
- keep URL-scheme launch discussion in historical critique documents only, not
  active setup/current-truth sections

WebSocket / local HTTP is explicitly rejected for the bridge: it requires a
daemon and introduces a background service as the architectural center.

Chosen direction:

- Native Messaging is the primary v1 bridge because Roger wants bounded
  readback, multi-instance disambiguation, and targeted local actions from the
  PR page.
- URL-scheme launch fallback is not a supported `0.1.0` bridge mode
- do not count clipboard/manual command copying as a core-functional fallback

### Trigger and notification model

Roger should own a small, explicit attention-event model rather than scattering
notification logic across the extension, CLI, TUI, or harness layer.

Principles:

- Roger owns canonical review state and canonical attention states
- harnesses such as OpenCode may emit useful progress or wait signals, but
  Roger should normalize them into Roger-owned events
- local surfaces should be push-capable where practical, but Roger should not
  depend on a polling loop against GitHub, Teams, or other external systems
- external collaboration systems should be treated as optional launch or mirror
  surfaces, not as the architecture center

Minimum event set:

- `review_started`
- `review_attached`
- `awaiting_user_input`
- `awaiting_outbound_approval`
- `findings_ready`
- `refresh_recommended`
- `review_failed`

Delivery surfaces:

- TUI status views and attention queues
- CLI status and explicit resume commands
- optional local desktop notifications
- extension-side status or affordances only if the chosen bridge supports them
- future external deep links or collaboration hooks built on the same event
  model

The simplest likely business-trigger path is still thin orchestration: a user
clicks through from an external message to the PR, then launches Roger in one
click. Roger should support that cleanly without trying to become a full
cross-tool workflow engine.

## CLI Requirements

The CLI is the glue layer between local repo context, the TUI, and automation.

Current live `0.1.0` command surface:

- `rr review`
- `rr resume`
- `rr return`
- `rr sessions`
- `rr findings`
- `rr search`
- `rr refresh`
- `rr status`
- `rr update`
- `rr extension setup`
- `rr extension doctor`
- `rr bridge ...`
- `rr robot-docs`

Current-scope command-surface additions still required to complete the product
truthfully:

- an explicit dense-workspace entry contract, whether through `rr review` /
  `rr resume` handoff or a dedicated command such as `rr tui` / `rr open`
- a first-class command family for draft, approval, and posting transitions
- a truthful bootstrap/preflight surface centered on `rr doctor`, plus `rr init`
  if that is the chosen canonical bootstrap command
- a product-facing `rr extension uninstall` path if Roger intends uninstall as
  part of the normal browser lane rather than leaving it as bridge-only repair

Deferred command candidate (not in current shipped `0.1.0` CLI surface):

- `rr memory rebuild` (defer until the explicit search/index maintenance slice
  is implemented and validated)

CLI behavior should be session-aware. If invoked from a repo directory, it
should infer the likely review target when possible rather than forcing
redundant flags.

Expected CLI behavior:

- `rr resume` in a repo should reopen the most relevant session when there is a
  single strong match
- `rr return` should jump back into the Roger session associated with the
  current dropped-out harness context when that context is available
- if there are multiple plausible matches, Roger should open a session finder or
  print a ranked selection list rather than silently picking one
- `rr sessions` should provide a global session finder for jumping across repos,
  PRs, and attention states
- CLI help, status output, and docs must describe only the provider surfaces
  and command paths that actually exist in the product; drift here is a product
  bug, not a documentation nicety

### Robot-facing CLI conventions

Roger should reserve a bounded machine-facing CLI surface so future coding
agents can drive `rr` without scraping human-oriented prose.

Suggested conventions:

- reserve the `--robot*` prefix for machine-facing flags and behaviors
- `--robot` should enable stable machine mode: no ANSI on stdout, no spinner
  chatter, no conversational framing, and deterministic exit codes
- diagnostics and progress meant for humans should go to stderr in robot mode
- `--robot-format json|compact|toon` should be the primary structured output
  selector, with `json` as the safe default
- `toon` output should only be allowed for commands whose payload shape has
  passed Roger's own smoke tests; unsupported commands should return an explicit
  structured fallback or error rather than silently drifting formats
- a small discovery surface such as `rr robot-docs guide`, `rr robot-docs
  commands`, `rr robot-docs workflows`, and `rr robot-docs schemas` should
  provide concise machine-readable usage help
- high-value commands such as `rr status`, `rr sessions`, `rr findings`, `rr
  search`, and selected dry-run launch paths should support robot mode early

Rules:

- robot mode should preserve the same semantics as the human-facing command, not
  create a separate hidden workflow
- Roger's durable findings schema remains the source of truth; robot output is a
  transport surface for automation, not a replacement for canonical storage
- when raw output, partial findings, or repair-needed states occur, robot mode
  should expose them explicitly rather than flattening them into success text

### Optional harness command surface

Where a harness supports commands, Roger should expose a small command surface
that mirrors core `rr` operations rather than inventing a separate workflow.

Recommended command IDs:

- `roger-help`
- `roger-status`
- `roger-findings`
- `roger-refresh`
- `roger-clarify`
- `roger-open-drafts`
- `roger-return`

These are logical command IDs, not fixed syntax. One harness may expose
`/roger-status`, another may expose `:roger status`, and another may offer no
command surface at all. The core requirement is parity of semantics, not
uniform literal spelling.

Scope rule:

- this surface should stay bounded to Roger navigation, status, refresh,
  clarification, and return affordances; approval and GitHub-posting actions
  remain explicitly elevated in the TUI or CLI approval flow rather than hidden
  behind lightweight in-harness commands

## Review Intake and Prompt Ingress

All launch surfaces should normalize into one canonical `ReviewIntake v1`
object before any session lookup, session creation, or prompt execution occurs.
The domain consumes only that normalized object plus a Roger-resolved launch
context; it must not branch on whether the request originally came from the
CLI, TUI, extension, or an external deep link.

### `ReviewIntake v1`

Required top-level fields:

- `schema_id`
  - fixed value `review-intake.v1`
- `source`
  - `surface`: `cli`, `tui`, `extension`, `external-link`, later others
  - `surface_invocation_id`: source-local correlation ID when available
  - `received_at`
- `action`
  - `start`, `resume`, `refresh`, or `follow_up`
- `target`
  - `repo_locator`: Roger-owned repo identity or canonical local repo path
  - optional `review_target`:
    - `pr` with provider and PR identifier
    - `branch` with branch/ref identity
    - `session` with `ReviewSession` identity
    - `finding` with `Finding` identity for local follow-up entrypoints
- `prompt_ingress`
  - optional `preset_id`
  - optional `explicit_objective`
  - `origin`: `defaulted`, `user_selected`, or `user_supplied`
- `ui_target`
  - requested local destination such as `tui` or `cli`
- `launch_preferences`
  - optional `launch_profile_id`
  - optional `instance_preference`
  - optional `worktree_preference`
- `config_selector`
  - optional repo/project/profile selector or overlay ID

Required invariants:

- `schema_id`, `source.surface`, and `action` are always required
- `target.repo_locator` is required for every action except a pure
  `resume` against a globally unique `session` target that Roger can resolve
  without ambiguity
- `resume` requires a resumable `session` target or enough repo/PR identity to
  locate exactly one eligible session
- `refresh` requires an existing `session` target or enough repo/PR identity to
  locate exactly one refreshable session
- `follow_up` requires a local `session` target and may optionally include a
  `finding` target when the handoff should open a specific finding or local
  clarification lane
- `start` may target a repo alone or a repo plus PR/branch context, but it must
  not silently reuse an existing session unless the action is explicitly
  normalized to `resume`

### Source-surface normalization

All surfaces map into the same contract, but not every surface may populate the
same fields.

- `cli`
  - may populate any `ReviewIntake v1` field Roger supports in `0.1.0`
  - may reference richer local prompt authoring inputs, but those must still
    normalize into the bounded `prompt_ingress` object before launch
- `tui`
  - may emit `resume`, `refresh`, and `follow_up` requests with strong local
    session or finding identity
  - may request focused handoff into local queues or inspectors
- `extension`
  - must emit only the bounded browser-safe subset defined below
  - never becomes a second general-purpose prompt-authoring surface
- `external-link`
  - follows the same bounded rules as the extension unless Roger later defines a
    stronger authenticated local surface

### Bounded prompt ingress

Roger supports prompt input from browser and deep-link surfaces, but only within
an intentionally small envelope.

Allowed `0.1.0` web-path prompt ingress:

- `preset_id`
- short `explicit_objective`
- no other prompt text fields

Rules:

- `explicit_objective` is short free text intended to sharpen the review goal,
  not replace the prompt system; it should be bounded to a small size budget
  such as a few hundred characters, not multi-paragraph instructions
- web-path ingress must not carry raw prompt templates, giant URL-encoded
  prompt blobs, attached prompt packs, policy text, or hidden execution flags
- deeper prompt authoring remains a local-first CLI/TUI capability
- if both `preset_id` and `explicit_objective` are absent, Roger may resolve a
  default preset from config, but it must not invent a user objective
- if `preset_id` is unknown in the selected config scope, the intake is
  rejected rather than falling back to a similarly named preset

### Allowed per-review overrides

`ReviewIntake v1` may override only bounded launch-shaping inputs:

- prompt preset selection
- short explicit objective
- config/profile selector
- UI target
- launch profile preference
- instance preference
- worktree preference

Disallowed through ordinary intake:

- relaxing approval gates
- enabling mutation-capable behavior
- weakening trust-floor policy
- changing posting authority
- changing provider capability tier or safety posture

Those concerns belong to explicit config or elevated local flows, not ambient
per-launch overrides. A request that attempts to smuggle them through the
intake contract must fail closed.

### Persistence and audit

Roger should persist both the received intake and the resolved launch decision
so resume, audit, and failure analysis can explain why a review started the way
it did.

Minimum persisted capture:

- canonical `ReviewIntake v1`
- source receipt metadata and any bounded raw payload snapshot needed for audit
- normalization result, including dropped fields and validation outcome
- resolved repo, PR, branch, session, and finding identities when available
- resolved config/profile, UI target, and launch profile
- resolved prompt preset and any accepted `explicit_objective`
- rejection reason or ambiguity reason when launch does not proceed

Roger should persist the normalized request even when launch is rejected so the
system can explain the failure without reconstructing it from logs.

### Fail-closed behavior

Missing, conflicting, or unsafe intake fields must not cause Roger to invent
launch state.

Required `0.1.0` fail-closed rules:

- missing required fields reject the intake with an explicit validation reason
- conflicting target identities, such as mismatched repo and session ownership,
  reject the intake
- ambiguous session lookup never auto-picks a session; Roger must require an
  explicit local disambiguation step
- unsupported source-surface fields are ignored only when they are clearly
  additive metadata; attempts to influence policy or execution outside the
  contract reject the intake
- unavailable launch profiles, worktree preferences, or UI targets must degrade
  truthfully with an explicit resolved-launch record, not silent substitution
- browser-originated requests never bypass Roger's ordinary local approval and
  audit paths even when launch succeeds

This gives Roger a real shared intake contract for CLI, TUI, extension, and
external-link launches while keeping web-path prompt ingress bounded and
auditable.

### Local launch profile and terminal topology

Roger should treat terminal and muxer selection as an explicit launch concern,
not a hidden implementation detail.

Required `0.1.0` concept:

- `LocalLaunchProfile`
  - `id`
  - `ui_target` such as `tui` or `cli`
  - `terminal_environment` such as `vscode_integrated_terminal`,
    `wezterm_window`, `wezterm_split`, or another supported local surface
  - `multiplexer_mode` such as `none`, `ntm`, `wezterm_split`, or another
    Roger-supported strategy
  - `reuse_policy` such as `reuse_if_possible` or `always_new`
  - optional repo/project overrides

Rules:

- local launches should resolve through a named launch profile rather than ad hoc
  terminal-chooser logic
- the extension and companion should be able to request a preferred launch
  profile without owning the platform-specific details
- if the requested terminal or muxer environment is unavailable, Roger should
  fall back truthfully and explain what launch surface was actually used
- multi-instance behavior should remain compatible with launch profiles so a
  user can prefer, for example, VS Code integrated terminals with NTM in one
  repo and bare WezTerm windows or splits in another

### Prompt preset model for `0.1.0`

Roger does not need a heavy prompt-pack versioning system in `0.1.0`.

Required `0.1.0` capabilities:

- stable prompt preset IDs
- recent prompts
- frequent prompts
- last-used prompt per repo
- optional favorites
- immutable execution snapshots of the exact resolved prompt text

Minimum model:

- `PromptPreset`
  - `id`
  - `name`
  - `scope`
  - `template_text`
  - `tags`
- `PromptInvocation`
  - `preset_id`
  - `resolved_text`
  - `user_override`
  - `repo_id`
  - `session_id`
  - `used_at`

Rules:

- active runs should snapshot the exact resolved prompt they used
- reuse should prefer preset IDs plus execution snapshots rather than a full
  release-management/versioning model
- if Roger later grows richer shared prompt packs, explicit prompt versioning
  can be added without invalidating the `0.1.0` storage model

### Session baseline and run modifiers in local surfaces

Roger should distinguish between sticky per-session prompt defaults and
one-off run modifiers.

Required `0.1.0` model:

- `session baseline`
  - Roger-owned defaults for the active `ReviewSession`
  - may include the resolved prompt preset, selected provider/model within the
    already-allowed capability tier, and bounded prior-run carry-forward
- `run modifiers`
  - one-off prompt-shaping inputs for the next `PromptInvocation`
  - may include the chosen preset, short `explicit_objective`, selected finding
    references, and scoped modifiers such as changed-files-only

Rules:

- session baseline must be visible and inspectable from local Roger surfaces
- changing the session baseline is an explicit forward-only action that affects
  future runs, not past prompt history
- a baseline change should create a visible run-mode boundary in session
  history, not silently rewrite what earlier runs meant
- baseline changes must not casually change review-target identity, approval
  policy, posting authority, or safety posture; those require a new session or
  a more explicit elevated local flow
- every `PromptInvocation` still snapshots the exact resolved prompt text it
  actually used, regardless of current preset definitions or later baseline
  changes

### Outcome capture for future analytics

Roger should collect enough structured data in `0.1.0` that later analytics can
reason about which prompts, findings, and review paths were actually useful.

Minimum analytics-ready capture:

- which prompt preset and resolved prompt text was used for each run
- which findings were accepted, ignored, resolved, or left stale
- which findings produced outbound drafts
- which drafts were approved and posted
- which posted actions map to GitHub review identifiers
- review completion state, PR state, and merge outcome when available
- links from findings and drafts to the commits, files, or PR states they were
  grounded in
- optional explicit human usefulness labels when the user wants to provide them

This is enough to support later heuristics such as:

- prompt preset usefulness by repo or project
- finding categories that frequently survive to approved/posted comments
- patterns that correlate with merged fixes versus ignored noise
- anti-pattern detection when a prompt or finding style repeatedly produces
  low-value or reverted outcomes

Roger does not need a user-facing analytics dashboard in `0.1.0`, but it
should avoid throwing away the evidence needed to build one later.

## Testing Principles

Roger should have extensive integration coverage, but only where it pays for
itself.

Principles:

- keep unit tests fast and broad around the domain model and prompt pipeline
- require at least one happy-path end-to-end integration test that exercises
  the real multi-integration boundary
- make integration tests target the harness contract rather than OpenCode-only
  internals so alternative providers can reuse the same suite
- avoid redundant slow tests; each one must defend a meaningful workflow,
  failure mode, or compatibility promise
- maintain a separate review-flow matrix in
  `docs/REVIEW_FLOW_MATRIX.md` as the scenario inventory for cross-surface
  consistency checks and integration-test selection
- keep the explicit provider/browser/OS/fixture support matrix in
  [`RELEASE_AND_TEST_MATRIX.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/RELEASE_AND_TEST_MATRIX.md)
  so coverage obligations do not live only as prose
- keep the implementation-facing harness contract in
  [`TEST_HARNESS_GUIDELINES.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/TEST_HARNESS_GUIDELINES.md)
  and the automated E2E budget in
  [`AUTOMATED_E2E_BUDGET.json`](/Users/cdilga/Documents/dev/roger-reviewer/docs/AUTOMATED_E2E_BUDGET.json)
  so tiers, fixtures, and E2E growth rules stay machine-checkable

## Worktree and Named Instance Model

Multi-instance support is essential. Roger must be able to run at least two
reviewers side by side without hidden assumptions about ports, env files, or
mutable local state.

Requirements:

- worktree support must be configurable rather than forced globally
- single-repo mode should work out of the box against the current checkout with
  little or no configuration
- review flows default to the current checkout plus a recorded repo snapshot;
  dedicated worktrees are created only when isolated execution, code changes, or
  conflicting local repo state require them
- users must be able to specify which supporting files get copied into a
  worktree (`.env`, `.env.local`, repo-local config, and similar files)
- multiple local copies of the app can coexist as named instances
- Roger state should use one canonical per-profile local store by default;
  named instances isolate repo-local mutable resources unless the user
  explicitly creates a separate Roger profile
- mutable resources that often collide in local dev must be made explicit:
  default ports, dev DBs, docker/container naming, cache dirs, artifact dirs,
  and log dirs
- named instances need clear preflight diagnostics for what is shared versus
  isolated
- Roger should offer configurable isolation primitives for these resource
  classes rather than trying to hardcode every development topology
- environment-specific writes should remain disabled by default

`0.1.0` mode-selection rules:

- `current_checkout` is the default review mode. Roger should use the existing
  checkout plus a recorded repo snapshot unless the user explicitly selects a
  different mode or preflight proves the default would be unsafe.
- `named_instance` is the default isolation mode when the user needs separate
  repo-local mutable resources but does not need a separate checkout. Roger may
  recommend this mode when ports, repo-local dev DBs, container names, caches,
  artifacts, or logs would otherwise collide, but it must not silently escalate
  into it.
- `worktree` is the heavier isolation mode. Roger should require explicit user
  choice or explicit confirmation before creating one, and should recommend it
  only when checkout-level isolation is actually needed for code changes,
  generated files, or conflicting repo state.
- Roger must not silently escalate from `current_checkout` to `named_instance`
  or `worktree`. Preflight may recommend a different mode, but the operator
  must see and approve the change in plain terms.
- Roger may silently reuse an already-bound named instance or worktree only
  when the user explicitly targeted that binding or resumed a session already
  attached to it.
- a separate Roger profile is required when the canonical Roger store itself
  must be isolated, not merely the repo-local mutable resources. In `0.1.0`,
  that means different local human identities, different GitHub identities,
  confidentiality boundaries that must not share memory/search/audit history,
  or incompatible policy/config overlays that would make shared-profile state
  misleading or unsafe.

`0.1.0` resource-class defaults:

| Resource class | `current_checkout` default | `named_instance` default | `worktree` default |
|----------------|----------------------------|--------------------------|--------------------|
| env/config files | read in place; no copy | no implicit copy | no implicit copy |
| ports | shared unless the launch profile declares a rewrite rule | deterministic per-instance rewrite for declared resources; otherwise block on collision | deterministic per-instance rewrite for declared resources; otherwise block on collision |
| repo-local DBs | shared only for read-mostly flows; mutation-capable use needs explicit override | rewrite to an instance-specific path for declared resources; never copy live DB files by default | rewrite to an instance-specific path for declared resources; never copy live DB files by default |
| container names | unchanged | append a deterministic instance suffix | append a deterministic instance/worktree suffix |
| caches | unchanged unless declared as Roger-managed | per-instance cache root by default | per-instance cache root by default |
| artifact dirs | Roger-managed artifacts stay in the canonical profile store; repo-local artifact outputs stay unchanged unless declared | per-instance repo-local artifact dir by default for declared resources | per-instance repo-local artifact dir by default for declared resources |
| log dirs | unchanged unless declared as Roger-managed | per-instance log dir by default | per-instance log dir by default |

Rules:

- Roger must never implicitly copy secret-bearing files such as `.env`,
  `.env.local`, `.env.*.local`, direnv files, local credential files, or other
  operator-marked secret inputs into a named instance or worktree.
- non-secret checked-in templates such as `.env.example` may be read or copied
  only when the resolved config explicitly allows that resource class.
- Roger should prefer path rewrites, deterministic naming, and explicit
  environment projection over copying mutable runtime state.
- Roger's own canonical DB/search store stays per-profile by default; named
  instances and worktrees isolate repo-local mutable resources before they
  isolate Roger's profile-level memory or audit state.

`0.1.0` preflight result classes:

| Result | Meaning | Minimum operator guidance |
|--------|---------|---------------------------|
| `ready` | selected mode and resource plan are safe as-is | proceed without extra steps |
| `ready_with_actions` | the topology is supportable, but Roger needs explicit user choices first | show the exact actions required, such as choosing `named_instance`, approving worktree creation, or allowlisting a non-secret file copy |
| `profile_required` | instance/worktree isolation is insufficient because the Roger profile store must also be isolated | tell the operator to create or select a separate Roger profile and explain why |
| `unsafe_default_blocked` | the default action would silently share or copy unsafe mutable state | block execution until the user changes mode, removes the unsafe default, or supplies an explicit override |
| `verification_failed` | Roger could not verify the chosen topology or resource rewrite plan | fail closed and report the specific verification gap |

Preflight rules:

- preflight must classify the launch before Roger creates a worktree, rewrites
  resources, or starts a mutation-capable local surface
- recommendation text must say which resources are shared, which are isolated,
  and which remain blocked
- the resolved preflight report should be inspectable later from Roger's local
  session state and CLI output

`0.1.0` hook phases and config layering:

- hook phases are `preflight`, `worktree_create`, `materialize_resources`,
  `session_env`, `verify`, and `cleanup`
- `preflight` is read-only and computes the selected mode, resource plan,
  preflight classification, and any explicit operator actions still required
- `worktree_create` runs only for approved `worktree` launches and is limited to
  creating or reusing the checkout plus any explicitly allowlisted non-secret
  file materialization
- `materialize_resources` creates instance-specific paths, deterministic names,
  and rewrite targets for declared ports, DBs, caches, artifacts, and logs; it
  must not perform implicit secret-file copy
- `session_env` produces the final environment projection handed to the review
  session or launched process; it may reference rewritten paths but must not
  mutate the source checkout's env files in place
- `verify` confirms that the resolved paths, names, and bindings exist and do
  not collide; any failure here yields `verification_failed`
- `cleanup` may remove only Roger-created ephemeral resources for the selected
  instance/worktree; it must not delete the canonical profile store, the source
  checkout, or user-provided files without explicit opt-in

Resolved config order for `0.1.0`:

1. built-in defaults
2. user-global templates
3. optional project/workspace profiles
4. repo-specific templates
5. selected launch profile
6. mode defaults for `current_checkout`, `named_instance`, or `worktree`
7. named-instance or worktree overrides
8. per-review overrides

Rules:

- the resolved config must be inspectable, with provenance for each effective
  value and resource decision
- later layers may override declared fields explicitly, but Roger should not
  rely on ambient shell state or hidden file copies as a second config channel
- per-instance naming, resource rewrites, and worktree-copy allowlists should
  all be visible in the same resolved config output

Avoid DB-copy synchronization as the default model.

## Configuration Model

Configuration should be layered and additive by default.

Layers:

- built-in defaults
- user-global templates
- optional project/workspace profiles spanning multiple repos
- repo-specific templates
- named-instance or worktree overrides
- per-review overrides

Rules:

- later layers may add or override explicitly
- hidden replacement behavior is not acceptable
- effective config should be inspectable
- prompt templates and skills should be inherited in a controlled way

Topology support needs to be explicit without overfitting to one company shape.

Target topology classes:

- single repo
- monorepo
- multi-repo project or service family
- multi-team, multi-repo environments as a later but plausible case

Configuration principles:

- single-repo and monorepo setups should work with sensible defaults and little
  or no configuration
- multi-repo/project defaults should be opt-in through explicit workspace or
  project profiles, not ambient scanning
- `project` membership should be an explicit Roger-managed allowlist of repos,
  not something inferred from naming, remotes, directory layout, or weak
  similarity
- team or org-level profiles should be possible later, but only as explicitly
  bound overlays that preserve clear provenance and avoid silently bleeding
  unrelated settings into a repo
- effective config should show where each value came from
- prompt presets, review objectives, launch defaults, search scope defaults,
  binding policies, trust floors, instance isolation settings, and
  worktree-copy rules should all fit into the same inspectable model
- launch profiles should also fit this model, including preferred terminal
  environment, muxer strategy, reuse policy, and per-repo or per-project
  overrides

This is intentionally a constrained model: broad enough to support monorepos
and related-repo groups, but not so abstract that Roger becomes a generic
enterprise policy engine before v1 exists.

### Canonical source defaults

Roger should be conservative about which checked-in docs become high-trust
policy automatically.

Default auto-canonical classes:

- repo `AGENTS.md`
- repo-local Roger policy/config docs
- explicitly bound ADR directories or policy directories

Not auto-canonical by default:

- generic `README.md`
- generic `CONTRIBUTING.md`
- issue templates
- PR templates
- broad architecture notes or exploratory docs

Those non-canonical docs may still be searchable evidence, cited context, or
promotion candidates later, but they should not silently become high-trust
policy just because they are present in the repo.

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

Rules:

- `gh` is an implementation detail of Roger's GitHub adapter, not an approved
  direct review-communication surface for agents
- agent-authored review communication should always become Roger-managed
  `OutboundDraft` or batch objects first
- direct raw-`gh` review writes bypass Roger's audit, invalidation, and
  approval protections and should therefore be treated as architecture
  violations

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

Testing needs to match both Roger's product shape and Roger's economics.

The default rule is:

- push confidence down the stack first
- keep exactly three validation lanes: `unit`, `integration`, and `e2e`
- keep parameterized and property-style testing inside the `unit` lane rather
  than treating it as a separate lane
- prefer unit coverage over heavier integration coverage wherever a lower layer
  can defend the same invariant
- keep provider acceptance, crash-recovery, bridge/install truthfulness, and
  bounded real-surface checks inside `integration` unless the defended promise
  is a true multi-surface product journey
- keep automated end-to-end coverage intentionally scarce
- require explicit justification before adding another slow multi-boundary test

### Test-pyramid posture

Roger should derive most confidence from:

1. unit tests
2. integration tests
3. a very small number of earned automated end-to-end tests
4. operator smoke and release evidence as explicit gate inputs, not as a
   separate validation lane

This is not because end-to-end tests are unimportant. It is because they are
expensive to set up, slower to run, harder to debug, and easy to overuse as a
substitute for stronger lower-level contracts.

### Unit-test obligations

Unit tests should be the largest lane in the suite. This lane includes
parameterized and property-style tests for Roger's high-dimensional rule
systems.

They should cover, at minimum:

- domain state machines for `ReviewSession`, `ReviewRun`, `Finding`,
  `FindingState`, `OutboundDraft`, `OutboundDraftBatch`, and approval invalidation
- deterministic fingerprint generation, finding reconciliation, and stale or
  carried-forward classification
- config layering, launch-profile resolution, and instance-isolation policy
- `ResumeBundle` construction, trimming, and continuity-state projection
- `StructuredFindingsPack` parsing, normalization, repair classification, and
  partial-salvage logic
- bridge-envelope serialization and deserialization
- `RogerCommand` routing and stable `RogerCommandResult` shaping
- GitHub comment, question, and suggestion-block rendering
- markdown or GFM-safe outbound payload generation
- named-instance resource rewriting for env files, ports, container names,
  artifact dirs, and logs
- search or memory ranking helpers, scope filters, and degraded-mode fallbacks
- TUI-presenter or view-model state reducers without needing a live terminal
- parameterized or property-style coverage for the following rule matrices:

- config-layer override matrices across global, project, repo, instance, and
  per-review layers
- finding-state transitions across triage, outbound, approval, and invalidation
  edges
- repair-loop outcomes across valid, partial, raw-only, repair-needed, and
  failed stage results
- provider-capability matrices across OpenCode primary, bounded live-CLI
  providers, and future unsupported providers
- scope and memory retrieval matrices across repo, project, org, and abstention
  behavior
- anchor normalization and invalidation across file movement, rebases, and
  refreshes
- launch-profile fallback behavior across terminal, muxer, and unavailable-target
  permutations
- worktree or named-instance resource-isolation matrices
- GitHub suggestion rendering across single-line, multi-line, and non-suggestible
  comment cases
- robot-output format matrices across `json`, `compact`, and optional `toon`
  support

Where feasible, Roger should favor concise parameter tables and generated case
matrices over hand-written one-off tests.

### Integration-test obligations

Integration tests should defend boundary behavior, not replace unit tests.
`Integration` is the home for provider acceptance, transaction and crash
recovery, bridge/install truthfulness, memory/search contract coverage, and
other bounded real-surface checks that do not need a full product journey.

Required integration families:

- storage plus migration tests, including canonical DB rows, artifact metadata,
  and content-addressed artifact lookup
- prompt pipeline plus canned provider-output corpora, including malformed,
  partial, raw-only, and repair-success cases
- harness-adapter tests with doubles plus bounded acceptance paths for supported
  providers
- CLI tests for repo-context inference, session binding, robot outputs, and
  durable resume behavior
- TUI controller tests with fake runtime services and structural state snapshots
  rather than brittle full-terminal pixel or text goldens
- bridge contract tests for Native Messaging envelopes, host-mode behavior, and
  truthful failure paths
- extension injection and action-wiring tests, keeping browser behavior narrow
  and PR-local
- GitHub-adapter tests with Roger-owned doubles for mutation behavior,
  invalidation, and audit persistence
- multi-instance and worktree tests for the concurrent reviewer case
- search/index tests using seeded fixtures and rebuildable sidecars rather than
  live model downloads
- provider-acceptance suites that prove truthful launch, resume, reseed,
  bounded dropout, and other published provider claims without promoting each
  provider path into its own heavyweight E2E
- transaction and crash-recovery tests for launch binding, artifact writes,
  return/rebind, retries after partial failure, and stale-event rejection
- search and memory contract tests proving repo-first lookup, explicit broader
  overlays, provenance buckets, candidate-versus-promoted behavior, and
  truthful degraded lexical-only fallback
- browser, bridge, install, and setup truthfulness checks unless the defended
  promise truly requires a full end-to-end journey

### Robustness, failure handling, and real-world usage obligations

Roger's correctness contract includes nominal, degraded, interrupted, and
failed states. Validation should name those states explicitly.

At minimum, current-scope validation should defend:

- launch verification and binding failures before a session is reported as
  started
- resume and return flows that degrade to reseed or fail closed truthfully
- refresh invalidation and reconfirmation for drafts and approvals
- stale or invalid evidence anchors without destruction of surviving finding
  context
- partial, raw-only, and repair-needed findings outcomes
- browser bridge unavailability, stale readback, and setup drift without
  nervous transport-plumbing UI
- posting rejection or failure recovery with preserved audit and retry or
  review path
- install, update, checksum, provenance, and manifest failures that must remain
  fail closed
- TUI selection persistence and session-switch correctness across refreshes,
  long queues, and ordinary operator interruptions

Real-world usage proof should also include at least bounded evidence for:

- browser-first review start into local Roger, then local continuation
- shell-first review, deliberate dropout to the harness, and durable return
- refresh after target change with correct invalidation behavior
- same-PR multi-session ambiguity and truthful session picking
- large-enough fixtures or longer-lived sessions to reveal queue, recency, and
  recovery problems that do not appear in toy demos

This is not a license to add more automated end-to-end tests by reflex. Most of
these obligations should be defended by unit, integration, operator-stability,
or manual-smoke evidence unless the promise is truly a multi-surface journey.

### End-to-end testing policy

Roger should keep one blessed automated happy-path end-to-end test in `0.1.x`
and admit additional E2Es only when they clearly earn their keep.

Recommended minimum automated E2E:

- launch from CLI
- create or resume a real provider-backed review session on the blessed path
- capture a valid structured findings pack
- normalize findings and materialize local drafts
- review or approve through Roger's local flow
- post through a GitHub adapter double
- persist the final audit chain

Rules:

- browser launch, Native Messaging, dropout or return, malformed findings,
  multi-instance routing, and provider-bounded behavior should usually be
  defended by integration-family suites or smoke evidence rather than promoted
  immediately into additional full E2Es
- every new automated E2E must justify why the failure mode cannot be defended
  more cheaply with unit or integration coverage
- adding a new E2E is appropriate only when it protects a product-defining
  promise across several real boundaries and a lower-layer breakdown would leave
  a meaningful gap
- if a defended journey depends on prior-review lookup or promoted memory, the
  E2E must assert the live memory contract rather than merely checking that
  search returned something. At minimum that means truthful retrieval mode,
  correct scope bucket, preserved provenance, and explicit degraded lexical-only
  behavior when semantic retrieval is unavailable
- the prescriptive E2E catalog lives in
  [`RELEASE_AND_TEST_MATRIX.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/RELEASE_AND_TEST_MATRIX.md);
  only entries promoted into
  [`AUTOMATED_E2E_BUDGET.json`](/Users/cdilga/Documents/dev/roger-reviewer/docs/AUTOMATED_E2E_BUDGET.json)
  consume blessed budget slots

### E2E admission contract

Every proposed new automated E2E should name, before it is accepted:

- the exact user-visible or operator-visible promise it defends
- the specific real boundaries it exercises that lower layers do not already
  cover
- why unit plus integration plus operator smoke are insufficient for that gap
- the fixture/runtime cost it adds to the suite and release process
- the support wording or release claim that depends on the test existing

Default rule:

- if the proposed E2E mostly reasserts a success path that lower layers can
  already defend, reject it
- if the defended promise is primarily about failure handling, degraded mode,
  invalidation, or recovery, prefer unit or integration proof first
- a happy-path E2E never substitutes for narrower failure-state coverage

### E2E budget feedback rule

Roger should include an explicit feedback mechanism for automated E2E growth.

Required behavior:

- Roger should track the current blessed automated E2E count in a simple
  machine-readable manifest, snapshot, or other Roger-owned budget file
- when an agent runs the test suite or the relevant validation command and the
  automated E2E count has increased relative to the recorded baseline, Roger
  should emit a visible feedback message
- that message should ask whether the new coverage could be defended more
  cheaply with a unit or integration test instead of
  taking the lazy path to another heavyweight E2E
- the feedback should request an explicit justification or annotation for the
  new E2E before the change is treated as acceptable

Suggested feedback tone:

- direct and a little sharp is acceptable
- the message should challenge the test author to justify the cost rather than
  silently accepting E2E growth
- it should not be insulting, but it should make laziness an explicit concern

Recommended rollout:

- start as a warning in local and CI test runs
- once the workflow is proven, escalate to a CI failure unless the added E2E is
  accompanied by an explicit justification record

### Agent-tooling validation obligations

Roger is building agent-facing surfaces, not only human UI.

That means the suite must also validate:

- stable `rr --robot` output shapes and deterministic exit behavior
- `StructuredFindingsPack`, `ResumeBundle`, bridge envelopes, and command-result
  schemas
- prompt-pack and repair-feedback payloads used by supported providers
- artifact ids, digests, lineage links, and replayability of raw versus
  normalized outputs
- command semantics shared across CLI, TUI, harness-command adapters, and later
  automation surfaces

### Rust validation-tooling direction

Roger is a Rust-first local product and should adopt a Rust-specific testing
toolchain where that improves proof quality without bloating the lane model.

Adopted command baseline:

- formatting gate: `cargo fmt --check`
- lint gate: `cargo clippy --workspace --all-targets -- -D warnings`
- broad workspace validation: `cargo test --workspace --all-targets`
- targeted suite replay: `cargo test -p <package> --test <suite> -- --nocapture`

Adopted Rust tools:

- `proptest` for rule matrices, reducer transitions, and other property-style
  unit coverage
- `insta` for stable structural snapshots where Roger wants queue, inspector,
  or controller-state proof without brittle full-terminal goldens
- `cargo llvm-cov` for coverage reporting and ratcheting

Approved but intentionally non-default:

- `cargo fuzz` for parsers, bridge envelopes, and structured-artifact surfaces
- `criterion` for explicit performance-contract work
- `loom` for narrow concurrency seams only when ordinary tests cannot defend the
  interleaving or lock-ordering contract honestly

Deliberate non-adoptions from adjacent Rust projects:

- do not make `rch` or any other remote compile wrapper part of Roger's
  canonical command surface
- do not create extra top-level validation lanes for fuzzing, benchmarking, or
  audits; keep them as supporting tooling around the same three-lane model
- do not let coverage percentages become the primary release truth; named
  invariants, suites, fixtures, and proof artifacts remain the governing model

### Test artifacts and fixture corpus

Tests should produce and consume explicit artifacts rather than relying on
ambient state.

Required artifact families:

- fixture repos for compact review, monorepo, malformed findings, memory scope,
  and same-PR multi-instance behavior
- canned provider-output corpora for valid, partial, raw-only, invalid-anchor,
  and repair-needed paths
- `StructuredFindingsPack` examples and counterexamples
- `ResumeBundle` snapshots for reopen, reseed, and dropout-control flows
- GitHub draft, approval-token, and posted-action payload snapshots
- bridge request/response transcripts
- TUI structural-state snapshots for queue, inspector, approval, and degraded
  states
- migration fixtures and artifact-store integrity fixtures

Rules:

- preserve failing test artifacts in CI where they materially aid diagnosis
- prefer structural snapshots over broad terminal goldens
- keep fixtures small, named, and purpose-built rather than accumulating a large
  opaque corpus

### CI and execution tiers

Roger should separate validation lanes from execution policies.

Validation lanes are only:

- `unit`
- `integration`
- `e2e`

Execution policies decide when and how those lanes run. Recommended policies:

- `local-bead`: the smallest truthful unit or integration slice required before
  committing a bead's changes
- CI reproduction: deterministic reruns of the relevant `unit` and
  `integration` coverage so support claims are reproducible outside one machine
- operator stability: on-demand or scheduled runs of expensive real-surface
  checks, selected integration suites, and the few E2Es that require a licensed
  or brittle environment
- `release-candidate`: an explicit operator gate backed by validation evidence,
  artifact verification, and the release smoke matrix in
  [`RELEASE_AND_TEST_MATRIX.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/RELEASE_AND_TEST_MATRIX.md)

The exact suite-family rules, fixture contract, artifact layout, and E2E budget
guard should live in
[`TEST_HARNESS_GUIDELINES.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/TEST_HARNESS_GUIDELINES.md)
rather than being rediscovered piecemeal during implementation. The concrete
flow-to-suite mapping and fixture ownership should live in
[`VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md).

### Release gate and operator evidence

Release is not a fourth validation lane. It is an explicit operator decision
that consumes current evidence from the three lanes plus release-specific smoke
and artifact checks.

Minimum release-candidate prerequisites:

- required implementation beads are closed or explicitly waived
- support wording for the candidate release is frozen and truthful
- the relevant `unit` and `integration` coverage is green
- the selected E2E set is green
- operator-run smoke for claimed brittle external surfaces is current
- artifact, checksum, installer, and manifest verification is green
- publish approval is explicit rather than ambient

### Manual validation and smoke

Manual smoke should stay small but real and should feed the release-candidate
gate rather than pretending to be its own lane.

Required manual smoke areas:

- CLI launch into a local review on the blessed path
- browser-to-local launch through the serious bridge path
- refresh after new commits
- explicit approval before posting
- plain OpenCode fallback and resume
- at least one concurrent-review or named-instance sanity check

## Rollout Plan

### Phase 0: Converge scope and unknowns

- lock the canonical name as Roger Reviewer
- define minimum v1 surface area
- isolate undefined terms from the brain dump as open questions
- decide what absolutely must ship before extension work starts

### Phase 0.5: Run architecture risk spikes

- validate the harness session boundary with OpenCode primary and at least one
  bounded live-CLI provider path
- validate the browser extension launch bridge
- validate the artifact storage split between DB rows and artifact blobs
- write ADRs for any decision that materially changes the package layout

### Phase 1: Foundation and domain

- set up repo structure and package boundaries
- define domain schema and storage migrations, including scoped memory and index
  state
- build the canonical per-profile Roger store and artifact layout
- build basic review session persistence
- build session linkage to underlying supported harness sessions

### Phase 2: CLI and prompt engine

- implement session-aware CLI
- implement review-stage orchestration
- persist structured findings, artifacts, and episodic summaries
- wire the first curated lexical + semantic retrieval slice and prove degraded
  lexical-only behavior
- prove that a local review loop works without the extension

### Phase 3: TUI

- implement TUI shell
- add findings list/detail/action flows
- surface related historical evidence with scope/provenance cues
- add outbound draft approval UX
- validate refresh and resume behavior

### Phase 4: GitHub integration and extension (v1)

- finalize GitHub adapter behavior
- validate the daemonless launch bridge on supported browsers, including Edge
- implement the minimum viable PR-page extension workflow
- prove launch from a PR page invokes a local review correctly

### Phase 4.5 (v2): Deep extension integration

- status indicator for unresolved findings on PR pages
- PR-aware dropdown with review actions and prompt overrides
- GitHub-specific shortcut integration
- live status reflection without a persistent daemon

### Phase 5: Search, memory, and polish

- harden promotion/demotion, project/org overlays, and conflict handling
- add review-memory workflows and failure-pattern capture
- evaluate optional structured context packaging such as TOON where it actually
  helps
- harden multi-instance and worktree workflows

This order intentionally defers the extension until the local review loop is
real. Otherwise the project risks optimizing the entrypoint before the product
core exists.

## Validation Gates

Do not advance phases casually.

### Gate A: Domain viability

- storage schema exists
- review session and finding lifecycle are explicit
- scope, provenance, and memory-state rules are explicit enough to prevent
  silent cross-scope bleed
- supported-harness session linkage is implemented truthfully, with OpenCode
  primary and bounded live-CLI providers kept literal about their tier
- finding identity and refresh semantics are explicit enough to avoid duplicate
  finding explosions

### Gate B: Core review loop viability

- CLI can start and resume a review
- prompt stages persist outputs cleanly
- findings survive restart
- the first curated retrieval slice works or degrades gracefully to lexical-only
  / DB-backed lookup

### Gate C: TUI usability

- user can review, filter, and change finding states quickly
- outbound drafts are reviewable locally

### Gate D: GitHub bridge realism

- the extension can invoke and coordinate a local review from a PR page
  without requiring a persistent daemon
- the supported v1 browser set includes Edge
- do not count clipboard/manual command copying as satisfying the gate

### Gate E: Safe outbound actions

- nothing posts without explicit approval
- posted outputs are tracked back to local findings

## Risks and Mitigations

### Risk: extension bridge forces a hidden daemon

Mitigation:

- treat bridge validation as an early spike
- define the supported bridge contract up front: Native Messaging only, with
  fail-closed setup guidance when registration is missing
- reject designs that quietly move core state into a background service

### Risk: OpenCode fallback becomes fake

Mitigation:

- require every review session to maintain an underlying OpenCode mapping
- test resume in plain OpenCode explicitly

### Risk: bounded provider support overpromises parity it does not have

Mitigation:

- keep Codex, Claude, and Gemini expectations bounded to Roger-owned
  ledgering, prompt intake, structured/raw capture, and truthful
  `ResumeBundle` reseed
- require unsupported deeper capabilities to fail clearly instead of emulating
  OpenCode semantics poorly
- gate bounded-provider release claims behind the provider acceptance suites in
  [`RELEASE_AND_TEST_MATRIX.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/RELEASE_AND_TEST_MATRIX.md)

### Risk: findings degrade into unstructured text dumps

Mitigation:

- define the finding schema early
- make all surfaces operate on structured findings
- add explicit finding fingerprint and refresh rules before implementing refresh

### Risk: search ambitions stall the product

Mitigation:

- keep the first semantic corpus narrow and curated rather than indexing
  everything
- keep lexical retrieval primary and semantic retrieval best-effort
- make index rebuilds and semantic lag non-blocking through degraded-mode reads

### Risk: worktree and multi-instance sync becomes overengineered

Mitigation:

- start with one canonical Roger store per profile and explicit instance
  overrides for repo-local mutable resources
- optimize only after the base isolation workflow works

### Risk: repo/project/org memory scope bleeds silently

Mitigation:

- keep scope as an explicit filter boundary, not just a ranking hint
- partition indices by scope and union them only when the session allows it
- test for scope-bleed suppression, conflict surfacing, and provenance display

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

These remaining questions are bounded implementation follow-ons. They no longer
block the implementation gate for the first local-core slice. Resolved runtime
and validation-ownership details now live in
[`TUI_RUNTIME_SUPERVISOR_POLICY.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/TUI_RUNTIME_SUPERVISOR_POLICY.md)
and
[`VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md).

- **Future harness expansion**: The `0.1.0` capability tiers and provider
  minima are now fixed. The remaining question is which later providers
  eventually earn Tier A, Tier B, or Tier C support beyond OpenCode and the
  current bounded live-CLI providers. GitHub Copilot CLI is active current scope; the open
  question is how much of its intended tier lands in the current slice, not
  whether it belongs to some undefined later phase.

- **Protocol adapters**: When Roger expands beyond the initial OpenCode and
  bounded-provider paths, which later integrations justify ACP as a
  harness-control edge, which justify MCP as a tool/context edge, and which
  clients such as VS Code, JetBrains, or GitHub Copilot are important enough to
  shape that evaluation?
  The baseline assumption is direct CLI integration first, not ACP-first, until
  a provider proves that ACP materially reduces adapter complexity without
  weakening Roger's safety posture.

- **Semantic packaging**: If hybrid search is in the first Roger search slice,
  which local embedding model ships first, how are its assets installed and
  verified, and when should Roger evaluate code-oriented or sparse variants?

- **Outcome labeling implementation**: What exact storage shape should represent
  merged-resolution links and `UsageEvent` derivation jobs?

- **TOON viability**: Which target models/backends pass enough structure
  correctness and latency tests to justify TOON as an optional packer?

The following topics were open during late planning but are now considered
settled enough for implementation:

- extension packaging and release ownership
- release target matrix baseline
- multi-instance and worktree defaults
- robot-facing CLI surface baseline
- bounded attention-event and notification model
- first-slice readiness without the extension

### Resolved questions

- ~~Rust TUI runtime~~: Confirmed Rust-native. Roger needs a Rust TUI layer.
- ~~Local runtime language bias~~: Favor Rust for CLI, app-core, storage, and
  search unless a platform constraint strongly justifies another language. The
  browser extension is the main expected JS/TS exception.
- ~~Daemonless bridge family~~: WebSocket/local HTTP remain rejected as the
  architectural center because they imply a daemon. The remaining candidates
  were custom URL launch and Native Messaging; `0.1.0` support is now narrowed
  to Native Messaging only.
- ~~Search direction~~: Roger should target Tantivy + FastEmbed from the first
  Roger search slice rather than planning a text-only launch followed by a
  semantic retrofit.
- ~~Credential flows~~: Non-issue. `gh` CLI already handles GitHub auth and
  stores tokens in the OS keychain. Roger inherits this; no separate Keychain
  integration needed for v1.
- ~~`FPs` and `SA` in brain dump~~: Business-specific terms from an unrelated
  project context. Illustrative only, not architectural requirements for Roger.

## Plan-to-Beads Strategy

Convert the plan into beads only after one critique/integration loop confirms
the architecture is stable enough.

For documentation-heavy changes, the analogous rule is:

- do not beadize or implement a new planning direction while it still depends
  on multiple overlapping live plan docs
- first fold the accepted direction into the canonical plan and relevant
  support contracts
- then archive, downgrade, or delete the temporary synthesis docs that are no
  longer needed as live inputs

The first bead graph should be organized around:

- repo foundation
- shared domain, storage, and search/index foundation
- supported-harness session orchestration, with OpenCode primary and bounded
  live-CLI providers kept literal about their tier
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
- the concrete user-visible or operator-visible promise being defended
- the primary ownership surface, such as domain, CLI, TUI, extension,
  bridge/setup, or release/install
- the primary failure, degraded, or recovery cases that are part of the same
  promise
- explicit validation contract, naming the cheapest truthful layer
- explicit in-scope and out-of-scope boundaries
- whether it is v1-critical or later
- the support-claim boundary: what wording becomes safe after the bead lands,
  and what must still remain narrowed
- any required proof artefacts, operator smoke lanes, or repair notes when
  real-surface behavior is involved
- any relevant flow ids from `REVIEW_FLOW_MATRIX.md`
- any relevant provider/browser/OS coverage obligations from
  `RELEASE_AND_TEST_MATRIX.md`

Execution-governance rules:

- beads should be proof-bearing slices, not broad work buckets
- parent beads should usually act as integration checkpoints while child beads
  carry implementation burden
- a bead does not close on "code landed"; it closes on acceptance evidence
- support claims must match live surface + docs + validation, not planning
  intent alone
- if a bead mixes UX creation, failure handling, and support-claim proof across
  several user-visible surfaces, split it unless it is intentionally an
  integration checkpoint
- if a bead touches launch, refresh, approval, posting, install/update, or the
  browser bridge, it should name invalidation, fail-closed, and recovery
  behavior explicitly rather than leaving those obligations implicit
- docs-only reconciliation may clarify truth, but it does not close the
  underlying implementation bead when live operator behavior or live validation
  proof is still missing

## Definition of Done for the Planning Stage

Planning for Roger Reviewer is complete when:

- the canonical markdown plan is internally consistent
- open questions are isolated enough not to block phase 1
- the first bead seed is ready to import into a task system
- the rollout order reflects real technical dependency rather than wishful UI
  ordering
- the safety model is explicit enough to prevent accidental GitHub or local
  environment mutations

These conditions were satisfied and recorded on 2026-03-30 in
[`READINESS_IMPLEMENTATION_GATE_DECISION.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/READINESS_IMPLEMENTATION_GATE_DECISION.md).
