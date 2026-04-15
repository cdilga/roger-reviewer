# Extension Identity Direction

This document records the branding rationale and narrowed options for the
extension identity lane (`rr-vsr2.*`).

## Goal

Introduce a distinct Roger extension identity that is recognizable in compact
browser surfaces while keeping GitHub-native usability and bounded launch
semantics intact.

## Options Considered

### Option A: Signal Beacon (Selected)

- Geometric mark with high contrast and clear silhouette at small sizes.
- Soft cyan/teal accent family for "assistive, local-first" tone.
- Two-line wordmark for popup headers and docs.
- Shared token sheet to keep brand colors consistent across popup and in-page
  surfaces without forcing a full theme rewrite.

Why selected:

- Stays readable in dense popup layouts.
- Distinct from GitHub's native marks without looking off-brand in context.
- Scales to icon, chip, and card treatments.

### Option B: Monogram-Only Minimal

- Single-letter glyph with monochrome palette.
- Very low visual footprint.

Why not selected now:

- Too ambiguous in the popup context and weak for future mixed surfaces.
- Harder to express a system (mark + wordmark + tokens) from one glyph alone.

### Option C: GitHub-Mimic Neutral

- Near-default Primer-like neutral palette and typography.
- Minimal Roger-specific visual personality.

Why not selected now:

- Underdelivers on the explicit "Roger identity" objective.
- Makes extension surfaces feel generic and less discoverable.

## Chosen Direction

Direction: **Signal Beacon**.

Asset set:

- `apps/extension/static/roger-mark.svg`
- `apps/extension/static/roger-wordmark.svg`
- `apps/extension/static/roger-identity.css`

## Adoption Map (Current)

- Popup shell: adopted in `apps/extension/src/popup/index.html`
- In-page PR entry surfaces: deferred to `rr-vsr2.3`
- Future surfaces (options/settings/onboarding): pending future lane

## Guardrails

- Branding must not introduce posting/approval controls.
- Branding must not imply extension authority beyond bounded launch/status role.
- Popup action set remains exactly:
  `start_review`, `resume_review`, `show_findings`.

## Validation Hooks

- `node --test apps/extension/src/popup/index.test.js apps/extension/src/popup/main.test.js`
- `scripts/swarm/validate_extension_entry_placements.sh`
- `docs/extension-visual-identity-smoke.md`
