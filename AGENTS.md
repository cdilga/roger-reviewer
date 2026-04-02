# AGENTS.md — Roger Reviewer

This file is the single source of truth for any agent (Claude Code, Codex, or
other) working in this repository. Read it before doing anything else.

---

## What is Roger Reviewer?

Roger Reviewer is a local-first pull request review system. It combines a
session-aware CLI (`rr`), a TUI-first review interface (FrankenTUI), and a
GitHub browser extension. Its purpose is to drive high-quality review loops,
keep review state durable and searchable, and require explicit human approval
before anything is sent back to GitHub.

The core differentiator is continuity. Every finding, prompt pass, artifact, and
follow-up maps back to a durable local session that can be resumed in plain
OpenCode even if Roger-specific layers are unavailable or compacted.

---

## Critical Constraints — Non-Negotiable

These are not preferences. Violating them is a bug.

- **No automatic GitHub posting.** Outbound actions require explicit human
  approval through the TUI or CLI approval flow.
- **No automatic bug-fixing.** Review and suggest only, unless the user
  explicitly enables fix mode.
- **No writes to dev/test environments** unless the user explicitly enables
  them. Default to read-mostly.
- **No long-running daemon as the architecture center.** The system must be
  daemonless in steady state.
- **OpenCode fallback must stay real.** Every Roger review session must map to
  an underlying OpenCode session that can be resumed directly.
- **Mutation-capable flows must be visibly elevated**, not implicit.
- **No raw review communication via `gh` or other direct GitHub write tools.**
  Agent-authored review comments, questions, and suggestions must flow through
  Roger's draft, approval, and posting model.

---

## Tech Stack (Current Direction)

**Rust-first local runtime direction** — FrankenTUI has been explored and
confirmed Rust-only. See `_exploration/frankentui`. Roger should therefore
favor Rust for local surfaces and local core/runtime layers unless a platform
constraint clearly justifies another language. The browser extension is the main
expected exception because it is web-native.

| Layer | Language | Notes |
|-------|----------|-------|
| TUI | Rust | FrankenTUI `Model` trait; in-process with Roger app-core in `0.1.x`; one primary `rr` binary, supervised background execution, and stable envelopes at external edges |
| CLI (`rr`) | Rust default | Session-aware commands, harness adapter, GitHub adapter |
| App core | Rust default | Domain logic, storage, finding lifecycle |
| Browser extension | TypeScript/JS | WebExtension; Native Messaging is the primary v1 bridge, custom URL launch may remain as a convenience path; keep runtime deps near zero and allow only a small typed toolchain |
| Search | Rust | Tantivy + FastEmbed hybrid targeted for the first Roger search slice |

## Repo Layout (Current High-Level Shape)

```
.
├── apps/
│   ├── cli/           # app-facing CLI crate wrapper / packaging entry
│   ├── extension/     # Chrome/Brave GitHub extension (TypeScript/JS)
├── packages/
│   ├── app-core/      # shared domain and orchestration
│   ├── config/        # layered config model
│   ├── github-adapter/ # gh CLI wrapper
│   ├── bridge/        # Native Messaging / launch bridge support
│   ├── cli/           # primary rr CLI implementation
│   ├── prompt-engine/ # staged review prompt pipeline
│   ├── session-codex/ # bounded Codex harness adapter
│   ├── session-gemini/ # bounded Gemini harness adapter
│   ├── session-opencode/  # OpenCode / harness wrapper
│   ├── storage/       # SQLite + Tantivy / vector search support
│   ├── test-harness/  # shared validation harness helpers
│   ├── validation/    # suite planning / budget tooling
│   └── worktree-manager/
├── _exploration/      # reference repos (do not import as dependencies)
│   ├── frankentui/    # FrankenTUI source — TUI architecture reference
│   ├── asupersync/    # async runtime — v2 extension bridge reference
│   └── cass/          # CASS search — search layer reference implementation
├── docs/              # planning and architecture docs
└── AGENTS.md          # this file
```

---

## Planning Documents

Read these to understand the full plan before touching code.

