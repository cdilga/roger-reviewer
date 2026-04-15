# Round 06 Provider Truth And Bead Reconciliation Brief

Status: historical gap-inventory brief from 2026-04-14. The live graph has
since widened substantially; use this as rationale and reconciliation context,
not as a current snapshot of `br ready`.
Audience: Roger maintainers and implementers preparing the next cohesive bead-shaping pass
Scope: provider truthfulness, review-worker boundary hardening, lifecycle hardening, Copilot admission, and graph widening needed before the next implementation wave

---

## Why this round exists

Roger already has two relevant planning packets:

- the canonical product plan in `docs/PLAN_FOR_ROGER_REVIEWER.md`
- the bounded side-plan `docs/PLAN_FOR_TRUTHFUL_PROVIDER_PARITY_AND_GITHUB_COPILOT_CLI.md`

The canonical plan has already absorbed much of the provider-parity critique:

- first-class provider admission rules
- launch-truth and transaction rules
- `rr init` / `rr doctor` bootstrap boundary
- explicit draft -> approve -> post productization
- anti-gap rules for provider-support epics and proof beads

Round 05 also explicitly treats the provider-parity side-plan as the active follow-on for provider and lifecycle truth while it narrows the TUI/CLI/extension surface problem.

The current graph still does **not** reflect that full program.

This round exists because the next bead-shaping wave should not assume the provider-parity/Copilot critique is already fully captured just because parts of it were merged into the canonical plan.

---

## Short answer

No.

If we started building beads only from the current open frontier or only from the Round 05 surface brief, we would **not** capture the full criticism set from `docs/PLAN_FOR_TRUTHFUL_PROVIDER_PARITY_AND_GITHUB_COPILOT_CLI.md`.

We need one explicit reconciliation pass that maps:

1. what is already authoritative in the canonical plan
2. what is still stranded only in the provider side-plan
3. what the live graph does not yet represent as proof-shaped work

This document is that pass.

---

## Current repo truth that forces this round

### The graph is too narrow

- `br ready` currently returns only one issue: `rr-1pz7` (`Implement GitHub Outbound Posting Adapter`)
- there are no live open beads for Copilot admission, provider-agnostic `rr doctor`, bootstrap reconciliation, launch-attempt transactionality, or provider-claim re-audit
- there are no `copilot` beads at all in `.beads/issues.jsonl`

### The source-run path is still not truthful

- `cargo run -q -p roger-cli --bin rr -- help` currently fails because `packages/cli/Cargo.toml` contains a duplicate `serde.workspace = true` key
- this means the from-source help or product-entry path is broken at exactly the layer that Round 05 and the provider-parity plan both treat as product truth, not just dev ergonomics

### Provider claims are still drifting across surfaces

Current code and docs do not tell one clean story:

- `packages/cli/src/lib.rs` still routes `review` and `resume` through `opencode`, `codex`, `claude`, and `gemini`
- the human-facing usage string in the same file still says `--provider opencode|codex`
- `AGENTS.md` and the canonical plan still describe `claude` and `gemini` as live `rr review --provider ...` paths
- prior reconciliation work already argued for narrower live claims unless those paths are truly exposed and proven

That is exactly the kind of support-claim drift the provider-parity critique was written to stop.

### Prior provider beads are useful, but not sufficient

Existing closed beads around OpenCode, Codex, Gemini, Claude Code, and provider
acceptance are valuable antecedents. They are **not** enough, by themselves, to
say the provider-parity program is fully containerized now.

The missing pieces are the ones the canonical plan now calls out explicitly:

- retroactive launch-truth hardening
- retroactive transactional lifecycle hardening
- explicit doctor/bootstrap surfaces
- live support-claim re-audit against the actual CLI/help/build path
- Copilot admission through the same proof rules

---

## What is already captured authoritatively

These directions should be treated as settled enough to bead directly:

### Already in the canonical plan

