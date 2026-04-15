# Plan For Transactional Launch Lifecycle And Bridge Truth

Status: Proposed bounded side-plan. Accepted directions should be folded back
into `docs/PLAN_FOR_ROGER_REVIEWER.md` and the relevant support contracts rather
than left here as a long-lived parallel source of product truth.
Audience: Roger maintainers and implementers working on lifecycle truth, bridge
realism, and cross-provider session binding in `0.1.x`
Scope: `rr review`, `rr resume`, `rr return`, and browser bridge handoff
semantics where a real provider session must be verified before Roger claims
success. Refresh-like reconciliation is automatic and bounded, not a separate
operator command.

---

## Why this plan exists

Round 06 identified transactional launch lifecycle and bridge truth as the
hardest under-investigated lane left in the current planning stack.

The canonical plan already says the right high-level things:

- Roger must not report success before a real provider session is verified
- launch/resume/return should be transactional per user-visible action
- stale review-state reconciliation should happen automatically when Roger
  re-enters or observes drift, not as a standalone operator verb
- the bridge must not return pretend success
- the launch-attempt lifecycle must be distinct from the durable
  `ReviewSession` lifecycle

What is still missing is the implementation-facing plan that turns those rules
into one coherent design.

This document exists to close that gap.

---

## Current repo truth

The current code confirms the problem is real.

### `rr review` still binds truth too late

In `packages/cli/src/lib.rs`, `handle_review` currently:

1. launches the provider adapter first
2. gets back a `SessionLocator`
3. then creates `review_sessions`, `review_runs`, `ResumeBundle` artifacts, and
   `session_launch_bindings`

That means Roger has no durable launch-attempt object while the external launch
is in flight, and no explicit recovery story if the process crashes between the
provider launch and the final Roger commit.

### The bridge still returns pretend success

In `packages/bridge/src/lib.rs`, `handle_bridge_intent` currently returns
success for `start_review`, `resume_review`, and `show_findings` without
actually invoking `rr`. The response body even leaves the session id empty.

That violates the canonical rule that the serious bridge path must invoke the
real CLI and return Roger-owned ids only after the command really succeeded.

### Storage has launch routing, but not launch execution truth

`packages/storage` already has useful routing and preflight objects:

- `session_launch_bindings`
- `launch_preflight_plans`

It does **not** currently have a dedicated aggregate for one real launch
attempt that can survive partial failure and later reconciliation.

That is the missing center of this lane.

---

## Goal

Make Roger's launch lifecycle truthful and crash-safe without introducing a
daemon or holding long-lived database transactions across external process
execution.

Concretely:

- Roger should durably record that an attempt exists before it launches or
  reopens a provider
- `ReviewSession` and `ReviewRun` rows should appear only after provider
  verification succeeds
- bridge success should mean a real `rr` command completed and returned a
  canonical Roger session id
- partial failure should leave behind an attempt record and evidence, not a
  fake completed session

---

## Non-goals

This plan does **not** attempt to solve:

- Copilot-specific hook design in detail
- final `rr doctor` UX or provider-by-provider doctor guidance
- the full `rr draft` / `rr approve` / `rr post` command surface
- TUI focus/open semantics such as `rr open --focus findings`

Those are adjacent lanes. This plan only defines the transactional lifecycle
and bridge-truth substrate they will depend on.

---

## Hard constraints

These are non-negotiable:

### C1. No durable review session before verified provider binding

Roger must not create or finalize a durable `ReviewSession` until a provider
returns verification evidence that includes a real provider session id.

### C2. No long-lived SQL transaction across external process launch

Roger cannot solve this by opening one SQLite transaction, spawning an external
provider, waiting on it, then committing later. That would hold locks too long
and create avoidable concurrency failure modes.

### C3. No bridge success without canonical Roger ids

The browser bridge must not return success unless the real `rr` command
completes and the returned machine payload contains the canonical Roger ids the
bridge action requires.

### C4. No stale verification event may bind to the wrong attempt

Hook output, temp files, or provider-side artifacts from one old attempt must
never be able to complete a different later attempt.

### C5. No new daemon

The design must stay daemonless. Launch truth must be achieved through
store-backed attempts, explicit verification evidence, and one-shot CLI/bridge
execution.

---

## Core decisions

### D1. Introduce `LaunchAttempt` as a first-class aggregate

Roger needs a durable aggregate that represents one specific attempt to:

- start a review
- resume a review
- return from bare harness context

`LaunchAttempt` is not the same thing as:

- `ReviewSession`
- `ReviewRun`
- `session_launch_binding`
- `launch_preflight_plan`

Those aggregates answer different questions.

### D2. Use a two-phase lifecycle, not one giant transaction