| Document | Purpose |
|----------|---------|
| [`docs/PLAN_FOR_ROGER_REVIEWER.md`](docs/PLAN_FOR_ROGER_REVIEWER.md) | Canonical product plan — architecture, workflows, rollout phases, risks |
| [`docs/adr/README.md`](docs/adr/README.md) | ADR index for implementation-shaping decisions and proposed contracts |
| [`docs/BEAD_SEED_FOR_ROGER_REVIEWER.md`](docs/BEAD_SEED_FOR_ROGER_REVIEWER.md) | Bead graph seed — epics, acceptance criteria, dependency structure |
| [`docs/CRITIQUE_ROUND_01_FOR_ROGER_REVIEWER.md`](docs/CRITIQUE_ROUND_01_FOR_ROGER_REVIEWER.md) | First adversarial critique and integration round |
| [`docs/CRITIQUE_ROUND_02_FOR_ROGER_REVIEWER.md`](docs/CRITIQUE_ROUND_02_FOR_ROGER_REVIEWER.md) | Second critique round — bridge realism, durability, storage simplification |
| [`docs/CRITIQUE_ROUND_03_FOR_ROGER_REVIEWER.md`](docs/CRITIQUE_ROUND_03_FOR_ROGER_REVIEWER.md) | Third critique round — Rust-first local app, Native Messaging, harness abstraction |
| [`docs/SUPPLEMENTARY_CHATGPT54PRO_FEEDBACK_ROUND_03.md`](docs/SUPPLEMENTARY_CHATGPT54PRO_FEEDBACK_ROUND_03.md) | Raw ChatGPT 5.4 Pro supplementary research and recommendations focused on Roger memory/search architecture |
| [`docs/CRITIQUE_ROUND_03_SUPPLEMENT_FOR_ROGER_REVIEWER.md`](docs/CRITIQUE_ROUND_03_SUPPLEMENT_FOR_ROGER_REVIEWER.md) | Formal integration of the supplementary ChatGPT 5.4 Pro feedback before the next adversarial round |
| [`docs/ROUND_04_ARCHITECTURE_RECONCILIATION_BRIEF.md`](docs/ROUND_04_ARCHITECTURE_RECONCILIATION_BRIEF.md) | Round 04 prep brief — settled versus unresolved architecture and blocked-bead impact |
| [`docs/ROUND_04_ARCHITECTURE_RECONCILIATION_OUTCOME.md`](docs/ROUND_04_ARCHITECTURE_RECONCILIATION_OUTCOME.md) | Formal closeout of Round 04 — reconciled decisions, remaining bounded questions, and bead impact |
| [`docs/PLANNING_WORKFLOW_PROMPTS.md`](docs/PLANNING_WORKFLOW_PROMPTS.md) | Prompts for future critique, integration, and bead polishing rounds |
| [`docs/REPO_ONBOARDING_AND_DISCOVERY_PROMPTS.md`](docs/REPO_ONBOARDING_AND_DISCOVERY_PROMPTS.md) | Reusable prompt pack for repo onboarding and pre-planning discovery |
| [`docs/DATA_MODEL_AND_STORAGE_CONTRACT.md`](docs/DATA_MODEL_AND_STORAGE_CONTRACT.md) | Implementation-facing contract for canonical entities, concurrency, artifacts, and migration boundaries |
| [`docs/CORE_DOMAIN_SCHEMA_AND_FINDING_FINGERPRINT.md`](docs/CORE_DOMAIN_SCHEMA_AND_FINDING_FINGERPRINT.md) | Narrow support contract for the core entity set, finding identity, refresh lineage, and invalidation rules |
| [`docs/PROMPT_PRESET_AND_OUTCOME_CONTRACT.md`](docs/PROMPT_PRESET_AND_OUTCOME_CONTRACT.md) | Support contract for prompt presets, invocation snapshots, and typed outcome events |
| [`docs/ATTENTION_EVENT_AND_NOTIFICATION_CONTRACT.md`](docs/ATTENTION_EVENT_AND_NOTIFICATION_CONTRACT.md) | Support contract for the canonical Roger attention-state model across CLI, TUI, and extension surfaces |
| [`docs/TUI_RUNTIME_SUPERVISOR_POLICY.md`](docs/TUI_RUNTIME_SUPERVISOR_POLICY.md) | Support contract for in-process queue classes, cancellation rules, and bounded refresh cadence |
| [`docs/EXTENSION_PACKAGING_AND_RELEASE_CONTRACT.md`](docs/EXTENSION_PACKAGING_AND_RELEASE_CONTRACT.md) | Support contract for the minimal extension toolchain, contract export, and bridge/extension release ownership |
| [`docs/ROBOT_CLI_CONTRACT.md`](docs/ROBOT_CLI_CONTRACT.md) | Support contract for the `0.1.0` `--robot` command shortlist and stable machine-readable output envelopes |
| [`docs/HARNESS_SESSION_LINKAGE_CONTRACT.md`](docs/HARNESS_SESSION_LINKAGE_CONTRACT.md) | Implementation-facing contract for the Roger-to-harness session boundary, `SessionLocator`, `ResumeBundle`, and adapter obligations (closes `rr-015`) |
| [`docs/SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md`](docs/SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md) | Support contract for prior-review search, semantic asset lifecycle, memory promotion rules, and `0.1.0` scope fence before `rr-024` |
| [`docs/RELEASE_AND_TEST_MATRIX.md`](docs/RELEASE_AND_TEST_MATRIX.md) | Explicit `0.1.0` provider, browser, OS, fixture, and validation matrix |
| [`docs/TEST_HARNESS_GUIDELINES.md`](docs/TEST_HARNESS_GUIDELINES.md) | Canonical implementation-facing contract for suite layers, fixtures, CI tiers, and E2E budget rules |
| [`docs/TEST_EXECUTION_TIERS_AND_E2E_BUDGET.md`](docs/TEST_EXECUTION_TIERS_AND_E2E_BUDGET.md) | Implementation-facing support contract for `0.1.0` test execution tiers, the one blessed automated E2E, and the machine-readable E2E budget guard |
| [`docs/REVIEW_FLOW_MATRIX.md`](docs/REVIEW_FLOW_MATRIX.md) | Scenario matrix mapping Roger user flows to happy-path, variant, and failure/recovery coverage for alignment across TUI, CLI, extension, and harness |
| [`docs/VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md`](docs/VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md) | Planning-stage matrix naming flow families, fixture families, suite families, and support-claim ownership; seeds the `rr-011.x` validation beads (closes `rr-025`) |
| [`docs/VALIDATION_HARNESS_SCAFFOLD_CONTRACT.md`](docs/VALIDATION_HARNESS_SCAFFOLD_CONTRACT.md) | Implementation-facing contract for suite directory layout, naming conventions, metadata envelope schema, helper boundaries, fixture manifests, and failure-artifact preservation rules (closes `rr-025.1`) |
| [`docs/VALIDATION_FIXTURE_CORPUS_AND_MANIFEST.md`](docs/VALIDATION_FIXTURE_CORPUS_AND_MANIFEST.md) | Canonical fixture corpus: all 13 initial fixture families with purpose, allowed consumers, degraded-condition annotations, provenance policy, and update rules (closes `rr-025.2`) |
| [`docs/VALIDATION_CI_TIERS_AND_ENTRYPOINTS.md`](docs/VALIDATION_CI_TIERS_AND_ENTRYPOINTS.md) | CI tier entrypoints (fast-local/PR/gated/nightly/release), suite metadata registration contract, artifact retention rules, and E2E budget guard integration (closes `rr-025.3`) |
| [`docs/READINESS_REVIEW_FIRST_IMPLEMENTATION_SLICE_WITHOUT_EXTENSION.md`](docs/READINESS_REVIEW_FIRST_IMPLEMENTATION_SLICE_WITHOUT_EXTENSION.md) | Narrow readiness result proving the first local implementation slice does not depend on extension delivery |
| [`docs/READINESS_IMPLEMENTATION_GATE_DECISION.md`](docs/READINESS_IMPLEMENTATION_GATE_DECISION.md) | Canonical go/no-go record for moving from planning into implementation |
| [`docs/REFERENCE_SOURCES_AND_EXPLORATION_TARGETS.md`](docs/REFERENCE_SOURCES_AND_EXPLORATION_TARGETS.md) | External standards, prior-art notes, and approved exploration targets for future architecture spikes |
| [`docs/ALIEN_ARTEFACTS_FOR_ROGER_REVIEWER.md`](docs/ALIEN_ARTEFACTS_FOR_ROGER_REVIEWER.md) | Compact artifact pack for external critique sessions |
| [`docs/ALIEN_WORKFLOWS_FOR_ROGER_REVIEWER.md`](docs/ALIEN_WORKFLOWS_FOR_ROGER_REVIEWER.md) | Roger-specific alien-workflow pack for external critique, research-and-reimagine, transfer-audit, and feedback-closure loops |
| [`docs/DEV_MACHINE_ONBOARDING.md`](docs/DEV_MACHINE_ONBOARDING.md) | Practical machine setup guide for Codex, Agent Mail, and planning workflow access |
| [`docs/IMPLEMENTATION_SOURCES.md`](docs/IMPLEMENTATION_SOURCES.md) | Saved implementation-time external sources for browser bridge, contract generation, and workflow methodology |
| [`docs/EXECUTION_GOVERNANCE_AND_REPO_BOUNDARY.md`](docs/EXECUTION_GOVERNANCE_AND_REPO_BOUNDARY.md) | Delivery-governance contract for bead splitting, closure proof, support-claim truthfulness, and repo-vs-operator boundary |
| [`roger-reviewer-brain-dump.md`](roger-reviewer-brain-dump.md) | Original raw brain dump — source of intent, not specification |

