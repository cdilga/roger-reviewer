# Extension Popup Redesign Brief (`rr-22ak.1`)

This brief is the authority packet for the popup redesign lane before
implementation (`rr-22ak.3`). It replaces ad hoc style tweaks with an explicit
interaction hierarchy that stays GitHub/Primer-aligned while introducing a
Roger-specific metallic identity treatment.

## Scope

- Surface: extension popup fallback only (`apps/extension/src/popup/*`)
- Constraint: popup remains a bounded manual backup launcher
- Non-goals: changing posting authority, changing action set semantics, adding
  extension-owned review state authority

## Current Surface Audit

Observed in the current popup implementation:

1. Button text and hierarchy are awkward: all actions look equivalent even
   though only one action should lead operator attention per context.
2. Descriptive copy is overloaded: subtitle/status text combines context,
   fallback policy, and error messaging in one lane.
3. Build/version metadata sits inline in the primary card and competes with
   first-read operator decisions.
4. A redundant status line pattern consumes vertical space while providing weak
   decision value.
5. Signal Beacon styling reads as a separate shell rather than GitHub-native
   with Roger-specific accent.

## Information Architecture

Target popup card hierarchy:

1. **Header row**
   - Left: compact Roger mark and short title.
   - Right: explicit info affordance button (`IconButton`-style control with
     accessible label).
2. **Context line**
   - PR context (`owner/repo#number`) when on PR page.
   - Neutral fallback context when not on PR page.
3. **Primary guidance line**
   - One short sentence for immediate next step only.
   - No long advisory essay in the main card.
4. **Action group**
   - One primary action + supporting secondary actions.
5. **Transient feedback region**
   - Reserved for launch-dispatch success/failure feedback only.
   - Not duplicated as always-on status prose.

## Action Hierarchy And Button Variants

Button hierarchy follows Primer interaction intent (one primary action per
group, with clear secondary treatment):

- `start_review`
  - Variant: `primary`
  - Role: first action when PR context is present and no active session is
    inferred.
- `resume_review`
  - Variant: `default`
  - Role: continuity path when operator wants to re-open an existing session.
- `show_findings`
  - Variant: `default` or subtle secondary depending on final spacing.
  - Role: read-first path that never out-ranks launch/start in initial scan.

Disabled states keep readable text and reduced emphasis; they do not replace
labels with unclear copy.

## Copy And States

Copy rules:

- Main guidance is concise and task-oriented.
- Error copy appears only in transient feedback region and uses explicit cause
  plus next step.
- Long-lived policy or build explanations do not stay in the main subtitle.

State text rules:

- **Loading**: brief verb-first text (for example `Checking PR context…`).
- **Dispatch in progress**: brief action text (for example
  `Dispatching launch intent…`).
- **Error**: one sentence with actionable next step.

## Info Affordance (Build/Version And Supplemental Guidance)

The inline build/version row is removed from the primary card body.

Replacement:

- persistent info affordance in the header (icon button with visible focus and
  accessible label)
- click opens a popover or compact modal containing:
  - build/version metadata
  - fallback policy reminder
  - low-frequency troubleshooting guidance

Tooltip-only delivery is not sufficient for critical metadata because tooltip
visibility is ephemeral. Build/version and fallback semantics must be available
through explicit persistent disclosure.

## Visual Language Split

GitHub-native cues (must remain):

- button sizing and emphasis hierarchy
- spacing rhythm for dense utility surfaces
- neutral base palette and border/shadow restraint
- readable typography optimized for compact control surfaces

Roger-specific metallic cues (allowed and required):

- metallic accent ramp for brand moments (mark, subtle chrome accents)
- walkie-talkie-derived mark/wordmark direction
- restrained branded highlights that do not alter control semantics

Disallowed drift:

- returning to full Signal Beacon shell styling for the popup card
- styling that makes fallback semantics look like primary in-page authority
- high-saturation treatments that reduce text/action legibility

## Implementation Handoff Checklist

Implementation bead (`rr-22ak.3`) must demonstrate:

1. one-primary-action hierarchy in the popup action group
2. no inline build/version row in the primary card
3. explicit header info affordance with persistent disclosure content
4. concise main guidance copy and non-redundant status/feedback treatment
5. GitHub-native interaction structure with Roger metallic identity accents
