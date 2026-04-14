# Round 04 Architecture Reconciliation Brief

Status: prep artifact for Round 04. This is not the canonical spec. If it
conflicts with `AGENTS.md` or `PLAN_FOR_ROGER_REVIEWER.md`, those canonical
documents still win until a later integration pass updates them.

Outcome: Round 04 was closed via
[`ROUND_04_ARCHITECTURE_RECONCILIATION_OUTCOME.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/ROUND_04_ARCHITECTURE_RECONCILIATION_OUTCOME.md).

Date: 2026-03-29

## Purpose

Round 04 should not behave like another broad ideation pass. The planning set
is already opinionated. The remaining work is to reconcile accepted decisions,
remove stale ambiguity, and turn the last architecture questions into explicit
implementation contracts that will not block the first real build slices.

This brief exists to separate:

- what is already effectively decided
- what still needs a real decision
- what is merely stale wording in higher-level docs
- which open beads are blocked by those remaining gaps

## Source Set Reviewed

- `AGENTS.md`
- `docs/PLAN_FOR_ROGER_REVIEWER.md`
- `docs/DATA_MODEL_AND_STORAGE_CONTRACT.md`
- `docs/RELEASE_AND_TEST_MATRIX.md`
- `docs/adr/001-rust-first-local-runtime.md`
- `docs/adr/002-harness-and-session-durability-contract.md`
- `docs/adr/003-browser-bridge-and-extension-dependency-policy.md`
- `docs/adr/005-multi-instance-and-resource-isolation.md`
- `docs/adr/008-tui-runtime-and-concurrency-boundary.md`
- `docs/adr/007-harness-native-roger-command-surface.md`
- `docs/CRITIQUE_ROUND_03_FOR_ROGER_REVIEWER.md`
- `docs/CRITIQUE_ROUND_03_SUPPLEMENT_FOR_ROGER_REVIEWER.md`
- open beads `rr-012`, `rr-015`, `rr-021`, and `rr-023`

## What Is Already Decided Enough To Stop Re-litigating

### 1. Rust-first local runtime and in-process TUI/core boundary

This is no longer an open ideology debate.

- Rust owns the TUI, CLI default path, app-core default path, storage, search,
  and local orchestration
- the browser extension is the main JS/TS exception
- the TUI and Roger app-core stay in-process in `0.1.x`
- the remaining local-runtime question is worker supervision and wake policy,
  not whether Roger should begin with a general TUI-to-core service split

Source: ADR 001 and ADR 008.

### 2. Roger-owned harness contract and session ledger

The core direction is already chosen.

- Roger owns a `HarnessAdapter` boundary
- Roger owns the durable session ledger
- `SessionLocator` and `ResumeBundle` are distinct objects with different jobs
- OpenCode is the primary path
- Gemini is a bounded secondary path
- plain-harness dropout and `rr return` are first-class, not emergency-only

Source: ADR 002.

### 3. Native Messaging as the primary serious browser bridge

This is no longer a binary architecture choice.

- Native Messaging is the primary v1 bridge
- custom URL launch can remain as convenience and recovery
- localhost HTTP/WebSocket is rejected because it recenters the architecture on
  a daemon
- the extension should stay thin and dependency-light

Source: ADR 003.

### 4. One canonical Roger store per profile

The storage/isolation baseline is already chosen.

- one canonical Roger store per user profile by default
- single-repo mode is the default path
- worktrees are opt-in, not ambient
- named instances isolate repo-local mutable resources before they isolate the
  Roger DB itself
- DB-copy synchronization is not the default model

Source: ADR 005 and the storage contract.

## Main Reconciliation Finding

The high-level docs had been describing several areas as if the base direction
were unsettled, but the ADR set had already narrowed them substantially. This
reconciliation pass tightens that wording; the remaining work is now mostly
contract closure rather than directional re-architecture.

That means Round 04 should focus on:

- reconciling stale wording in `AGENTS.md` and the canonical plan
- freezing exact contracts and default behaviors
- moving any leftover sub-questions into scoped follow-up tasks instead of
  leaving them ambient

The architecture is not blocked by missing philosophy. It is blocked by
unfrozen contracts.

## True Remaining Architecture Questions

These are the decisions Round 04 still needs to drive to closure.

### 1. TUI wake and background-task model

The broad runtime split is now decided. The remaining local-runtime question is
how the accepted in-process model handles long-running work and cross-process
refresh.

Already decided:

- the system is Rust-first
- the TUI and app-core stay in-process in `0.1.x`
- Roger should invest first in stable external contracts, not in a mandatory
  local TUI-to-core IPC boundary
- the Roger TUI foreground loop is synchronous
- background work should run behind Roger-owned worker channels or a supervised
  executor

Source: ADR 001 and ADR 008.

Still unresolved:

- whether the first background-task model should be plain threads plus channels,
  a dedicated async-runtime thread, or a small supervisor wrapper
- the wake/refresh policy for surfacing cross-process updates into an active TUI
- which actions should always bounce the user into the TUI versus returning a
  compact machine-readable or harness-local result

What Round 04 should decide:

- the first worker/supervision model
- the bounded wake/refresh strategy
- the result-shape policy for cross-surface actions

Suggested Round 04 decision:

#### First implementation shape

- Roger should ship as a small Rust workspace with shared crates, not as a
  split local-service architecture.
- The TUI process should host the Roger TUI runtime, the Roger command router, domain
  access, and view-model assembly in-process.
- CLI commands, bridge host entrypoints, and other automation entrypoints may
  run as separate Roger processes against the same canonical store.
- Long-running or latency-tolerant work such as harness IO, indexing, bridge
  handling, and GitHub adapter requests should run on supervised worker threads
  or a dedicated async-runtime thread behind Roger-owned channels.
- Same-session concurrency should be solved at the canonical store and event
  layers via session lease or optimistic `row_version` conflict handling, not by
  inventing a mandatory local IPC service for the TUI.

#### Protocol envelope shape

Roger should define one versioned envelope family now and reuse it at the
external edges. The TUI does not need to speak this over IPC in `0.1.0`, but
its internal command routing should map cleanly onto the same concepts.

Recommended envelope:

- `schema_version`
- `kind`: `request` | `response` | `event`
- `name`: logical operation or event name
- `correlation_id`
- `source_surface`: `tui` | `cli` | `bridge` | `harness_command` | `agent`
- `session_id` when bound
- `run_id` when bound
- `instance_id` when relevant
- `payload`
- `error` for failure cases

Recommended initial logical names:

- requests: `resume_session`, `refresh_review`, `show_findings`,
  `ask_clarification`, `open_drafts`, `return_to_roger`
- events: `session_updated`, `findings_updated`, `drafts_updated`,
  `attention_state_changed`, `background_job_changed`

Rule:

- keep the envelope small and ordinary JSON at stable external boundaries
- do not make TOON, protobuf, or a local IPC transport part of the required
  `0.1.0` TUI/core boundary

#### Stable in `0.1.0`

- browser bridge request/response envelopes
- robot-facing CLI command outputs for whichever `rr --robot` commands Roger
  explicitly commits to
- harness-native Roger logical command IDs and result semantics if any are
  enabled in `0.1.0`
- persisted `ResumeBundle`, `SessionLocator`, and structured-findings contracts
  to the extent they are stored and consumed across processes or restarts
- canonical state and event semantics at the storage layer: findings, approval,
  posting lineage, attention state, and conflict handling

#### Internal in `0.1.0`

- exact in-process Rust trait/module boundaries between TUI, router, and domain
  code
- worker supervision internals: plain threads, channels, or a dedicated async
  executor thread
- TUI wake strategy details: polling cadence, notification mechanism, or other
  bounded refresh implementation
- view-model composition and reducer/state internals
- any future extraction seam for a stronger cross-process app-core service

Recommended wording change for Round 04:

- stop treating "TUI ↔ app-core protocol" as "should Roger split TUI and core
  into separate processes"
- treat it as "which external envelopes and internal wake/task contracts must be
  frozen now, given that the first implementation is in-process"

Why it matters:

- it affects package layout, testing seams, bridge reuse, and how expensive it
  will be to add later clients

### 2. Harness capability contract beyond the current narrative

The broad adapter direction is accepted, and the repo now has enough information
to freeze the `0.1.0` implementation contract without waiting for another broad
ideation pass.

Already decided:

- `SessionLocator`, `ResumeBundle`, and `HarnessCapabilitySet` are core objects
- OpenCode is primary
- Gemini is bounded
- Roger owns continuity instead of inheriting provider semantics blindly

Still unresolved:

- the exact schema budget and size discipline for `ResumeBundle`
- whether any provider beyond OpenCode should pursue Tier B or Tier C in a
  later release

What Round 04 should decide:

- integrate the resolved capability table into the canonical plan and AGENTS
- ensure `rr-015` and the release matrix match the frozen `0.1.0` contract
- move only the genuinely later provider-expansion questions into follow-up work

Suggested Round 04 decision:

#### Provider capability tiers

Roger should classify harnesses by capability tier, not by brand-specific
special casing.

- **Tier A: bounded supported harness**
  - start a Roger-owned review session
  - seed from `ResumeBundle`
  - capture raw stage output durably
  - feed Roger's structured-findings normalization or repair path
  - bind the run to a review target explicitly
  - report continuity quality truthfully enough for Roger to choose reopen
    versus reseed
- **Tier B: continuity-capable harness**
  - everything in Tier A
  - reopen by locator
  - open in bare-harness mode
  - return cleanly to Roger
- **Tier C: ergonomic harness**
  - everything in Tier B
  - optional Roger-native in-harness commands
  - optional artifact-reference attachment or similar richer ergonomics

`0.1.0` provider target:

- OpenCode: Tier B, with selected Tier C affordances allowed but not required
- Gemini: Tier A only
- future providers: admitted against the same tier table rather than through
  bespoke contract branches

#### Continuity-quality rule

Roger should use only three continuity outcomes:

- `usable`
- `degraded`
- `unusable`

Rule:

- `usable` means Roger can continue in the original provider session without
  lying about target, run binding, or operator control context
- `degraded` means Roger can continue truthfully only by reseeding from
  `ResumeBundle`, or reopen succeeded but does not meet Roger's confidence bar
- `unusable` means Roger cannot reopen and cannot reseed truthfully enough to
  continue

Roger should continue in the original provider session only when locator reopen
succeeds, the review target still matches, the adapter reports `usable`, and
the user did not request a fresh session. Otherwise Roger should reseed or fail
closed.

#### Harness-native command stance

- No provider is required to support Roger-native in-harness commands in
  `0.1.0`.
- OpenCode may expose a small safe subset if it can do so cleanly.
- Gemini is not required to expose any Roger-native commands in `0.1.0`.

Preferred first subset when implemented:

- `roger-help`
- `roger-status`
- `roger-findings`
- `roger-return`

Everything else stays optional, and approval/posting remains elevated in the
TUI or canonical `rr` path.

Why it matters:

- this is the blocker behind `rr-015` and part of the truthfulness bar for the
  release matrix

### 3. Extension build and packaging contract

The strategic direction is already chosen. The practical build contract is not.

Already decided:

- TS is allowed as a small typed toolchain
- browser runtime dependencies should be near zero
- Rust owns the bridge contract types
- Native Messaging host install and extension packaging are part of product work

Still unresolved:

- exact build stack: plain `tsc` only versus one narrow bundler or transpile
  helper if browser entrypoint realities force it
- how contract generation from Rust lands in the extension tree
- which Roger-owned commands/scripts own host-manifest generation, packing, and
  install flows
- how release automation publishes companion binaries plus extension artifacts
  without devolving into a manual packaging process

What Round 04 should decide:

- the minimal accepted extension toolchain
- the contract-export workflow
- the release/devops ownership boundary for binary and extension artifacts

Why it matters:

- this is the blocker behind the practical start of `rr-021`, not the bridge
  philosophy itself

### 4. Multi-instance/worktree defaults and preflight UX

The architecture stance exists, but the operator model still needs real rules.

Already decided:

- single checkout plus recorded repo snapshot is the default path
- worktrees are opt-in
- one canonical Roger store per profile is default

Still unresolved:

- which resource classes get first-party primitives in `0.1.0`
- what default copy rules Roger ships for `.env`, `.env.local`, and similar
  files
- what the first port, local DB, docker/container, cache, artifact, and log
  isolation strategies are
- how preflight diagnostics classify conflicts and what Roger suggests by
  default
- when a separate Roger profile is justified instead of a named instance

What Round 04 should decide:

- the initial built-in isolation matrix
- the preflight checklist and conflict classes
- the minimum viable named-instance UX

Why it matters:

- this is the blocker behind `rr-023`, and it is also required to make the
  bridge and multi-reviewer story truthful

### 5. Robot-facing CLI surface

This is still a real architecture question because it shapes the durable CLI
contract and future agent automation.

Already decided:

- `--robot` is the naming direction
- JSON is the safe structured default
- robot mode must preserve command semantics rather than invent a second
  workflow

Still unresolved:

- which commands must support robot mode in `0.1.0`
- the exact stable schemas for those commands
- where `compact` and `toon` are justified versus where plain JSON should be
  the only stable format
- what the discovery surface should look like for machine-readable command docs

What Round 04 should decide:

- the phase-1 robot command shortlist
- the stable schema commitment for each chosen command

Why it matters:

- it affects CLI design now; punting it too long risks breaking automation later

### 6. Configuration topology and prompt ingress

The layering philosophy is clear, but the topology rules are not fully frozen.

Already decided:

- additive layering is required
- effective config must be inspectable
- prompt ingress should normalize into one shared review-intake shape

Still unresolved:

- the exact product rule for `project` boundaries across repos
- how org-level or workspace-level profiles avoid ambient bleed
- where launch profiles live in the same layering model
- how much prompt authoring the extension can do in `0.1.0` beyond preset plus
  short objective

What Round 04 should decide:

- the first explicit topology rule for `repo`, `project`, and future `org`
- the initial inspectable config-resolution model
- the hard boundary for extension-side prompt ingress

Why it matters:

- this is the difference between additive config and future config drift

### 7. Attention-event model and notification surfaces

The plan has good principles, but the integration shape is still loose.

Already decided:

- Roger owns canonical attention states
- other surfaces mirror them rather than redefine them

Still unresolved:

- the canonical event schema
- whether attention is a session attribute, event stream, or both
- which surfaces are mandatory in `0.1.0` versus optional later mirrors

What Round 04 should decide:

- the minimum event/state contract
- the v1 mirror surfaces Roger is actually committing to

Why it matters:

- it shapes TUI overview, CLI status, and extension readback behavior

## Secondary But Real Open Questions

These matter, but they should not dominate the first reconciliation pass.

### Semantic search packaging

The plan now consistently wants lexical plus narrow local semantic retrieval in
the first real Roger search slice. The still-open question is operational:

- which embedding model ships first
- how model assets are installed and verified
- what degraded-mode guarantees remain if semantic assets are absent or stale

### Project boundary and memory promotion policy

The scope and memory posture are much improved, but the product rule for when
multiple repos qualify as one `project` overlay still needs a crisp definition.

### TOON viability

TOON is already narrowed to optional prompt packaging. The remaining question is
whether any target model/backend combination is good enough to justify it in
`0.1.0` for any command at all.

## Document Drift That Round 04 Should Clean Up

### 1. `AGENTS.md` needed several open-question and tech-stack rows tightened

Examples:

- browser bridge family
- worktree/store baseline
- Rust-first local runtime

These are no longer greenfield questions. The remaining work is contract detail
and packaging/UX detail.

### 2. The canonical plan and ADR set were partially out of phase

The plan already contained most of the accepted direction, but some wording was
broader than the ADR conclusions. This pass narrows that language further, but
Round 04 still needs to turn the remaining sub-questions into follow-up
contracts or beads.

### 3. The bead graph is still blocked despite new ADR progress

`rr-012` remains open even though several package-shaping ADRs now exist. Round
04 should either:

- declare which parts of the spike are complete and close or split `rr-012`, or
- identify the specific remaining spike outputs still needed before it can close

## Bead-Level Impact

The blocked beads line up cleanly with the unresolved contracts:

- `rr-012`: architecture spikes and ADR capture need reconciliation against the
  newer ADR set and any still-missing spike outputs
- `rr-015`: blocked mainly by the harness capability matrix and exact
  session/resume contract
- `rr-021`: blocked mainly by extension packaging/release contract and the
  finalized browser/local bridge surface
- `rr-023`: blocked mainly by the instance isolation matrix and preflight UX

## Recommended Round 04 Agenda

Round 04 should aim to finish architecture reconciliation, not restart design
from first principles.

### Step 1: Reconcile the planning baseline

- update the canonical plan and `AGENTS.md` so accepted ADR decisions are
  reflected cleanly
- narrow each remaining open question into a scoped contract decision

### Step 2: Freeze the local runtime/process model

- confirm the `0.1.x` in-process TUI/runtime shape from ADR 008
- define the external Roger envelope family and the small set of stable logical
  command and event names
- define which persistence and external-edge boundaries are frozen in `0.1.0`
  and which TUI/runtime seams remain internal

### Step 3: Freeze the harness capability matrix

- integrate the explicit provider minima for OpenCode and Gemini
- confirm the `ResumeBundle` scope and continuity-quality rule
- confirm the accepted `0.1.0` harness-native command stance

### Step 4: Freeze extension packaging and release ownership

- choose the minimal TS build toolchain
- define Rust-to-TS contract generation
- define local install, pack, and release automation ownership

### Step 5: Freeze named-instance defaults

- define the first-party isolation primitives
- define the preflight matrix
- define when Roger profiles diverge from named instances

### Step 6: Freeze the robot CLI minimum contract

- choose the early `--robot` commands
- publish stable JSON schemas for those commands

### Step 7: Move leftovers into explicit follow-up tasks

- anything not required to start Phase 1 should leave Round 04 as a bounded
  follow-up bead or later ADR, not as ambient uncertainty

## Exit Criteria For Round 04

Round 04 should be considered successful if:

- `AGENTS.md`, the canonical plan, and the ADR set tell the same story
- the remaining open questions are contract-sized rather than philosophy-sized
- `rr-012` is either closable or split cleanly
- `rr-015`, `rr-021`, and `rr-023` each have enough architectural precision to
  become actionable after readiness review
- the planning set is ready for bead polishing and then readiness review rather
  than another broad architecture rewrite