---

## Document Authority and Reading Order

Not every planning document has the same authority. Treat them differently.

Authority order for repo work:

1. User instructions in the current session
2. `AGENTS.md` for operational rules and repo workflow
3. [`docs/PLAN_FOR_ROGER_REVIEWER.md`](docs/PLAN_FOR_ROGER_REVIEWER.md) for the
   canonical product and architecture plan
4. [`docs/BEAD_SEED_FOR_ROGER_REVIEWER.md`](docs/BEAD_SEED_FOR_ROGER_REVIEWER.md)
   and the live beads graph for task decomposition
5. Support docs such as the data/storage contract, release/test matrix,
   onboarding, workspace-status, ADRs, execution-governance notes, and prompt packs
6. Historical critique rounds and supplementary feedback
7. [`roger-reviewer-brain-dump.md`](roger-reviewer-brain-dump.md) as raw intent only

Rules:

- If a critique round conflicts with the canonical plan, the canonical plan wins.
- If the bead seed conflicts with the canonical plan, the canonical plan wins
  unless the user explicitly directs a plan update.
- Historical documents exist to explain why the plan changed, not to define the
  current spec.
- The brain dump is context, not authority.

Default reading path for agents:

1. `AGENTS.md`
2. `docs/PLAN_FOR_ROGER_REVIEWER.md`
3. Relevant bead or support doc for the task at hand

Use [`docs/REPO_ONBOARDING_AND_DISCOVERY_PROMPTS.md`](docs/REPO_ONBOARDING_AND_DISCOVERY_PROMPTS.md)
when the task is to study an unfamiliar repo, produce a current-state brief,
or establish an authority map before planning.

Read critique rounds only when:

- you are running a new planning critique round
- you need rationale for why a prior change was made
- the user explicitly asks for historical review context

Do not treat "latest critique doc read" as "current truth". The canonical plan
is the current truth unless the user says otherwise.

---

## Project Stage Status

The project has completed planning, bead polishing, and readiness review.
Implementation is now underway. Planning artifacts remain authoritative for the
intended `0.1.0` shape, but they are no longer a substitute for reading the
current code and live beads. The live bead count changes as work is added and
closed; use `br info` for the authoritative current count and see
[`docs/BEADS_WORKSPACE_STATUS.md`](docs/BEADS_WORKSPACE_STATUS.md) for the
current repair and health notes.

The first real Roger implementation release is now defined as **`0.1.0`**.
When this file or the canonical plan says "v1", read that as the `0.1.0`
release line unless the user explicitly reframes it.

Planning phase checklist:

- [x] Initial plan written
- [x] Critique Round 01 completed and integrated
- [x] Critique Round 02 completed and documented
- [x] Critique Round 03 completed and documented
- [x] Architecture reconciliation after Round 03 and stakeholder review
- [x] Bead polishing (see prompt in `PLANNING_WORKFLOW_PROMPTS.md`)
- [x] Readiness review before implementation begins

Implementation gate status:

- passed on 2026-03-30
- authoritative readiness artifacts:
  [`docs/READINESS_REVIEW_FIRST_IMPLEMENTATION_SLICE_WITHOUT_EXTENSION.md`](docs/READINESS_REVIEW_FIRST_IMPLEMENTATION_SLICE_WITHOUT_EXTENSION.md)
  and
  [`docs/READINESS_IMPLEMENTATION_GATE_DECISION.md`](docs/READINESS_IMPLEMENTATION_GATE_DECISION.md)
