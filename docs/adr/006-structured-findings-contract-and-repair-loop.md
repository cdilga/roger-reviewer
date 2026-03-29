# ADR 006: Structured Findings Contract and Repair Loop

- Status: accepted
- Date: 2026-03-29

## Context

Roger's TUI and extension both depend on findings being navigable as structured
objects, not just as raw chat output. The current plan already requires
structured findings, but it does not yet define the operational contract for
when an LLM review task:

- emits no findings pack at all
- emits a malformed or partially valid pack
- produces valid findings mixed with invalid anchors or invalid fields
- returns raw text that is still useful even when structure extraction fails

Without a tighter contract, implementation will oscillate between brittle
all-or-nothing parsing and vague manual recovery.

Roger also needs a more precise answer for code-backed findings. A finding may
need to carry one or more repo code locations so the TUI can inspect the exact
evidence and Roger can later open that evidence set in a local editor such as
VS Code without inventing a second ad hoc format.

There is enough prior art to justify a stronger stance:

- schema-constrained structured output is now a first-class capability in modern
  provider APIs
- TOON provides a compact, schema-aware model-facing format for structured
  packets when the model/backend supports it well
- retrieval and prompt-chaining guidance from major providers reinforces the
  value of smaller, bounded repair steps rather than repeating a whole large job

## Decision

Roger should treat each review-stage result as two parallel artifacts:

- raw model output
- a structured findings pack in a Roger-approved schema

Recommended contract:

- prefer provider-native structured output or strict tool/schema modes where the
  harness supports them
- allow TOON or compact JSON as the model-facing findings pack format depending
  on model/backend support and smoke-test results
- normalize accepted structured findings into Roger-owned rows and linked
  artifacts immediately after validation
- let each finding carry zero or more normalized code-evidence locations with
  repo-relative path, line/column range when available, excerpt or excerpt
  artifact, and evidence role such as `primary`, `supporting`, or
  `contradicting`
- never make raw-output preservation conditional on successful structure parsing

Recommended validation and repair behavior:

- validate incrementally, not all-or-nothing
- salvage any finding or artifact reference that is fully valid
- mark the stage result as `structured`, `partial`, `raw_only`, `repair_needed`,
  or `failed`
- classify repairable failure types explicitly, including missing pack,
  malformed syntax, schema drift, invalid field values, invalid anchors, and
  contradictory state transitions
- salvage the rest of a finding when one code-evidence location is invalid or
  stale; invalid anchors should degrade that location explicitly rather than
  forcing the whole finding to disappear
- send concise machine-readable repair feedback back to the LLM when repair is
  likely to succeed
- use a bounded retry budget and an idempotent repair pipeline rather than
  unbounded reruns
- keep every repair attempt linked to the original raw output and prompt stage

UI consequences:

- the TUI should primarily navigate normalized structured findings
- the TUI should show attached code-evidence locations as first-class finding
  detail, not bury them in free-form markdown only
- Roger may derive a thin local editor-open action from those normalized code
  locations, but the editor handoff is downstream of Roger's own finding model
- the TUI must also expose `view raw output`, `view original pack`, and `retry
  structure repair`
- the extension may surface bounded states such as `findings ready`,
  `partial findings`, or `repair needed`, but should hand detailed recovery back
  to the local TUI

## Consequences

- Roger gains a truthful degraded mode instead of pretending malformed output is
  either perfect or useless
- valid findings survive mixed-quality model output
- retry logic becomes an explicit, testable subsystem rather than prompt magic
- raw-output inspection remains a durable audit and recovery path

## Open Questions

- what exact Roger findings schema should be the first stable version?
- what retry budget is acceptable per stage before surfacing `repair needed` to
  the user?
- when should Roger switch from repair to a fresh rerun?
- should Roger allow a secondary parser/repair model or keep repair inside the
  same stage provider by default?

## Follow-up

- define the first `StructuredFindingsPack` schema and validator
- define the initial `CodeEvidenceLocation` shape and validation rules inside
  that pack
- define the stage-result state machine and error taxonomy
- add smoke tests for TOON and compact-JSON findings packs on supported models
- add integration scenarios for `partial`, `raw_only`, and `repair_needed`
  review states