- provider capability tiers and first-class admission rule
- launch-attempt lifecycle distinct from durable `ReviewSession`
- transactional launch/resume/refresh/return rule
- provider-continuity truth categories (`usable`, `degraded`, `unusable`)
- explicit `rr init` / `rr doctor` bootstrap boundary
- explicit outbound draft -> approve -> post surface
- anti-gap rule requiring provider work to split into lifecycle truth, transactionality, outward product surface, and provider-specific integration

### Already in Round 05

- CLI still needs product-facing draft/approve/post, `rr doctor`, and `rr extension uninstall`
- the surface layer should not be widened on top of dishonest lifecycle or support claims
- user-facing help and command surfaces must be layered and truthful

### Still primarily stranded in the provider side-plan

These remain detailed there and are not yet represented by the graph:

- Copilot-specific Tier A/Tier B shape, hook profile, and policy profile
- concrete Copilot file additions and hook artifact responsibilities
- Copilot-specific deterministic doubles and smoke-lane expectations
- provider-aware audit artifact classes and policy-digest status details
- a slice order that ties lifecycle hardening, provider narrowing, and Copilot admission into one cohesive lane

---

## Gap audit against the provider-parity plan

### W1. Hardening the core Roger lifecycle

Status: partially absorbed into the canonical plan, not captured as an active bead group

Still missing as graph-shaped work:

- launch-attempt state machine as a distinct proof unit
- transactional lifecycle retrofits for review/resume/refresh/return
- bridge replacement of “pretend success” with real `rr` execution proof
- bootstrap/help reconciliation tied to the actual shipped CLI surface

### W1A. Review-worker runtime and tool boundary

Status: newly identified planning gap; not represented by the current graph or by Round 05

What is missing:

- a first-class contract for the review worker that actually performs Roger
  review tasks inside a provider session
- a clear semantic split between manager-facing review lifecycle commands and
  worker-facing memory/finding/context tools
- a canonical result envelope for worker-returned findings, clarification
  requests, and follow-up proposals
- an explicit transport decision for that worker surface rather than implicit
  reuse of whatever current CLI commands happen to exist

Why it matters:

- provider hardening alone is not enough if the worker boundary remains
  architecturally implicit
- Copilot admission, memory hooks, clarification, and future MCP evaluation all
  depend on the same missing worker contract
- the current `StageHarness` shape is too thin to express this boundary

### W1B. Search-planner truth and degraded retrieval boundary

Status: newly elevated current-scope gap; not yet represented as an explicit
proof group in the live graph

What is missing:

- an explicit bead group for replacing compatibility-ingress `query_mode=auto`
  with concrete planned search intent before execution
- an explicit bead group for `RecallEnvelope` fields that distinguish requested
  versus resolved planner intent
- an explicit bead group for `recovery_scan` as a degraded recovery mode rather
  than a quiet fallback that can become the de facto search path

Why it matters:

- otherwise the implementation can drift back toward “just use the straight
  simple query” under pressure
- QMD-inspired uplift only matters if Roger actually lands a real planner rather
  than a thin compatibility shim
- active-agent memory access and future CLI/TUI search UX both depend on the
  planner and degraded-mode contract being explicit and testable

### W2. Productize the explicit outbound approval/posting flow

Status: partially captured

What exists:

- Round 05 names the CLI/TUI product-surface gap
- `rr-1pz7` captures the GitHub posting adapter itself

What is still missing as explicit beads:

- command-surface completion for `rr draft`, `rr approve`, and `rr post`
- outbound-state visibility and invalidation as queryable product truth
- recovery and retry proof beyond the adapter transport layer

### W3. Finish OpenCode truthfully before using it as the benchmark

Status: not safe to treat as closed just because earlier OpenCode beads closed

The canonical plan now makes launch truth and transactionality retroactive for existing providers. That means OpenCode needs a re-audit bead group rather than being inherited as “already done.”

### W4. Narrow Codex/Gemini/Claude Code to honest claims