Every user-visible launch-like action should follow this shape:

1. create a durable pending `LaunchAttempt`
2. perform preflight and external provider work outside long-held SQL locks
3. atomically finalize the Roger session/run/binding state only after provider
   verification succeeds

This is not a classic distributed two-phase commit protocol. It is a Roger
owned, crash-tolerant execution discipline that matches local SQLite plus
external CLI providers.

### D3. Keep `launch_preflight_plans` advisory and reusable

`launch_preflight_plans` stay useful for route selection, worktree checks, and
surface guidance. They are not enough to prove that a specific launch actually
happened.

`LaunchAttempt` is the execution-truth object. `launch_preflight_plan` is the
routing/preflight object.

### D4. Finalization is atomic per user-visible lifecycle action

Once provider verification exists, Roger should finalize the relevant durable
changes in one storage transaction for that user-visible action.

Examples:

- start review
- resume by locator
- resume by reseed
- automatic reconciliation into a new run when needed
- return-to-Roger rebind

### D5. Bridge actions dispatch through real `rr --robot`

The serious bridge path should shell out to the real Roger CLI in robot mode,
parse the returned schema, and only then return a bridge success response.

### D6. Recovery must be explicit, not magical

If Roger finds a pending or partially-completed attempt later, it should either:

- finish it using still-valid verification evidence, or
- mark it failed/abandoned truthfully and surface repair guidance

Roger should not silently invent success during startup or through broad
best-effort guessing.

---

## Proposed lifecycle model

## One attempt per action

Each launch-like command creates exactly one `LaunchAttempt`.

Examples:

- `rr review --provider opencode` creates one attempt with action
  `start_review`
- `rr resume --session X` creates one attempt with action `resume_review`
- `rr return --session X` creates one attempt with action `return_to_roger`

## Automatic reconciliation

Roger may perform bounded automatic reconciliation of stale review state when
it re-enters an existing session or observes drift through status and readback
surfaces. That reconciliation should not create a separate user-visible action
kind or a dedicated bridge request. It should update the same durable session
truth that `review`, `resume`, and `return` already maintain.

Retries create new attempt ids. Roger should not reuse one failed attempt row as
if it were a fresh try.

## State machine

Recommended `LaunchAttempt.state` values:

Non-terminal:

- `pending`
- `dispatching`
- `awaiting_provider_verification`
- `committing`

Terminal success:

- `verified_started`
- `verified_reopened`
- `verified_reseeded`

Terminal failure:

- `failed_preflight`
- `failed_spawn`
- `failed_provider_verification`
- `failed_session_binding`
- `failed_commit`
- `abandoned`

`abandoned` means Roger can prove an attempt existed but cannot honestly finish
or classify it as a clean provider failure anymore. This is a recovery state,
not a success state.

## State transition rules

### Start review

Typical happy path:

1. `pending`
2. `dispatching`
3. `awaiting_provider_verification`
4. `committing`
5. `verified_started`

### Resume by locator

Typical happy path:

1. `pending`
2. `dispatching`
3. `awaiting_provider_verification`
4. `committing`
5. `verified_reopened`

### Resume by reseed fallback

Typical degraded-but-truthful path:

1. `pending`
2. `dispatching`
3. `awaiting_provider_verification`
4. `committing`
5. `verified_reseeded`

### Failure examples

- preflight fails before spawn: `pending -> failed_preflight`
- provider binary spawn fails: `pending -> dispatching -> failed_spawn`
- provider emits stale or mismatched session evidence:
  `pending -> dispatching -> awaiting_provider_verification -> failed_provider_verification`
- provider session exists, but Roger cannot atomically bind session/run/state:
  `... -> committing -> failed_session_binding` or `failed_commit`

---

## Proposed data model

## New aggregate: `LaunchAttempt`

Suggested minimal relational fields:

- `id`
- `action_kind`
  - `start_review`
  - `resume_review`
  - `return_to_roger`
- `provider`
- `surface`
  - `cli`
  - `bridge`
  - `tui`
- `attempt_state`
- `repo_locator`
- `review_target_json` nullable
- `requested_session_id` nullable
- `launch_profile_id` nullable
- `launch_binding_id` nullable
- `preflight_plan_id` nullable
- `attempt_nonce`
- `provider_session_id` nullable
- `provider_locator_artifact_id` nullable
- `resume_bundle_artifact_id` nullable
- `verification_artifact_id` nullable
- `committed_review_session_id` nullable
- `committed_review_run_id` nullable
- `failure_reason_code` nullable
- `failure_detail_json` nullable
- `created_at`
- `updated_at`
- `committed_at` nullable
- `row_version`

## Why these fields exist