- remaining questions are now bounded implementation-shaping follow-ons tracked
  in beads and support contracts; they do not block the first local
  implementation slice
- keep the implementation order local-core-first and extension-last

**Implementation is active. Do not assume a planning doc saying "future" or
"not yet landed" is still true without checking the code and live beads. Do
not start extension delivery or mutable GitHub-write work before the local
core, approval surfaces, and posting safety beads are in place.**

### Current Repo Truth

Use this as the short truth snapshot before you start inferring support from
historical docs.

- the repo contains active Rust implementation across CLI, storage, harness,
  bridge, and validation crates
- the local-first CLI path is real, but not every planned `0.1.0` flow is
  equally complete or equally proven
- support claims must be earned by live command surfaces plus named validation,
  not by planning intent or partially landed adapters
- the extension remains bounded and should not be treated as the source of
  truth for Roger state
- operator/swarm tooling exists in this repo today, but much of it is process
  machinery rather than product scope; do not confuse it with Roger itself
- if the live code, tests, and beads disagree with a planning doc, fix the
  mismatch instead of rationalizing it away

### Non-Goals For Agents

Do not optimize for these:

- closing beads early to make the graph look healthy
- preserving stale support claims because the docs once said them
- treating personal swarm/operator tooling as if it were core Roger product work
- widening claims from adapter coverage to live user-facing support
- adding heavyweight validation when a cheaper truthful layer would defend the
  same promise

---

## Working with Beads

All implementation work is tracked as beads in `.beads/beads.db`. Use `br`
(beads_rust) to interact with them.

Common commands:

```sh
br info              # workspace summary
br list              # all beads with status
br list --status open
br ready             # open, unblocked, not deferred
br show <id>         # full bead detail
br update <id> --status in_progress
br close <id>        # mark a bead complete
br doctor            # workspace health check
```

`br` currently resolves to a local patched build at
`/Users/cdilga/.local/bin/br -> /Users/cdilga/.local/bin/br-0.1.34.pinned`.
This is not the stock upstream `0.1.34` release. Local investigation on
2026-03-31 showed the upstream regression still reproduced on both the
published `0.1.34` build and upstream `main`, but a narrow local source patch
to the fresh-schema migration path restored clean `init`, `create`, `doctor`,
and `info` behavior for Roger's workload.

Important 2026-04-02 follow-up:

- onboarding rehearsal `rr-1f4.2` reproduced claim-mutation FK failures on
  `br-0.1.34.localfix` while `br-0.1.34.pinned` succeeded for the same
  mutation path.
- `scripts/swarm/resolve_br.sh` now defaults to `br-0.1.34.pinned` so swarm
  and onboarding flows converge on the mutation-safe binary without manual
  per-command overrides.
- the prior rollback experiment to `br-0.1.28.pinned` remains rejected for this
  workspace due queue-truth divergence and SQLite integrity risk.
- therefore, keep `br-0.1.34.pinned` as the canonical default for this repo
  unless a newly validated replacement is explicitly announced.

Upstream fresh-init regression report remains:
`Dicklesworthstone/beads_rust#213`.

If `br doctor` reports malformed-page warnings again, repair with:

```sh
sqlite3 .beads/beads.db "PRAGMA wal_checkpoint(TRUNCATE); VACUUM; PRAGMA integrity_check;"
br doctor
```

`wal_checkpoint(TRUNCATE)` alone was not sufficient in local repros; `VACUUM`
was the step that cleared the integrity-check failures. Preserved recovery
artefacts under `.beads/.br_recovery` are still only a cleanup warning.

Do not assume upstream `br 0.1.34` is equivalent to this local build. If you
need to reevaluate a future upstream version, test it explicitly against:

```sh
git init tmp && cd tmp
br init
br create --title "repro one"
br create --title "repro two"
sqlite3 .beads/beads.db "PRAGMA integrity_check;"
br doctor
```

### How to pick your next bead

1. Run `br ready` to see available work, or `br list --status open` for the full queue.
2. Respect the dependency graph — do not start a bead whose dependencies are
   not yet `done`.
3. Mark the bead `in_progress` before starting work.
4. Complete all acceptance criteria before marking `done`.
5. If you discover a blocker or ambiguity, add a note to the bead rather than
   guessing.

If `br` reports `database is busy`, do not treat that as "no work exists".
Wait briefly, then retry. The live queue is authoritative only after a clean
read.

### Bead shaping is allowed

Agents are explicitly allowed to shape the backlog when the current frontier is
too narrow, a bead is obviously too large, or a blocking unknown needs its own
container. Valid autonomy includes:

1. splitting a large bead into smaller non-overlapping child beads
2. creating a planning or design bead to settle a blocking unknown
3. creating a spike bead to test a risky seam or adapter contract
4. creating a bead whose purpose is to widen safe parallel work for other agents
5. adding missing dependency edges or clarifying notes when the graph is
   materially incomplete

Rules:

- new beads must be justified by the canonical plan and current repo reality,
  not invented busywork
- split beads must preserve dependency truth; do not use child beads to dodge a
  real blocker
- announce new or split beads in Agent Mail so other agents can pick them up
- every new implementation bead should include an explicit validation contract

### Beads are proof units, not work buckets

Treat beads as independently provable slices of product progress.

Rules:

- a leaf bead should usually have one clear ownership area, one acceptance
  boundary, and one validation story
- if a bead is too large for one agent to finish and prove in one bounded
  session, split it before or during execution
- parent beads should usually act as integration checkpoints; child beads should
  carry the implementation burden
- if `br ready` is empty but adjacent safe work is obvious, do not declare the
  repo "done"; shape the graph or create the missing child bead instead
- if a bead contains multiple disjoint code areas, multiple support claims, or
  multiple unrelated validation layers, it is probably undersplit

### A bead does not close on "code landed"