Status: only partially captured

We have some prior narrowing work, but the repo still shows active drift between:

- docs
- help text
- router logic
- supported-provider lists
- previous close reasons

This needs one explicit claim-audit group rather than scattered spot fixes.

### W5. Add GitHub Copilot CLI as the first-class golden-path provider

Status: missing from the graph

The provider side-plan contains a real implementation program here. The live bead graph currently contains none of it.

### W6. Testing plan

Status: only partially captured

Earlier provider acceptance work exists, but the newer critique requires additional proof containers:

- transaction/crash-recovery tests for verified launch and rebinding
- Copilot doubles and provider acceptance
- real-provider smoke lane only when claim level requires it

### W7. Operational safeguards and engineering controls

Status: missing from the graph as a cohesive lane

The graph does not currently expose a cohesive group for:

- provider-agnostic `rr doctor`
- operator-safe `rr init`
- provider audit artifact classes
- policy-digest visibility in status output

---

## Round 06 decisions

### D1. Do not treat the provider-parity side-plan as “already beaded”

It has influenced the canonical plan, but the live graph still lacks the work containers needed to implement it honestly.

### D2. Build the next bead wave around proof groups, not around package names

The provider-parity critique is fundamentally about truthfulness and proof boundaries. The next graph should be shaped around those boundaries first.

### D3. Re-audit previously closed provider work under the newer anti-gap rules

This is not a rollback. It is a recognition that the newer plan now imposes stricter closure standards than some earlier provider slices were shaped against.

### D4. Freeze provider hierarchy separately from implementation order

The authoritative provider support order is:

1. GitHub Copilot CLI
2. OpenCode
3. Codex
4. Gemini
5. Claude Code

That is the product support hierarchy, not the implementation order for the
next bead wave. The bead wave still begins with worker-boundary, provider-truth,
and lifecycle hardening because those lanes are prerequisites for widening the
Copilot claim honestly.

### D4. Keep Copilot in scope, but only behind the same truth rules

Copilot should be admitted as current-scope planned work, not as a shortcut around lifecycle hardening or support-claim discipline.

### D5. The review worker boundary is now part of the same proof program

The next bead wave should not treat review-worker semantics as an implementation
detail hidden inside provider adapters or prompt-engine glue. The worker
runtime/tool boundary is now a first-class planning and proof lane.

### D6. Search-planner truth must be beaded explicitly

The next bead wave should not allow search work to hide behind vague “query”
language or a compatibility-only `auto` ingress.

Required explicit proof groups:

- planner-intent resolution
- recall-envelope truth
- degraded `recovery_scan` truth

---

## Required bead groups for the next cohesive wave

The next bead-shaping pass should widen the graph into at least these groups.

### G0. Review-worker runtime and tool boundary

Own:

- first-class review-worker contract and object model
- explicit split between Roger review management and worker-executed review
  tasks
- dedicated worker-facing tool/context surface for memory, findings, status,
  and artifact reads
- canonical worker result envelope for returned findings and clarification
- explicit transport decision: dedicated worker transport first, MCP optional
  later

Why it exists:

- this is the architectural seam currently missing between provider launch truth
  and structured findings truth
- without this group, Copilot or future worker-tool work will smear semantics
  across the human CLI again

### G1. Build/help truth and provider-surface truth

Own:

- fix the duplicate-key manifest break in `packages/cli/Cargo.toml`
- restore a truthful from-source `rr --help` / `rr help` path
- reconcile router, usage/help text, docs, and robot envelopes so provider exposure tells one story

Why it exists:

- until this lands, every later provider or CLI claim sits on a visibly broken product-entry surface

### G2. Transactional lifecycle and bridge truth

Own:

- launch-attempt ledger/state machine
- transactional review/resume/refresh/return persistence
- real bridge dispatch to `rr --robot`
- stale-event rejection and retry-safe lifecycle handling

Why it exists:

