# Roger Harness Session Linkage Contract

This document is the implementation-facing contract for `rr-015`. It consolidates
the architectural decisions from ADR-002 and ADR-007 into a stable, adapter-
implementable spec for the Roger-to-harness session boundary.

Authority:

- `AGENTS.md` and `docs/PLAN_FOR_ROGER_REVIEWER.md` are the canonical source
- `docs/adr/002-harness-and-session-durability-contract.md` is the parent ADR
- `docs/adr/007-harness-native-roger-command-surface.md` covers in-harness commands
- This document narrows those decisions into implementable rules without
  superseding them

---

## Core Principle

Roger owns the review. The harness is an adapter. Roger's session ledger is the
source of truth, not the harness session transcript.

---

## Durability Layers

### `SessionLocator`

Harness-specific reopen information. Contains only what the harness adapter
needs to attempt a reopen.

Required fields:

- `provider` — harness identifier, e.g. `opencode`, `gemini`
- `session_id` — harness-specific session identifier
- `invocation_context` — bounded JSON bag for provider-specific launch params
- `captured_at` — timestamp when the locator was written
- `last_tested_at` nullable — when Roger last confirmed this locator was usable

`SessionLocator` is always harness-specific. Its fields may differ across
providers. Adapter code reads it; domain code must never parse provider-specific
fields directly.

### `ResumeBundle`

Harness-neutral Roger continuity packet. Its job is to let Roger continue the
review coherently when the original harness session is gone, compacted, or
no longer useful.

`ResumeBundle` is **not**:

- a full transcript clone
- a lossless cross-agent session mirror
- a provider-internal serialized state blob

Required payload classes:

- **Roger schema and bundle version**: for forward/backward repair
- **Review target identity**: repo, PR, base/head commits, cwd/worktree context
- **Launch intent**: requested action, preset/objective, preferred local surface
- **Roger control context**: review mode, safety posture, active Roger
  skills/instructions
- **Harness provenance summary**: provider, locator reference, last known run
  state, last known continuity quality
- **Stage continuity**: current prompt stage, stage summaries, pending
  follow-up questions
- **Surviving findings**: finding ids, normalized summaries, states, evidence
  anchors, refresh lineage references
- **Outbound continuity**: draft ids, approval state, posted-action linkage
- **Compact attention summary**: unresolved decisions and immediate follow-ups
- **Selected artifact references**: digests and small inline excerpts only when
  essential; large material stays in cold artifacts

Size constraint: the serialized bundle must normally fit within a
prompt-sized payload (suggested budget: ≤ 16KB inline; overflow to cold
artifacts with digest references). Inline excerpts are bounded and selective,
not wholesale transcript copies.

### Two delivery profiles over the same bundle schema

- `dropout_control`: compact handoff for intentional dropout to a plain harness
  session — Roger's current state, safety posture, and brief context summary
- `reseed_resume`: full reseed payload for fresh harness session after
  compaction, failure, or degraded continuity

Fields are a strict superset relationship: `reseed_resume` includes everything
in `dropout_control` plus stage/finding/artifact depth.

---

## Capability Tiers

Every harness adapter declares a capability tier. Roger claims only the level of
support the tier earns.

### Tier A: Bounded Supported Harness

Minimum required for any Roger support claim:

- `start_session` — can create or link to a Roger-owned review session
- `seed_from_resume_bundle` — can ingest a `ResumeBundle` into a fresh session
- `capture_raw_output` — can durably capture raw stage output
- `normalize_or_repair_findings_from_output` — raw output feeds Roger's
  structured-findings path or repair loop
- `bind_review_target` — session is explicitly bound to repo/PR/target
- `report_continuity_quality` — can report `usable`, `degraded`, or `unusable`

Support claims allowed at Tier A:

- Roger may claim **bounded support**: the harness can start, run, and capture
  a review with Roger's approval and audit model in place

Claims NOT allowed at Tier A:

- direct resume from a prior session
- bare-harness dropout
- in-harness Roger commands

### Tier B: Continuity-Capable Harness

Tier A plus:

- `reopen_by_locator` — can reopen an existing session by `SessionLocator`
- `open_in_bare_harness_mode` — can expose the underlying session directly
- `return_to_roger_session` — has a supported return path back to Roger