Closing a bead requires explicit acceptance evidence, not just changed files or
passing vibes.

Minimum closeout evidence:

1. each acceptance criterion is either satisfied explicitly or left open
2. the exact validation command, suite, or manual smoke is named
3. the actual result is recorded truthfully
4. any residual gap or degraded mode is stated plainly
5. if support wording changed, the relevant docs/tests changed in the same slice

Do not close a bead if:

- acceptance criteria were not checked one by one
- no validation was run for implementation work
- the behavior was inferred from code instead of exercised
- obvious missing child beads or dependency gaps were discovered but left
  untracked
- the close reason would overstate provider, browser, setup, E2E, or approval
  support relative to the current repo

When a bead cannot yet be closed honestly:

- leave it open
- add a note describing the exact remaining gap
- create a child bead if the remaining work is separable

### Validation contract is part of the task

Do not treat validation as an afterthought or as something inferred from vibe.
Every implementation bead should name the cheapest truthful validation layer
that defends its promise.

Minimum validation contract:

1. what promise or acceptance criterion is being defended
2. which layer is required: `unit`, `prop`, `int`, `accept`, `e2e`, or manual
   `smoke`
3. the exact suite name or command expected at closeout
4. the CI tier or release lane it belongs to when relevant
5. any fixture families or failure artifacts the suite depends on

Rules:

- smoke is not a blanket closeout. It is sufficient only when the bead or the
  governing validation docs explicitly say smoke is the right layer
- provider acceptance is not the same thing as end-to-end validation
- do not add a new blessed automated E2E unless the budget and justification
  rules in the validation docs are satisfied
- if a bead is missing a validation contract, add or clarify it before closing
  the bead rather than silently guessing

### Critical dependency spine (v1)

Repo foundation → domain schema → storage → harness linkage → prompt pipeline →
structured findings → session-aware CLI → TUI findings workflow → outbound
draft model → explicit posting flow → GitHub adapter → extension bridge and UI

The extension is intentionally last. Do not start extension work before the
local review core is real.

---

## Domain Model Summary

First-class entities:

- `ReviewSession` — top-level container for a review, linked to a supported
  harness session plus Roger-owned continuity state
- `ReviewRun` — a single pass within a session
- `Finding` — structured result with evidence links and optional attached
  code-location evidence, not free-form text
- `FindingFingerprint` — deterministic ID for stable identity across reruns
- `FindingState` — triage (`new`, `accepted`, `ignored`, `needs-follow-up`, `resolved`, `stale`) and outbound (`not-drafted`, `drafted`, `approved`, `posted`, `failed`) tracked separately
- `CodeEvidenceLocation` — normalized repo code anchor attached to a finding for
  TUI inspection, refresh reconciliation, and thin local editor handoff
- `PromptStage` — exploration, deep review, follow-up
- `Artifact` / `ArtifactDigest` — stored content with content-addressed identity
- `OutboundDraft` — local representation of a proposed GitHub action
- `OutboundDraftBatch` — grouped outbound payload bound to one review target and approval token
- `PostedAction` — immutable audit record after posting
- `ConfigLayer` — one layer of the additive config stack

---

## Architecture Principles

- **Ports and adapters.** The review domain owns findings, sessions, prompt
  stages, and approval state. UI surfaces (TUI, extension, CLI) are adapters.
  They do not reimplement domain rules.
- **Local-first.** Local SQLite is the source of truth. GitHub is a target
  surface.
- **Daemonless.** No always-on background service. The extension bridge must
  use a one-shot or on-demand launch mechanism.
- **Additive config.** Later layers override or extend explicitly. Hidden
  replacement is not acceptable.
- **Approval gates.** Outbound drafts are materialized locally. The posting
  step is explicit and auditable.
- **Roger-mediated GitHub communication.** `gh` may exist behind Roger's GitHub
  adapter, but agents should not bypass Roger by issuing raw review-comment or
  review-submission writes directly.

## Engineering Quality Bar

- Prefer truthful beta support claims over fake parity claims. Roger can commit
  to eventual support across the named matrix, but current release wording must
  still distinguish today's blessed paths from the broader support track.
- Choose explicit, inspectable contracts over ambient magic or hidden fallback.
- Keep failure modes bounded and repairable; fail closed on mutable or
  approval-sensitive paths.
- Treat packaging, install, upgrade, and recovery as first-class product work.
- Prefer boring, durable primitives on critical paths and isolate fast-moving
  dependencies behind Roger-owned adapters.
- Make degraded modes honest: weaker paths may do less, but they must not
  pretend parity with stronger ones.
- Use alien-tier ambition to justify stricter engineering, not sloppier
  engineering. Roger should push beyond normal human-team convenience defaults
  while still demanding explicit contracts, bounded complexity, and hard
  evidence that each added moving part earns its cost.

## Expected Agent Posture

Agents should operate with high initiative and high closure pressure.

Default posture:

- try to finish the full truthful scope of the bead, not just the easiest
  visible fragment
- when the remaining work is adjacent and clear, investigate it and either
  complete it or split it into a new bead immediately
- do not stop at the first local success signal if the real product promise is
  still only partially defended
- do not wait passively for the user to notice a missing child bead, setup gap,
  support-claim mismatch, or validation hole that is already obvious from the
  code and docs
- when you find "a bit more" that is necessary for an honest closeout, treat
  that as part of the job: finish it, or bead it

The desired behavior is:

- finish the bead completely and truthfully when that is feasible
- otherwise leave the repo in a better-shaped state by creating the next obvious
  bead, dependency, or note so another agent can continue without rediscovery

The undesired behavior is:

- doing the narrowest possible interpretation of the bead and stopping
- closing a bead while known adjacent work remains untracked
- treating investigation as optional when it is the difference between "mostly
  works" and "truthfully complete"

