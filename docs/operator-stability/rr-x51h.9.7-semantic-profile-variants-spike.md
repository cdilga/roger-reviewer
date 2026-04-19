# rr-x51h.9.7 Semantic Profile Variants Spike

Date: 2026-04-19
Bead: `rr-x51h.9.7`

## Decision Summary

Roger keeps `semantic-default` as the only supported semantic profile baseline
for `0.1.x`.

Code-oriented, sparse, and rerank variants remain deferred until they satisfy an
explicit value-versus-cost gate. This spike defines that gate and does not
widen current support claims.

## Inputs Reviewed

- `docs/PLAN_FOR_ROGER_REVIEWER.md`
- `docs/SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md`
- `docs/TEST_EXECUTION_TIERS_AND_E2E_BUDGET.md`
- `docs/VALIDATION_INVARIANT_MATRIX.md`

## Candidate Variant Families

- **Code-oriented profile**: embeddings/ranking tuned for symbols, paths, and
  diff-local code evidence.
- **Sparse profile**: lexical/sparse semantic blend intended to reduce dense
  model dependency and install footprint.
- **Rerank profile**: second-pass ranking model over retrieved candidates.

## Evaluation Rubric

Any non-default profile must score positively across all dimensions below before
implementation beads are created.

| Dimension | Required proof |
| --- | --- |
| Retrieval value | Measurable improvement over `semantic-default` on representative Roger retrieval tasks (`recall`, `related_context`, `candidate_audit`, `promotion_review`) |
| Asset burden | Clear install footprint, download cost, and verification/repair story via Roger-owned asset commands |
| Degraded truth | Explicit lexical-only and `recovery_scan` behavior with no hidden fallback semantics |
| Operator surface cost | Bounded additions to `rr assets/status/doctor` and status messaging; no ambiguous mode reporting |
| Validation budget | Fits existing CI/E2E budget posture without forcing broad new real-provider or browser lanes |
| Memory-policy alignment | Preserves candidate/promoted boundary and does not weaken promotion-review workflow |

## Admission Gate For New Semantic Profiles

Create follow-on implementation beads only when all are true:

1. Benchmark evidence shows material retrieval gain versus `semantic-default` on
   Roger-owned scenarios.
2. Asset install + verify path remains explicit and Roger-owned (`rr assets ...`),
   with digest-verified manifests and fail-closed semantics.
3. Degraded-mode envelopes remain literal (`hybrid`, `lexical_only`,
   `recovery_scan`) and operator-visible.
4. Added profile does not break candidate/promotion boundaries or inflate
   `promotion_review` ambiguity.
5. Validation additions are proportionate and executable within the documented
   tier/budget model.

## Out-Of-Scope For This Spike

- Admitting any profile variant as live support.
- Expanding semantic indexing to raw full-code dumps or raw transcripts.
- Quietly changing `semantic-default` behavior without explicit artifact/version
  signaling.

## Operator-Surface Implications (If A Variant Is Admitted Later)

Later implementation beads must define, at minimum:

- profile identifiers and install commands
- profile-specific status and doctor reporting
- profile-specific verification metadata and digest provenance
- explicit fallback semantics when that profile is unavailable

## Non-Widening Statement

This spike does not change the current live search support matrix.
`semantic-default` remains the only supported baseline profile in `0.1.x`.
