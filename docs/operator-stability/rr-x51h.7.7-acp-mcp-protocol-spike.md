# rr-x51h.7.7 ACP/MCP Protocol-Edge Spike

Date: 2026-04-19
Bead: `rr-x51h.7.7`

## Decision Summary

Roger remains **direct-CLI-first** for `0.1.x`.

This spike does not widen any live provider support claim. ACP and MCP remain
future edge adapters gated by explicit proof requirements.

## Inputs Reviewed

- `docs/PLAN_FOR_ROGER_REVIEWER.md`
- `docs/REVIEW_WORKER_RUNTIME_AND_BOUNDARY_CONTRACT.md`
- `docs/DEV_MACHINE_ONBOARDING.md`
- `docs/ROUND_06_PROVIDER_TRUTH_AND_BEAD_RECONCILIATION_BRIEF.md` (historical context only)

## Comparison

| Dimension | Direct CLI + Roger-owned hooks (current baseline) | ACP as harness-control edge (future candidate) | MCP as tool/context edge (future candidate) |
| --- | --- | --- | --- |
| Session authority | Roger controls launch, locator, `ResumeBundle`, retry, and status in canonical store | Viable only if ACP exposes stable session identity and lifecycle control equal to current direct path | Not a harness authority layer; must stay client-facing/context-facing |
| Safety posture | Current policy/hook digests and fail-closed rules are already enforceable | Must prove no policy bypass and no weaker fail-closed behavior than direct path | Must be read-mostly in review mode; no bypass of approval/posting controls |
| Continuity truth | Current lane can represent usable/degraded/unusable truth without protocol indirection | Must reduce lifecycle complexity measurably while preserving truthful degrade/fallback semantics | Must not become hidden continuity authority or implied reopen/rebind path |
| Auditability | Launch attempts, provider artifacts, and posting approvals remain Roger-owned | Must preserve canonical audit classes and provenance parity | Must preserve canonical audit classes and provenance parity |
| `0.1.x` fit | Shipped baseline | Not required for first implementation | Not required for first implementation |

## ACP Admission Gate (Future)

Create ACP follow-on implementation beads only if all are true:

1. At least one real provider/client exposes stable ACP lifecycle primitives that map cleanly to Roger's `SessionLocator` and retry semantics.
2. ACP path demonstrably reduces adapter complexity versus direct CLI for the same provider lane.
3. ACP path preserves or improves fail-closed safety, policy digest enforcement, and audit evidence.
4. ACP path passes the same continuity truth matrix (usable/degraded/unusable + explicit fallback) as direct CLI.

## MCP Admission Gate (Future)

Create MCP follow-on implementation beads only if all are true:

1. Roger worker operations are schema-stable enough to expose through a local adapter without leaking internal volatility.
2. MCP remains edge-only: read-mostly review context and bounded Roger tools; no hidden mutation or posting authority.
3. MCP cannot bypass finding validation, approval gates, or policy-profile restrictions.
4. At least one concrete client/editor integration proves material operator value not already provided by direct CLI + worker transport.

## Safety Boundaries That Stay Fixed

- No automatic GitHub posting.
- No implicit widening of provider capabilities.
- No replacement of Roger's canonical store/session model with external protocol state.
- No default broad MCP access in review mode.

## Impact On Deferred Follow-ons

This spike resolves the protocol-edge decision framing so later deferred beads can
be evaluated against explicit gates:

- `rr-x51h.6.6` (future provider admissions)
- `rr-x51h.6.6.1` (pi_agent_rust admission evaluation)

Those beads remain separate admission decisions and must not infer support from
this memo alone.

## Non-Widening Statement

No live support matrix row changes because of this spike. Current live claim
remains the direct-CLI baseline with existing bounded provider posture.