Support claims allowed at Tier B (in addition to Tier A):

- Roger may claim **direct-resume support**: can reopen by locator and fall
  back to reseed when the locator is stale
- Roger may claim **dropout and return support**: user can leave Roger into the
  bare harness and return explicitly

Claims NOT allowed at Tier B without Tier C:

- in-harness Roger command dispatch

### Tier C: Ergonomic Harness

Tier B plus:

- `supports_roger_commands`
- `describe_roger_command_bindings`
- `invoke_roger_command`
- `attach_artifact_reference` when useful

Support claims allowed at Tier C:

- Roger may claim **in-harness command support** for the affordances it has
  actually shipped and smoke-tested

---

## `0.1.0` Provider Targets

| Capability | OpenCode 0.1.0 | Gemini 0.1.0 | Future-provider rule |
|------------|---------------|--------------|----------------------|
| `start_session` | Required | Required | Required for any support |
| `seed_from_resume_bundle` | Required | Required | Required for any support |
| `capture_raw_output` | Required | Required | Required for any support |
| `normalize_or_repair_findings_from_output` | Required | Required | Required for any support |
| `bind_review_target` | Required | Required | Required for any support |
| `report_continuity_quality` | Required | Required | Required for any support |
| `reopen_by_locator` | Required | Optional | Required for resume claims |
| `open_in_bare_harness_mode` | Required | Optional | Required for dropout claims |
| `return_to_roger_session` | Required | Optional | Required for dropout claims |
| `supports_roger_commands` | Optional | Not required | Optional ergonomic layer |

**OpenCode** should reach Tier B and may expose a safe Tier C subset
(`roger-help`, `roger-status`, `roger-findings`, `roger-return`).

**Gemini** only needs Tier A in `0.1.0`. Roger must not claim reopen, dropout,
or in-harness command support for Gemini until it satisfies the corresponding
tier requirements and has passed provider acceptance tests.

---

## What Roger May Not Claim (Per Provider)

### OpenCode `0.1.0`

May claim:

- start, run, capture, and durably normalize findings
- direct session reopen when locator is valid
- bare-harness dropout with Roger control bundle
- explicit return via `rr return` or equivalent
- optional: `roger-help`, `roger-status`, `roger-findings`, `roger-return`
  if smoke-tested on the supported OpenCode version

May NOT claim:

- lossless cross-session continuity when a session has been compacted
- full transcript migration to another harness
- in-harness commands that include approval or GitHub-posting capability

### Gemini `0.1.0`

May claim:

- start a Roger-owned review session
- ingest a `ResumeBundle` for reseed
- capture raw stage output
- feed Roger's findings normalization or repair path
- bind to an explicit review target
- report continuity quality (truthfully, even when quality is degraded)

May NOT claim:

- session reopen by locator
- bare-harness dropout
- in-harness Roger commands
- continuity quality better than `degraded` unless reopen actually succeeds

---

## Resume Algorithm

```
1. load latest SessionLocator, ResumeBundle, and ReviewRunState
2. ask the adapter to reopen the original harness session
3. if reopen succeeds:
   a. check effective review target still matches
   b. adapter reports continuity quality
   c. if quality == usable: continue in the original session
   d. if quality == degraded or unusable: fall through to step 4
4. start a fresh harness session
5. seed from the latest ResumeBundle (reseed_resume profile)
6. attach additional artifacts only if the adapter or stage requires them
7. record a new SessionLocator and update ReviewRunState
```

Roger must bias toward false negatives: uncertain continuity becomes a clean
reseed, not a confusing partial-resume story.

---

## Intentional Dropout

Deliberate dropout to the bare harness is first-class, not emergency-only.

Rules:

- Roger writes the `ResumeBundle` in `dropout_control` profile before exposing
  the bare session
- the control bundle is the same durability object as `ResumeBundle`, not an
  ad hoc handoff
- if the harness supports Roger commands, Roger must advertise available
  commands from within that session context
- Roger must provide an explicit return path: `rr return` or equivalent
- if Roger launched the bare-harness phase as an owned subprocess, automatic
  return on harness exit is allowed as a convenience but must NOT be the only
  return path
