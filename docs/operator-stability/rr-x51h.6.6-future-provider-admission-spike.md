# rr-x51h.6.6 Future Provider Admission Spike

Date: 2026-04-19
Bead: `rr-x51h.6.6`

## Decision Summary

No provider beyond the current matrix is admitted by this spike.

This spike defines a future-provider admission rubric and minimum proof packet
for any Tier A/B/C expansion after the current matrix.

## Current Matrix Baseline

Current support order and scope remain as defined in the canonical plan and
release matrix:

1. GitHub Copilot CLI (feature-gated bounded Tier B, not default public live)
2. OpenCode (strongest current continuity path)
3. Codex (bounded Tier A)
4. Gemini (bounded Tier A)
5. Claude Code (bounded Tier A)
6. Pi-Agent (deferred candidate, not live)

## Admission Rubric

A future provider (or deeper tier claim for an existing bounded provider) must
pass all sections below before implementation beads are created.

| Section | Required bar |
| --- | --- |
| Launch truth | Deterministic start with verified provider session binding; no completed-looking Roger session before verified binding |
| Continuity truth | Honest `usable/degraded/unusable` reporting with explicit fallback behavior; no implied reopen without proof |
| Policy safety | Enforce Roger policy profile constraints (review read-only posture where required) with fail-closed behavior |
| Worker boundary | No bypass of Roger-owned worker/task contracts, finding normalization, approval gating, or posting control |
| Auditability | Canonical launch/provider artifacts and bounded failure taxonomy preserved in Roger-owned evidence |
| Operator surface truth | `rr doctor`, `rr status`, `rr robot-docs`, docs/matrix wording remain literal and non-aspirational |
| Validation cost | Required suites fit the documented CI/E2E budget model; no hidden new always-on expensive lanes |

## Minimum Proof Packet

Any admission proposal must include, at minimum:

1. Capability mapping to Tier A/B/C contract rows from
   `HARNESS_SESSION_LINKAGE_CONTRACT.md`.
2. Fail-closed launch + continuity evidence with deterministic doubles.
3. Doctor/status/help/robot truth wiring showing exact support posture.
4. Audit artifact classes and degraded-mode behavior definitions.
5. Explicit support wording proposal for README/AGENTS/release matrix.
6. Validation plan naming concrete suite IDs and budget tier placement.

## Candidate Ranking (Post-Matrix)

### Priority 1: Pi-Agent (existing deferred candidate)

- Keep as explicit candidate for evaluation under `rr-x51h.6.6.1`.
- Must prove direct-CLI launch truth, policy control, auditability, and
  continuity-tier fit before any support wording changes.

### Priority 2: Deeper-tier upgrades for existing bounded providers

- Codex/Gemini/Claude can only move beyond Tier A if they satisfy Tier B/Tier C
  requirements with real proof.
- Brand/reputation is not proof; tier claims follow capability evidence only.

### Priority 3: Editor-hosted integrations (VS Code/JetBrains/Copilot IDE)

- Evaluate only through explicit edge-adapter gates from `rr-x51h.7.7`
  (direct-CLI baseline first, ACP/MCP only when justified).

## Out-Of-Scope For This Spike

- Admitting any new provider as live support immediately.
- Granting Tier B/Tier C claims to existing bounded providers without full proof.
- Treating protocol/editor availability as automatic provider admission.

## Non-Widening Statement

This spike defines future admission gates only. It does not change live provider
support claims.