- `attempt_nonce` is the anti-stale-binding anchor. External hook or provider
  verification evidence must match it before Roger trusts the evidence.
- `requested_session_id` links resume/return attempts to an intended existing
  Roger session without implying success.
- `committed_review_session_id` and `committed_review_run_id` let doctor and
  recovery flows prove what, if anything, was finalized.
- `verification_artifact_id` keeps the proof object durable and inspectable.

## Append-only event history

Roger does not need a dedicated `launch_attempt_events` table in `0.1.x` if the
existing event machinery can carry launch-attempt transitions truthfully.

Recommended path:

- materialize current attempt state in `launch_attempts`
- write append-only `OutcomeEvent` rows for significant transitions such as:
  - attempt created
  - preflight failed
  - provider verification captured
  - final commit succeeded
  - final commit failed
  - attempt abandoned

This keeps the hot relational model small while preserving repair/audit detail.

## Relationship to existing tables

### `review_sessions`

- only durable successful session truth
- never a placeholder for an in-flight attempt

### `review_runs`

- created only during finalization
- one attempt may create zero or one new run depending on action

### `session_launch_bindings`

- still represent durable re-entry ownership and routing state
- written only during finalization, not as speculative launch placeholders

### `launch_preflight_plans`

- remain reusable routing/preflight results
- may be linked from an attempt, but do not replace it

---

## Verification contract

Roger should accept provider verification only when all of the following are
true:

1. provider name matches the attempt provider
2. provider session id is non-empty and syntactically valid for that adapter
3. verification evidence includes the expected `attempt_nonce` or a stronger
   equivalent Roger-owned correlation token
4. repo/worktree/cwd evidence matches the expected launch scope strongly enough
5. review target still matches or can be classified truthfully as reseed/degraded

If any of those fail, Roger must not finalize the attempt as success.

## Verification evidence shape

Different providers may prove launch differently:

- direct adapter return value
- hook artifact
- transcript metadata
- provider-local session-state file

Roger should normalize them into one verification envelope with at least:

- `provider`
- `provider_session_id`
- `attempt_nonce`
- `verification_source`
- `captured_at`
- `repo_or_worktree_context`
- `raw_evidence_artifact_id`
- provider-specific details bag

The normalized envelope is what finalization code should trust.

---

## Command execution model

## `rr review`

Recommended algorithm:

1. resolve repo and PR target
2. create a pending `LaunchAttempt`
3. run preflight checks
4. if preflight fails:
   - mark attempt `failed_preflight`
   - return blocked/degraded response with repair guidance
5. dispatch provider launch
6. capture verification envelope
7. finalize atomically:
   - persist artifacts
   - create `ReviewSession`
   - create `ReviewRun`
   - persist `ResumeBundle` reference
   - persist `session_launch_binding`
   - update attention and continuity state
   - mark attempt `verified_started`
8. return success with canonical Roger ids

## `rr resume`

Recommended algorithm:

1. resolve target session/binding
2. create pending `LaunchAttempt`
3. decide reopen versus reseed using existing continuity logic
4. dispatch the chosen provider path
5. verify provider evidence
6. finalize atomically:
   - create a new `ReviewRun` when appropriate
   - update continuity/attention state
   - reconcile stored locator or bundle refs
   - mark attempt `verified_reopened` or `verified_reseeded`

## `rr return`

`return` should also use a `LaunchAttempt`, even when the provider is already
known.

Why:

- return is still a cross-boundary rebinding action
- it can fail due to stale provider state, wrong cwd/worktree, or missing Roger
  session context
- it needs the same truthful recovery story as start and resume

---

## Finalization transaction boundary

Each successful lifecycle action should finalize in one store transaction.

The exact rows differ by action, but the transaction should cover the durable
Roger truth for that action.

### Start review finalization

Atomic unit should include:

- any newly stored verification/bundle artifacts
- `review_sessions` insert
- `review_runs` insert
- `session_launch_bindings` insert/update
- continuity and attention state initialization
- `launch_attempts` success transition
- append-only event rows

### Resume/return finalization

Atomic unit should include:

- verification artifacts
- updated session continuity/attention state
- any new run rows
- any refreshed launch binding or locator refs
- `launch_attempts` success transition
- append-only event rows

### Failure after provider verification but before commit

If provider verification exists but the final transaction fails:

- the attempt must end in `failed_session_binding` or `failed_commit`
- Roger must keep the verification artifact reference
- Roger must not leave behind a half-created durable session/run state that
  looks successful

---

## Recovery and reconciliation

## Recovery principle

Roger should recover from partial launch state by reconciling one attempt, not
by inferring success from scattered rows.

## Fresh pending attempt on startup or doctor