- this is the proof lane that stops Roger from claiming launched sessions before provider binding is real

### G3. Bootstrap and doctor product surface

Own:

- decide and implement the canonical `rr init` story
- implement provider-aware `rr doctor`
- make onboarding/help/recovery route through real shipped commands only

Why it exists:

- the provider-parity critique explicitly treats this as part of product truth, not docs cleanup

### G4. Explicit outbound flow completion

Own:

- `rr draft`
- `rr approve`
- `rr post`
- visible outbound states, approval invalidation, and retry/recovery proof
- completion of the already-open posting-adapter lane without pretending that the adapter alone closes the whole product flow

Why it exists:

- the provider-parity brief and Round 05 both require a fully visible draft -> approve -> post surface

### G5. OpenCode and bounded-provider claim re-audit

Own:

- revalidate OpenCode against the stronger launch-truth and transaction rules
- reconcile Codex, Gemini, and Claude Code exposure to the actual live CLI
  surface
- narrow docs/help/status immediately where proof is still bounded

Why it exists:

- current repo truth still contains support-claim drift

### G6. Copilot admission program

Own:

- `packages/session-copilot`
- provider parsing/config wiring
- hook profile and instructions
- review-safe Copilot policy profile
- feature-flagged Tier A first
- Tier B reopen/return only after proof

Why it exists:

- the provider side-plan already defines this lane; the graph currently does not

### G7. Provider proof and audit hardening

Own:

- deterministic Copilot doubles
- transaction/crash-recovery tests
- provider acceptance expansion where claims widen
- real-provider smoke lane only when support wording requires it
- audit artifact classes and policy-digest status output

Why it exists:

- this is the proof lane that prevents another over-closed provider/support epic

---

## Recommended implementation order for the bead wave

1. `G0` review-worker runtime and tool boundary
2. `G1` build/help truth and provider-surface truth
3. `G2` transactional lifecycle and bridge truth
4. `G3` bootstrap and doctor surface
5. `G4` explicit outbound flow completion
6. `G5` OpenCode and bounded-provider claim re-audit
7. `G6` Copilot Tier A behind a feature gate
8. `G7` Copilot Tier B, provider proof expansion, and smoke-lane admission

Do not invert this order by starting Copilot-first or browser-surface-first work while lifecycle and support claims are still mismatched.

---

## Bead-shaping rules for this round

The next graph should obey these additional rules:

- no single “provider parity” epic should close on adapter coverage alone
- Copilot work must not be the only place lifecycle truth or transactionality is captured
- `rr doctor` / `rr init` must be first-class product beads, not TODO bullets inside a broader CLI bead
- OpenCode re-audit must be explicit instead of assumed from older close reasons
- provider claim-audit work for Codex/Gemini/Claude Code must be separate from
  Copilot implementation
- every provider-support bead group must name its validation lane and whether it widens live support wording
- any bead that changes provider claims must update the canonical plan, release/test matrix, and user-facing help in the same slice

---

## What this round means for future bead creation

If we begin building beads after this round, the bead wave should explicitly cover:

- lifecycle truthfulness
- review-worker runtime/tool boundary
- transactionality/crash safety
- outward product surface completion
- provider-claim re-audit
- Copilot integration
- provider proof and audit lanes

If those groups are present, we will capture the criticism set from
`docs/PLAN_FOR_TRUTHFUL_PROVIDER_PARITY_AND_GITHUB_COPILOT_CLI.md` rather than only the parts already folded into general surface or architecture docs.

If those groups are absent, we will miss the lot again.

---

## Bottom line

The provider-parity/Copilot critique is **not** fully represented by the current bead frontier.

The canonical plan now contains much of the right truth, but the graph still
needs an explicit reconciliation wave before the next cohesive implementation
period.

This round closes the planning gap by naming the missing groups and the order
they should land in. The next step is to shape beads directly from these groups
instead of assuming the side-plan has already been operationalized.
