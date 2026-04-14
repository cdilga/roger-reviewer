# Memory Promotion And Scope Recall Evolution

Status: merged into canonical/support contracts on 2026-04-14
Class: historical side-plan / merged memory brief
Audience: maintainers shaping Roger memory, recall, and usefulness-learning beads

Authority:

- [`PLAN_FOR_ROGER_REVIEWER.md`](./PLAN_FOR_ROGER_REVIEWER.md)
- [`SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md`](./SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md)
- [`adr/004-scope-and-memory-promotion-policy.md`](./adr/004-scope-and-memory-promotion-policy.md)

This document does not replace those authorities. It extends them into a more
explicit “right memory at the right time” design direction.

---

## Premise

Roger should learn over time without becoming ambient global memory.

The design rule is:

**Roger learns locally, proves narrowly, and exports only by explicit binding.**

That is the right answer to the developer-learning problem. It solves repeated
re-teaching without collapsing all memory into one cross-repo blob.

---

## Core stance

Memory should be treated as a gated surface, not a ranked blob.

What surfaces in-session should depend on:

- locality
- anchor overlap
- trust state
- freshness
- contradiction status
- explicit overlay enablement

What should not happen:

- weak repo results silently widening into project/org memory
- candidate memory silently acting like promoted memory
- old lessons surviving major repo or policy changes just because they still
  rank well

---

## Durable retrieval lanes

Roger already has the right lane model. It should be strengthened, not replaced.

### `evidence_hits`

Use for:

- findings
- session summaries
- user notes
- policy excerpts
- ADR sections
- episodic history

Behavior:

- searchable immediately
- always carries provenance
- may inform the agent, but does not behave like durable approved memory

### `tentative_candidates`

Use for:

- extracted facts and heuristics not yet proven
- early procedural lessons
- tentative cross-run patterns

Behavior:

- only surface with high anchor overlap or explicit request
- never silently injected as stable guidance
- demote quickly on contradiction or harm

### `promoted_memory`

Use for:

- `established` and `proven` semantic or procedural memory
- allowlisted canonical policy imports

Behavior:

- eligible for ordinary retrieval and prompt injection
- must still preserve scope and provenance
- must still be subject to invalidation when anchors or policy epochs move

---

## Locality model

### `repo`

Default memory home for ordinary Roger work.

What belongs here:

- repo-specific practices
- file/symbol/test heuristics
- findings and fixes that are only trustworthy inside one codebase

### `project`

Explicit overlay across an allowlisted set of repos.

What belongs here:

- shared conventions across a real project family
- repeated lessons that are useful across those bound repos

Rules:

- never ambient
- never automatic fallback from weak repo search
- must remain visibly labeled as overlay material

### `org`

Explicit policy overlay only.

What belongs here:

- approved policy guidance
- stable organizational review constraints
- canonical standards that outrank local heuristic noise

Rules:

- do not treat this as an ambient company memory bucket
- do not let generic docs auto-promote here

### Future “memorywide” rule

If Roger later introduces a broader “memorywide” concept, it should still be a
named, bound overlay with explicit enablement, not a silent default fallback.

---

## Promotion and demotion posture

The current policy is directionally right. What is missing is a clearer recall
contract.

### Promotion

- `observed -> candidate` after extraction yields a structured fact, heuristic,
  or procedure with evidence
- `candidate -> established` only after repeated helpful use, explicit human
  promotion, or conservative canonical import
- `established -> proven` only after repeated successful approved use, merged
  evidence, or bound canonical policy authority

### Demotion

- contradiction and harmful outcomes should demote faster than helpful outcomes
  promote
- `candidate` should drop fast
- `established` should lose ordinary retrieval eligibility after strong harm
- `proven` should fall to `deprecated` when contradicted by newer policy or
  repeated bad outcomes
- `anti_pattern` should remain searchable only as warning material

---

## When should memory surface?

Recall should be event-driven and change-aware.

### Surface on

- active anchors match the memory’s anchor set closely
- current repo epoch still matches
- current policy version has not invalidated the memory
- allowed overlay scopes are enabled
- the agent’s current task actually benefits from recall rather than raw
  evidence browsing

### Do not surface on ordinary retrieval when

- scope is broader than the session allows
- the item is still only a `candidate` with weak anchor overlap
- the memory was contradicted, marked harmful, or tied to invalidated anchors
- semantic verification is unavailable and the hit depends on unverified
  semantic-only evidence

### Force reevaluation on

- new commit, rebase, or merge-base change
- repo policy or ADR change
- scope binding change
- semantic/lexical generation change
- contradiction or harmful outcome
- major dependency or platform epoch changes

---

## What QMD helps with

QMD is useful here for mechanics, not memory authority.

Useful ideas:

- retrieval contexts and default-inclusion posture for well-bounded collections
- query typing and hybrid retrieval
- explainable hit provenance
- graceful degradation to lexical-only behavior

Not useful as Roger authority:

- collection-centric inheritance as the main memory model
- ambient context bleed from generic default inclusion
- flattening documents and memory into one retrieval universe

---

## The missing recall contract

Roger should add an explicit recall contract that answers:

1. why did this memory surface now
2. what lane did it come from
3. what scope owns it
4. what invalidation checks still pass
5. whether the agent may cite it, rely on it, or only inspect it cautiously

Without that contract, “learning over time” turns into opaque ranking behavior,
which is exactly the wrong failure mode.

---

## Recommended staged implementation

1. Keep the existing scope and promotion policy as the authority model.
2. Add a machine-readable recall envelope for every surfaced item.
3. Build event-driven invalidation and freshness checks into retrieval.
4. Record usefulness and harmfulness as first-class feedback that changes recall
   eligibility.
5. Only widen memory beyond `repo` through explicit overlays and explicit
   operator or launch binding.

---

## Result

The right long-term behavior is not “Roger remembers more.”

It is:

- Roger remembers the right things
- in the right scope
- at the right time
- with the right trust level
- and with a clear path to stop remembering harmful lessons