If a later Roger process finds a pending or in-progress attempt, it should
classify it using:

- age
- presence or absence of verification evidence
- whether final durable rows were already committed
- whether the evidence still matches the attempt nonce and target scope

Recommended outcomes:

- finalize now if verification is present and still valid
- mark `abandoned` if evidence is absent/stale/ambiguous
- mark explicit failure if a known failure class can be proven

## No silent arbitrary replay

Roger should not blindly re-run provider launch on startup just because an old
attempt is pending. Retry is an operator action or an explicit command retry
that creates a new attempt id.

---

## Bridge truth rules

## Bridge role

The bridge remains:

- daemonless
- launch-only / open-local
- non-mutating

It is **not** allowed to invent review success.

## Required bridge algorithm

For each supported action:

1. validate bridge request shape
2. run bridge preflight
3. if preflight fails, return blocked/failure with guidance
4. invoke the real `rr` command in `--robot` mode
5. parse the robot envelope
6. verify required ids/fields exist for the action
7. map the robot result into the bridge response

## Required bridge action semantics

### `start_review`

Requires:

- successful `rr review --robot ...`
- canonical `session_id`

### `resume_review`

Requires:

- successful `rr resume --robot ...`
- canonical `session_id`

### `show_findings`

This action should not claim session success unless it has enough information to
open findings for one real session truthfully.

Acceptable first-release behavior:

- return blocked/open-local guidance when session resolution is ambiguous
- return success only when the underlying `rr` command reports the session that
  findings belong to

## Bridge failure classes

Bridge responses should distinguish at least:

- preflight failure
- CLI process failed to start
- robot schema mismatch
- missing required Roger ids
- CLI returned blocked/degraded outcome

The bridge should prefer blocked/failure over vague success with empty ids.

---

## Validation plan

This lane should be defended mostly by integration tests, not heavyweight E2E.

## Required test families

### Storage + migration

- launch-attempt table migration and reopen
- terminal success/failure state persistence
- attempt-to-session/run linkage integrity

### CLI lifecycle truth

- `rr review` does not create durable session rows before verification succeeds
- `rr resume` and `rr return` are transactional and do not expose stale
  continuity as committed success
- crash/fault injection between provider verification and final commit does not
  leave a fake successful session
- retry after partial failure creates a new attempt id and does not reuse stale
  evidence
- automatic reconciliation after re-entry or state drift updates review truth
  without requiring a separate manual refresh step

### Provider verification safety

- stale verification artifact from earlier attempt nonce is rejected
- worktree mismatch fails closed
- missing provider session id fails verification

### Bridge truth

- bridge invokes real `rr --robot`
- bridge rejects robot payloads that omit required ids
- bridge returns failure when the CLI exits non-zero or returns blocked

### Recovery

- pending attempt with valid verification can be finalized once
- pending attempt with stale/ambiguous evidence is marked `abandoned`
- no duplicate session/run creation during recovery

## Explicitly not required here

- a second heavyweight automated E2E
- full browser UX validation
- provider-matrix-wide real smoke on every PR

---

## Suggested implementation slices

### Slice 1. Contract and migration design

- add this plan
- decide exact `launch_attempts` schema
- define normalized verification envelope shape

### Slice 2. Storage support

- add `launch_attempts` migration
- add storage APIs for create/update/finalize/query
- add storage smoke for success/failure/abandon paths

### Slice 3. CLI lifecycle retrofit

- rework `rr review`
- rework `rr resume`
- rework `rr return`
- add automatic reconciliation into status/readback paths
- add fault-injection tests around verification and final commit

### Slice 4. Bridge retrofit

- replace stub bridge success with real `rr --robot` dispatch
- enforce action-specific required Roger ids
- add bridge truth tests

### Slice 5. Recovery hook-in

- add explicit pending-attempt reconciliation entrypoints for later doctor or
  startup use
- classify stale versus recoverable attempts truthfully

---

## What should merge back into the canonical plan

If this direction is accepted, the following should migrate out of this file and
into canonical truth:

- the `LaunchAttempt` aggregate
- the two-phase lifecycle rule
- the bridge dispatch-through-real-CLI rule
- the finalization transaction boundary
- the recovery/reconciliation rule set

The narrow storage and verification details can then live in
implementation-facing support contracts.

---

## Bottom line

Roger does not need a daemon or a giant cross-process transaction to achieve
launch truth.

It needs one missing aggregate and one disciplined execution model:

- durable `LaunchAttempt` rows before provider launch
- verification evidence before success
- atomic finalization of Roger truth after verification
- real CLI-backed bridge responses instead of pretend success

That is the hardest planning gap in the current provider-truth program, and it
should be settled before Copilot admission or broader provider claims move
forward.