## Testing Philosophy

Testing is part of the implementation contract, not cleanup for later. Roger
should get more defensible with every bead that lands.

Core stance:

- almost every implementation bead should add or update tests
- most beads should close with unit or parameterized tests, not heavyweight
  user-flow tests
- integration tests are for real boundaries: storage, migrations, adapters,
  CLI or TUI controller seams, prompt execution edges, bridge envelopes, and
  similar cross-component contracts
- acceptance tests are for explicit support claims such as provider behavior or
  other published capability promises
- automated E2E is intentionally rare and should defend only product-defining
  multi-boundary promises that cheaper layers cannot cover truthfully

Default rule:

1. identify the exact behavior that changed
2. choose the cheapest truthful validation layer that proves that behavior
3. add or update tests in that layer as part of the bead
4. escalate to a heavier layer only when the lower layer would miss the real
   failure mode

Expected defaults:

- domain rules, reducers, serializers, state transitions, refresh
  classification, invalidation logic, and shaping logic should usually get unit
  tests
- small rule matrices should usually get parameterized or property-style tests
  rather than bespoke integration suites
- storage, migration, adapter, CLI routing, prompt execution, bridge envelope,
  and similar seam work should usually get narrow integration tests
- provider or bridge support claims should get named acceptance or smoke
  coverage only when the governing validation docs require it

E2E policy:

- Roger `0.1.x` is deliberately E2E-skeptical
- the planning and validation docs bless only a very small automated E2E
  budget, centered on one heavyweight happy-path review loop
- do not add a new automated E2E because it feels convenient, prestigious, or
  "safer" by default
- do not claim E2E coverage just because a planning doc describes a future E2E;
  only claim the suites that actually exist in the repo and were actually run

Current repo honesty rule:

- if a planned E2E, acceptance suite, or smoke lane does not yet exist in the
  repo, say so plainly
- do not close a bead with wording that implies end-to-end coverage when only
  unit or integration coverage exists
- docs, planning, and bead-shaping tasks may close without code tests, but
  implementation beads normally should not

Authority:

- [`docs/TEST_HARNESS_GUIDELINES.md`](docs/TEST_HARNESS_GUIDELINES.md) is the
  canonical testing-methodology contract
- [`docs/TEST_EXECUTION_TIERS_AND_E2E_BUDGET.md`](docs/TEST_EXECUTION_TIERS_AND_E2E_BUDGET.md)
  is the canonical tiering and E2E-budget contract
- [`docs/REVIEW_FLOW_MATRIX.md`](docs/REVIEW_FLOW_MATRIX.md) identifies the
  user flows whose promises eventually deserve broader cross-boundary coverage

## Support Claim Discipline

Support claims are product commitments, not optimistic interpretations of code.

Rules:

- the code, tests, and docs must agree on what is actually supported now
- a planned capability is not a shipped capability
- adapter coverage is not the same thing as live CLI or UX support
- bounded or degraded support must be described as bounded or degraded, not as
  parity with the primary path
- when live probing contradicts docs or beads, reality wins and the docs or
  graph must be corrected

Required honesty checks:

- provider claims must match the live `rr` surface and named acceptance coverage
- browser or extension claims must match the real launch/readback/install scope
- setup or install claims must match a fresh-user path that was actually
  exercised
- E2E claims must match executable suites that exist and were run

## Dependency Policy

- Dependencies must earn their keep. Prefer a small, high-leverage set over a
  broad convenience tree.
- Prefer standard library, platform APIs, SQLite, and thin Roger-owned code
  before adding wrappers or frameworks.
- Large transitive trees need a strong justification. "Convenient" is not
  enough on its own.
- Every significant dependency should be justified in writing in the plan, ADRs,
  or the implementing bead. "The agent found it easier" is not an acceptable
  rationale on its own.
- Agents are expected to challenge dependency additions during review and
  acceptance. Convenience deps are review targets, not defaults.
- Isolate significant dependencies behind Roger-owned adapters so they can be
  replaced later if they become a security, compliance, or churn problem.
- For the browser extension specifically, prefer browser APIs plus minimal
  hand-rolled TS/JS over framework stacks. A small typed build toolchain is
  acceptable if it strengthens contracts and packaging, but runtime npm
  dependencies still need strong justification because the dependency and
  vulnerability surface is part of the product cost.
- Roger should aim for alien-tier output quality while remaining dependency-
  skeptical: if the swarm can build and own a smaller durable abstraction in-
  repo, that should often beat importing a broad dependency tree that expands
  runtime, packaging, and security surface area.

---

## Rollout Phase Summary

| Phase | Focus |
|-------|-------|
| 0 | Scope and unknown convergence |
| 0.5 | Architecture risk spikes (harness boundary, browser bridge, artifact storage) |
| 1 | Repo structure, domain schema, storage, supported-harness session linkage |
| 2 | Session-aware CLI, prompt pipeline, structured findings |
| 3 | TUI shell, findings workflow, outbound draft approval |
| 4 | GitHub adapter, daemonless bridge, and the minimum viable extension workflow |
| 4.5 (v2) | Deeper extension affordances if they are not already pulled into v1 |
| 5 | Search hardening, multi-instance hardening, and ergonomics |

---

## Validation Gates

Do not advance phases without meeting the gate.

- **Gate A (Domain viability):** schema exists, session/finding lifecycle is
  explicit, supported-harness linkage works truthfully, finding identity
  prevents duplicate explosions.
- **Gate B (Core review loop):** CLI can start and resume, prompt stages persist
  outputs, findings survive restart.
- **Gate C (TUI usability):** user can triage findings, outbound drafts are
  reviewable locally.
- **Gate D (Bridge realism):** the extension can invoke and coordinate a local
  review without a persistent daemon on supported browsers, including Edge.
  Do not count clipboard/manual workarounds as satisfying the gate.
