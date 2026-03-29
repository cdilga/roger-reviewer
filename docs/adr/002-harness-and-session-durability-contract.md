# ADR 002: Harness and Session Durability Contract

- Status: accepted
- Date: 2026-03-29

## Context

Roger must wrap OpenCode without making fallback fake. The plan already requires
session linkage, resume, and compaction recovery, but it does not yet define the
exact persisted contract well enough to implement safely.

Implementation will churn unless Roger owns:

- a stable harness boundary
- a Roger-owned durable session ledger
- explicit run/resume state transitions

## Decision

Roger should own a `HarnessAdapter` boundary and a Roger-owned session ledger.

Recommended minimum durable objects:

- `ReviewSession`
- `ReviewRun`
- `ReviewRunState`
- `SessionLocator`
- `ResumeBundle`
- `HarnessCapabilitySet`

Recommended run states:

- `created`
- `running`
- `awaiting_user`
- `interrupted`
- `completed`
- `failed`
- `cancelled`

Recommended contract:

- the harness adapter creates or reattaches to an underlying provider session
- Roger records a `SessionLocator` with enough data to attempt reopen
- Roger writes a `ResumeBundle` at durable stage boundaries
- resume first attempts reopen of the original provider session
- if reopen fails, Roger starts a fresh provider session and reseeds it from the
  latest `ResumeBundle`
- Roger domain logic never depends directly on undocumented provider internals
- the first OpenCode adapter should be a hybrid of live CLI/session control plus
  Roger-owned ledger/artifact state, rather than deep provider internals or a
  transcript-only file contract

### SessionLocator versus ResumeBundle

Roger should distinguish three different durability layers:

- `SessionLocator`: harness-specific reopen information
- `ResumeBundle`: harness-neutral Roger continuity packet
- cold artifacts: larger transcripts, tool traces, raw prompts, and reference
  payloads kept for audit or selective reinsertion

Example:

- `SessionLocator` for OpenCode might contain enough information to run
  `opencode -s ses_2c7bb37a6ffeAHdNGa5rM0hb3N`
- if that works and the adapter reports the session is still usable, Roger
  should resume there first
- if it fails, or if the old session no longer carries enough useful context,
  Roger starts a fresh provider session and seeds it from the latest
  `ResumeBundle`

### What ResumeBundle is

`ResumeBundle` should be the minimum Roger-owned continuation packet required to
resume the review coherently in the same or another supported harness.

It is **not** a full transcript clone and **not** a lossless cross-agent
transliteration of the entire underlying session.

Its job is:

- restore Roger continuity when harness continuity is unavailable or weak
- carry enough state to continue the review, not to perfectly recreate the old
  conversation
- let Roger reseed a fresh session through one stable contract even if provider
  internals differ

For deliberate bare-harness dropout, the operational projection of the same
bundle is the Roger control bundle. This is not a second durability object. It
is the same Roger continuity packet rendered for immediate handoff into a plain
harness session.

### ResumeBundle delivery profiles

Roger should support two delivery profiles over the same logical schema:

- `dropout_control`: the compact handoff used when Roger intentionally opens or
  resumes a plain harness session for the user
- `reseed_resume`: the durable reseed payload used when Roger must continue in a
  fresh harness session after compaction, failure, or degraded continuity

The fields should stay aligned. The difference is mainly how much supporting
excerpt material Roger chooses to inline for the moment.

### What ResumeBundle should contain

Recommended payload classes:

- Roger schema version and bundle version
- review target identity: repo, PR, base/head commits, cwd/worktree context
- launch intent: requested action, preset/objective, preferred local surface
- Roger control context: review mode, safety posture, and any loaded
  Roger-specific skills/instructions needed to keep the session on-task
- harness provenance summary: provider, original locator reference, last known
  run state, and whether reopen was last known to work
- stage continuity: current prompt stage, stage summaries, and pending
  follow-up questions
- surviving findings: finding ids, normalized summaries, states, evidence
  anchors, and refresh lineage references
- outbound continuity: draft ids, approval state, and posted-action linkage
- compact attention summary: what still needs a decision right now
- selected artifact references plus small inline excerpts only when they are
  essential to resume quality

### What ResumeBundle should not contain

- full raw transcripts by default
- provider-internal serialized session state
- large diff blobs or arbitrarily long prompt logs
- enough material to pretend Roger has losslessly recreated the entire original
  harness session

Large or verbose material should remain in cold artifacts and be linked from the
bundle by digest or artifact id.

### Size and shape constraints

The bundle should stay intentionally compact.

Recommended constraints:

- serialized bundle should normally fit within a small prompt-sized payload
  rather than a transcript archive
