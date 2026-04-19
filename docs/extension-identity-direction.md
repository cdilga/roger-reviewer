# Extension Identity Direction

This document records the branding rationale and narrowed options for the
extension identity lane (`rr-vsr2.*`).

## Goal

Introduce a distinct Roger extension identity that is recognizable in compact
browser surfaces while keeping GitHub-native usability and bounded launch
semantics intact.

## Options Considered

### Option A: Walkie-Talkie Relay (Selected)

- Compact walkie-talkie mark with radio pulse accent and metallic shell.
- Steel and graphite neutrals aligned to GitHub/Primer surfaces, with a focused
  blue radio accent for interactive emphasis.
- Two-line wordmark with embedded compact relay motif for popup headers and docs.
- Shared token sheet that supports a metallic variant without implying a full
  extension theme takeover.

Why selected:

- Stays legible at extension popup scale (16-34px mark usage).
- Feels GitHub-adjacent instead of generic teal product chrome.
- Preserves a clear Roger-specific personality across mark, wordmark, and chip
  treatments.

### Option B: Monogram-Only Minimal

- Single-letter glyph with monochrome palette.
- Very low visual footprint.

Why not selected now:

- Too ambiguous in the popup context and weak for future mixed surfaces.
- Harder to express a system (mark + wordmark + tokens) from one glyph alone.

### Option C: Signal Beacon (Demoted)

- Prior teal beacon direction used in earlier popup identity work.

Why demoted:

- Over-indexed on teal accenting and now clashes with the metallic GitHub-aligned
  redesign direction tracked under `rr-22ak.*`.
- Did not carry clear "radio relay" metaphor for operator continuity.

## Chosen Direction

Direction: **Walkie-Talkie Relay**.

Asset set:

- `apps/extension/static/roger-mark.svg`
- `apps/extension/static/roger-wordmark.svg`
- `apps/extension/static/roger-identity.css`

## Adoption Map (Current)

- Popup shell: adopted in `apps/extension/src/popup/index.html`
- In-page PR entry surfaces: deferred to `rr-vsr2.3`
- Future surfaces (options/settings/onboarding): pending future lane

## Deprecated Direction Note

The earlier **Signal Beacon** rationale is now historical context only. New
identity or token updates should follow the walkie-talkie metallic direction and
should not add fresh Signal Beacon derivatives.

## Guardrails

- Branding must not introduce posting/approval controls.
- Branding must not imply extension authority beyond bounded launch/status role.
- Popup action set remains exactly:
  `start_review`, `resume_review`, `show_findings`.

## Validation Hooks

- `node --test apps/extension/src/popup/index.test.js apps/extension/src/popup/layout_redesign.test.js apps/extension/src/popup/main.test.js`
- `scripts/swarm/validate_extension_entry_placements.sh`
- `docs/extension-visual-identity-smoke.md`