- **Gate E (Safe outbound):** nothing posts without approval; posted outputs
  are tracked back to findings.

---

## If You Are Running a Planning Critique Round

Use the prompts in [`docs/PLANNING_WORKFLOW_PROMPTS.md`](docs/PLANNING_WORKFLOW_PROMPTS.md).

The adversarial review loop:
1. Take the current `docs/PLAN_FOR_ROGER_REVIEWER.md` to a frontier model
   (GPT Pro Extended Reasoning or Claude Opus 4.5 in the web app) using
   Prompt #2 (Plan Review).
2. Bring the output back to Claude Code and use Prompt #3 (Integration) to
   merge revisions in-place.
3. Record the outcome in a new `docs/CRITIQUE_ROUND_NN_FOR_ROGER_REVIEWER.md`.
4. Repeat until suggestions become incremental (typically 4–5 rounds total).
5. Then run Prompt #6 (Bead Polishing) and Prompt #7 (Readiness Review) before
   starting implementation.

---

## If You Are Running an Implementation Bead

1. Read `docs/PLAN_FOR_ROGER_REVIEWER.md` to understand the architectural
   context.
2. Read the bead in full with `br show <id>`.
3. Implement exactly what the acceptance criteria require. No more.
4. Most implementation beads should add or update tests. Start with the
   cheapest truthful validation layer and escalate only when a lighter layer
   would miss the real risk.
5. Verify the bead has an explicit validation contract. If it does not, add a
   note or split the missing validation work before treating the bead as
   closeable.
6. Do not touch GitHub write paths, posting flows, or mutation-capable code
   without the approval model in place.
7. Run the exact validation layer named by the bead or governing validation
   contract before marking done.
8. Record the exact validation command or suite result in the bead close reason
   or bead notes. Do not imply broader coverage than what actually ran.
9. Smoke alone is enough only when the bead explicitly calls for smoke or the
   lower-layer validation docs make that the correct layer.
10. Do not imply automated E2E coverage unless a real `e2e_*` or explicitly
    named heavyweight suite exists in the repo and was run in this workspace.
11. If your change increases the number of blessed automated E2E tests, stop and
   justify why a unit, parameterized, or narrow integration test would not
   defend the same promise more cheaply.
12. If you discover a dependency is incomplete, stop and flag it rather than
   working around it.
13. Do not close the bead until the acceptance criteria and proof have been
    mapped explicitly in the close reason or bead note.
14. If the bead is too large to close honestly in one pass, split it rather
    than forcing a premature close.
15. If you find the next obvious missing slice needed for a truthful closeout,
    investigate it and either finish it if it is already inside scope or create
    the follow-on bead yourself.

## Repo Boundary

This repo should primarily contain Roger product code, product tests, release
machinery, and repo-specific policy/config.

Keep in repo:

- product code and fixtures
- product validation and release scripts
- canonical product and implementation-policy docs
- minimal repo-specific config needed to point external tooling at Roger

Keep out of repo when possible:

- personal swarm launcher/control scripts
- personal Agent Mail dashboards or watcher surfaces
- machine-local observer/bootstrap tooling
- ad hoc repro helpers that are only useful for the operator's own workflow

If tooling is primarily part of your personal multi-agent operating system,
prefer moving it to an external ops toolkit and leaving only a small repo-local
config or wrapper here.

---

## Open Questions (non-blocking as of 2026-03-30)

These are bounded follow-on questions. They do not block the first
implementation slice, but agents should still resolve them in the named beads
or support contracts before implementing the affected surface.

- **Future harness expansion**: ACP may later become a harness-control edge and
  MCP may later become a tool/context edge, but neither should become Roger's
  core architecture or a `0.1.0` dependency.
- **Semantic packaging**: confirm the first local embedding asset and its
  install or verify shape before hybrid search moves from contract to code.
- **Outcome labeling implementation**: settle the exact storage shape for
  merged-resolution links and `UsageEvent` derivation jobs when the prompt and
  usefulness pipeline is implemented.
- **TOON viability**: prove which target backends are strong enough to justify
  TOON as an optional packer instead of plain JSON or compact JSON.

The following topics are no longer pre-implementation blockers because they now
have dedicated support contracts or closed planning beads:

- browser bridge packaging and release ownership
- multi-instance and worktree defaults
- Roger attention-state and notification mirroring
- in-process queue classes, cancellation rules, and refresh cadence
- validation matrix, fixture ownership, and support-claim coverage
- robot-facing CLI surface
- first-slice readiness without the extension

### Harness support policy (`0.1.0`)

Roger should track harness support explicitly rather than letting agents assume
every provider is equally supported.

| Provider | Roger role | `0.1.0` drop-in support | `0.1.0` deeper integration | Notes |
|----------|------------|-------------------------|----------------------------|-------|
| OpenCode | Primary review harness | Yes | Yes | Must preserve real direct-resume fallback |
| Codex | Secondary bounded review harness | Yes | Bounded | Exposed via `rr review --provider codex`; Tier A only today (no locator reopen or `rr return`) |
| Gemini harness | Adapter-contract lane (not current CLI launch surface) | No | Bounded adapter only | Keep Tier A adapter acceptance truthful; do not claim live `rr review --provider gemini` support until it is actually exposed |
| Claude | Future review harness | No | No | Same as Codex |
| Pi-Agent | Future review harness | No | No | Keep room in the adapter contract only |
| GitHub CLI (`gh`) | GitHub adapter, not review harness | N/A | N/A | Write/read adapter for GitHub flows, not a drop-in review engine |

Rules:

- `0.1.0` should feel excellent on the OpenCode path before Roger widens the
  provider matrix.
- Codex and Gemini claims must stay truthful and bounded; only Codex is
  currently exposed in the live `rr review --provider ...` surface.