- inline excerpts should be bounded and selected, not wholesale transcript
  copies
- larger supporting material should be reinjected by explicit artifact
  selection, not automatically bundled

### Recovery algorithm

Recommended resume flow:

1. load the latest `SessionLocator`, `ResumeBundle`, and run state
2. ask the provider adapter to reopen the original session
3. if reopen succeeds and continuity is still good enough, continue in that
   session
4. if reopen fails, or the adapter marks continuity as degraded, start a fresh
   provider session
5. seed that new session from the `ResumeBundle`
6. attach additional artifacts only if the adapter or stage needs them

### Continuity threshold

Roger should reuse the original harness session only when all of the following
are true:

- `SessionLocator` reopen succeeds
- the reopened session still points at the same effective review target
- the adapter reports continuity quality as `usable` rather than `degraded`
- the user has not explicitly requested a fresh session

If any of those fail, Roger should prefer starting fresh from the latest
`ResumeBundle` rather than gambling on a half-broken old session. Roger should
bias toward false negatives here: uncertain continuity should become a clean
reseed, not a confusing partial-resume story.

### Cross-harness implication

`ResumeBundle` should be strong enough to seed another Roger-supported harness,
but only at the Roger review layer.

That means:

- Roger can carry forward the review target, stage progress, findings, drafts,
  and selected evidence
- Roger does **not** promise full transcript-isomorphic migration across
  harnesses
- richer cross-agent resume can be explored later as a higher-cost feature, but
  it should not be the v1 durability requirement

### Harness capabilities

The adapter capability set should distinguish at least:

- `reopen_by_locator`
- `seed_from_resume_bundle`
- `attach_artifact_reference`
- `focus_review_target`
- `report_continuity_quality`
- `open_in_bare_harness_mode`
- `supports_roger_commands`
- `describe_roger_command_bindings`
- `invoke_roger_command`
- `return_to_roger_session`

### Capability tiers

Roger should classify harnesses by capability tier rather than by provider
brand.

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

`0.1.0` provider target:

- OpenCode should reach Tier B and may expose selected Tier C affordances
- Gemini only needs Tier A in `0.1.0`
- future providers should be admitted against the same tier table rather than
  bespoke provider branches

### Support claim rule

Roger should only claim the level of support that the harness capability tier
actually earns.

- Roger may claim **bounded support** only when a harness satisfies Tier A
- Roger may claim **direct-resume or dropout support** only when a harness
  satisfies Tier B
- Roger may claim **in-harness Roger command support** only when a harness
  satisfies the relevant Tier C affordances
- unsupported capabilities must fail clearly and route the user back to the
  canonical `rr` or TUI path rather than pretending parity

### Continuity quality outcomes

Roger should use only three continuity outcomes:

- `usable`
- `degraded`
- `unusable`

Rules:

- `usable` means Roger can continue in the original provider session without
  lying about target, run binding, or operator control context
- `degraded` means Roger can continue truthfully only by reseeding from
  `ResumeBundle`, or reopen succeeded but does not meet Roger's confidence bar
- `unusable` means Roger cannot reopen and cannot reseed truthfully enough to
  continue

### Intentional dropout to the bare harness

Roger should support deliberate dropout to the underlying harness, not only
emergency fallback after failure.

That means:

- a user may leave the Roger shell temporarily to inspect or question the code
  in plain OpenCode
- Roger should still provide a compact control bundle so that session remains
  tied to the same review target, safety posture, and relevant skills or
  instructions
- if the harness supports commands, Roger should advertise the available
  Roger-native in-harness commands from the same session context rather than
  forcing the user to rediscover them manually
- Roger should provide an explicit return affordance from that bare-harness
  session, such as a lightweight `rr return` command or equivalent helper
- returning from that bare-harness phase should preserve continuity back into
  Roger rather than treating it as an unrelated fresh session
- if Roger launched the bare-harness phase as a child/owned process, automatic
  return to the Roger TUI on harness exit is allowed as a convenience behavior,
  but it must not be the only supported return path

This path is first-class and necessary, not an emergency-only escape hatch.

## Consequences

- OpenCode becomes the first adapter, not the architecture center
- interrupted runs and compacted sessions become first-class cases
- durable state is defined by Roger rather than by chat transcript survival
- Roger continuity and harness continuity become separate concepts
- deliberate dropout to plain OpenCode becomes part of the supported operator
  workflow and validation matrix
- future cross-harness support can reuse the same Roger-level continuity packet
  without requiring transcript-level migration guarantees

## Follow-up

- write the schema sketch for the session ledger
- define the `ResumeBundle` schema and size budget
- define the harness capability discovery shape
- define the Roger command-binding contract and the canonical command/result
  objects for supported harnesses