- bare-harness sessions must not allow approval or GitHub-posting flows to
  execute inside the harness command surface; those flows stay in the TUI or
  canonical `rr` approval path

---

## Continuity Quality Rules

Roger uses exactly three continuity quality outcomes:

| Outcome | Meaning |
|---------|---------|
| `usable` | Roger can continue in the original harness session without lying about the review target, run binding, or operator control context |
| `degraded` | Roger can continue truthfully only by reseeding from `ResumeBundle`, or reopen succeeded but does not meet Roger's confidence bar |
| `unusable` | Provider cannot reopen and Roger cannot reseed truthfully enough to continue the review |

Roger should keep the original session only when ALL of the following are true:

- locator reopen succeeded
- effective review target still matches
- adapter reports `usable`

If any condition fails, Roger starts fresh from the latest `ResumeBundle`.

---

## Mandatory Smoke Scenarios

These scenarios must be implemented and pass before any adapter can be declared
supported. They are the minimum validation baseline for rr-003.1 (OpenCode) and
rr-003.2 (Gemini).

### Smoke 1: Locator Reopen (OpenCode)

1. Start a review session, capture `SessionLocator`
2. Close and reopen Roger
3. Roger uses `SessionLocator` to reopen the original OpenCode session
4. Continuity quality is reported as `usable`
5. Review continues without reseeding

**Pass condition:** Roger resumes in the original session with target identity
preserved and continuity quality `usable`.

### Smoke 2: Stale-Locator ResumeBundle Reseed (OpenCode)

1. Start a review session with findings, capture `ResumeBundle`
2. Make the `SessionLocator` invalid (delete session or simulate expiry)
3. Attempt resume
4. Roger detects locator failure, starts a fresh session, seeds from `ResumeBundle`
5. Findings and review context are available in the new session

**Pass condition:** Roger reseeds cleanly, findings survive, no silent data loss,
no false `usable` claim.

### Smoke 3: Plain OpenCode Dropout

1. Start a review session in Roger
2. User triggers bare-harness dropout
3. Roger writes `ResumeBundle` in `dropout_control` profile
4. Plain OpenCode session is exposed with Roger control bundle injected
5. User can see review context in the bare session

**Pass condition:** Bare session contains Roger control bundle; review target
and safety posture visible; no approval or GitHub-posting paths reachable.

### Smoke 4: `rr return` Rebind

1. Complete Smoke 3 (user is in bare OpenCode session)
2. User runs `rr return` (or equivalent)
3. Roger reattaches the bare session to the original `ReviewSession`
4. TUI or CLI reflects the resumed state correctly

**Pass condition:** Roger rebinds correctly; return path is explicit, not
ambient; auto-return on harness exit is optional not required.

### Smoke 5: Bounded Gemini Reseed (Gemini)

1. Start a Roger review session using Gemini harness
2. Session is bound to a review target
3. Roger writes a `ResumeBundle` at a stage boundary
4. Simulate continuity loss (provider session gone)
5. Roger reseeds from `ResumeBundle` in a fresh Gemini session

**Pass condition:** Gemini adapter reports continuity quality honestly; Roger
reseeds cleanly; no claim of `reopen_by_locator` capability; no OpenCode-parity
claims made.

---

## Support Claim Enforcement Rules

1. An adapter must not report a capability it has not implemented
2. An adapter must not claim continuity quality `usable` unless all four
   `usable` conditions above are met
3. In-harness commands must never expose approval or GitHub-posting flows
4. Roger domain code must never call provider-specific session internals
   directly; all harness access goes through the `HarnessAdapter` boundary
5. Gemini-specific limitations must be visible to the user at runtime, not
   silently papered over with OpenCode-equivalent UX

---

## Follow-up Beads

- `rr-003.1`: Implement OpenCode primary adapter satisfying Tier B + smoke 1-4
- `rr-003.2`: Implement Gemini bounded adapter satisfying Tier A + smoke 5
- `rr-003.3`: Implement session persistence and resume ledger
- `rr-006.2`: Define TUI/app-core supervisor policy (can now proceed; depends
  on this contract)