- Roger may commit to an eventual broader provider/browser/OS support track, but
  current beta claims must still stay honest about which paths are presently
  blessed, acceptance-tested, or partial.
- New providers should only be added when Roger can specify what continuity,
  findings capture, approval safety, and recovery actually mean for them.

Capability-tier rule:

- Tier A bounded support: start session, seed from `ResumeBundle`, capture raw
  output, feed structured-findings normalization or repair, bind review target,
  and report continuity quality
- Tier B continuity support: Tier A plus reopen by locator, bare-harness mode,
  and `rr return`/equivalent return path
- Tier C ergonomic support: Tier B plus optional Roger-native in-harness
  commands and related bindings

`0.1.0` intent:

- OpenCode should reach Tier B and may expose selected Tier C affordances
- Codex currently exposes a bounded Tier A path in the live CLI
- Gemini Tier A remains adapter-contract coverage until `rr` provider launch
  support is actually exposed
- no provider is allowed to claim deeper support than its capability tier earns

Harness-native Roger commands are optional in `0.1.0`. If implemented, prefer
the safe subset `roger-help`, `roger-status`, `roger-findings`, and
`roger-return`. Approval/posting stays in the TUI or canonical `rr` flow.

Cross-harness session portability is a future-direction concern, not a
`0.1.0` dependency. If a stable Jeffrey Emanuel portability layer such as CASR
proves mature enough later, evaluate it for v2 behind Roger's own harness
contract rather than making it the foundation of `0.1.0`.

Future editor/client surfaces such as VS Code, JetBrains, and GitHub Copilot
should be treated as later clients over Roger-owned contracts or future
ACP/MCP edge adapters, not as reasons to make Roger protocol-first in
`0.1.0`.

### Resolved
- ~~FrankenTUI runtime~~ → Rust-native confirmed. Roger must have a Rust TUI layer.
- ~~TUI/app-core process split~~ → Roger stays in-process in `0.1.x`; the
  remaining question is worker/wake behavior rather than whether Roger starts
  with a general local IPC architecture.
- ~~Browser bridge family~~ → Native Messaging is the serious v1 bridge; custom
  URL launch may remain as a convenience path only.
- ~~Canonical Roger store shape~~ → One canonical Roger store per profile is
  the default; named instances isolate repo-local mutable resources before they
  isolate the DB.
- ~~Semantic search direction~~ → First Roger search slice is expected to ship with both text and semantic retrieval, likely Tantivy + FastEmbed in Rust.
- ~~Credential flows~~ → Non-issue. `gh` CLI owns GitHub auth. No Keychain work needed.
- ~~Configuration topology and prompt ingress~~ → `repo` is the default scope;
  `project` is an explicit Roger-managed allowlist overlay; future `org`
  profiles are opt-in only; web-path prompt ingress stays bounded to preset
  selection plus a short explicit objective; effective config remains
  inspectable and additive.
- ~~Canonical source defaults~~ → auto-canonical by default only for repo
  `AGENTS.md`, repo-local Roger policy/config docs, and explicitly bound
  ADR/policy directories. Generic `README.md`, `CONTRIBUTING.md`, templates,
  and broad notes are searchable evidence, not high-trust canonical policy by
  default.
- ~~`FPs` / `SA`~~ → Irrelevant to Roger architecture.

<!-- bv-agent-instructions-v1 -->

---

## Beads Workflow Integration

This project uses [beads_viewer](https://github.com/Dicklesworthstone/beads_viewer) for issue tracking. Issues are stored in `.beads/` and tracked in git.

**Note:** `br` is non-invasive and never executes git commands. After
`br sync --flush-only`, manually run `git add .beads/` and `git commit` when
you want to record beads changes.

`mcp_agent_mail` installs an interactive-shell compatibility alias
`bd='br'`. Some external docs and `bv --robot-*` outputs may still emit `bd`
examples; translate them to `br` for automation and repo docs.

### Essential Commands

```bash
# View issues (launches TUI - avoid in automated sessions)
bv

# CLI commands for agents (use these instead)
br ready              # Show issues ready to work (no blockers)
br list --status open # All open issues
br show <id>          # Full issue details with dependencies
br create --title="..." --type task --priority 2
br update <id> --status in_progress
br close <id> --reason "Completed"
br close <id1> <id2>  # Close multiple issues at once
br sync --flush-only  # Export DB state to .beads/issues.jsonl
git add .beads/
git commit -m "sync beads"
```

### Workflow Pattern

1. **Start**: Run `br ready` to find actionable work
2. **Claim**: Use `br update <id> --status in_progress`
3. **Work**: Implement the task
4. **Complete**: Use `br close <id>`
5. **Sync**: Always run `br sync --flush-only` at session end, then commit `.beads/`

### Key Concepts

- **Dependencies**: Issues can block other issues. `br ready` shows only unblocked work.
- **Priority**: P0=critical, P1=high, P2=medium, P3=low, P4=backlog (use numbers, not words)
- **Types**: task, bug, feature, epic, question, docs
- **Blocking**: `br dep add <issue> <depends-on>` to add dependencies

### Session Protocol

**Before ending any session, run this checklist:**

```bash
git status              # Check what changed
git add <files>         # Stage code changes
br sync --flush-only    # Export beads changes
git add .beads/         # Stage beads export
git commit -m "..."     # Commit code
br sync --flush-only    # Export any new beads changes
git add .beads/
git commit -m "sync beads"
git push                # Push to remote
```

### Best Practices

- Check `br ready` at session start to find available work
- Update status as you work (in_progress → closed)
- Create new issues with `br create` when you discover tasks
- Use descriptive titles and set appropriate priority/type
- Always `br sync --flush-only` before ending session, then commit `.beads/`

<!-- end-bv-agent-instructions -->
