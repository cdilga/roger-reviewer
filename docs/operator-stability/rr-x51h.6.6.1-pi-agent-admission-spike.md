# rr-x51h.6.6.1 Pi-Agent Admission Spike

Date: 2026-04-19
Bead: `rr-x51h.6.6.1`
Reference target: `_exploration/pi_agent_rust`

## Decision Summary

Pi-Agent remains out of the live Roger support matrix.

Outcome for this spike: **deferred Tier A candidate only**. No CLI exposure,
README widening, or live provider claim changes are justified from this review.

## Snapshot Against Roger Admission Rubric

### Observed strengths (from local exploration materials)

- Single-binary CLI posture with explicit interactive/print/RPC modes.
- Session durability model (JSONL primary with optional SQLite index).
- Capability-gated extension/runtime safety concepts and policy controls.
- Large provider/onboarding surface in the reference project, indicating broad
  adapter experimentation potential.

### Critical gaps versus Roger admission requirements

- No demonstrated Roger `SessionLocator`/`ResumeBundle` contract mapping.
- No demonstrated Roger launch-attempt ledger semantics for
  `pending/verified/*/failed` lifecycle truth.
- No proof that Pi-Agent can be constrained under Roger policy profiles
  equivalent to `review_readonly` with fail-closed enforcement.
- No proof of Roger-owned audit artifact classes and posting-boundary control
  through a Pi-Agent adapter path.
- No proof of Tier B continuity parity (locator reopen + truthful `rr return`
  equivalent) under Roger authority.

## Conservative First Slice If Reopened Later

If this candidate is reopened for implementation planning, the maximum safe
starting point is:

- **Tier A only**
- start + reseed + raw-capture path under Roger-owned lifecycle truth
- explicit fail-closed behavior for reopen/return/dropout claims
- no implied Tier B/Tier C support

## Required Proof Before Any Implementation Beads

1. Deterministic direct-CLI launch adapter design that maps Pi-Agent sessions
   into Roger launch-attempt and session-binding contracts.
2. Policy envelope showing Roger-enforced safety/tool posture with explicit
   denied capabilities and fail-closed responses.
3. Audit mapping for launch/provider artifacts + degraded/failure taxonomy.
4. Truthful doctor/status/help/robot surface proposal for a non-live candidate.
5. Validation plan scoped to deterministic doubles first, with bounded smoke
   requirements only if support wording widens.

## Recommendation

Keep Pi-Agent as a deferred future-harness candidate and do not create
implementation beads until the proof packet above is assembled.

This preserves Roger's current local-first safety and truth posture while
keeping Pi-Agent as a structured future option rather than an implied roadmap
commitment.

## Non-Widening Statement

No live support claim changes in this spike. Pi-Agent remains outside `0.1.0`
live support.
