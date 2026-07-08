# Roger Skills

This directory contains reusable Roger-specific skill artifacts.

## Current files

- `DICKLESWORTHSTONE_SOURCE_EXTRACTION_NOTES.md`
  - Honest source notes describing what was directly found in public
    Dicklesworthstone materials and what was distilled into Roger-native form.

- `ROGER_ALIEN_ARTIFACT_DECISION_CONTRACT.md`
  - Roger-native decision contract for explainable, calibrated,
    review-safe decisions under uncertainty.

- `ROGER_EXTREME_SOFTWARE_OPTIMIZATION.md`
  - Roger-native performance-work contract based on baseline-first,
    profile-first, proof-backed optimization discipline.

- `ROGER_INSIDE_ROGER_AGENT.md`
  - Narrow agent-only skill for safe in-harness operation when already inside a
    Roger-managed session; keeps the agent on the supported Roger-native subset.

- `ROGER_REVIEW_DRIVER.md`
  - Operator-side skill for driving the full rr review loop from outside a
    provider session: start/resume/inspect, robot envelopes, provider
    selection, the seven-verb `rr send` draft → edit → approve → post
    chain, and fail-closed recovery.

- `ROGER_COPILOT_HARNESS.md`
  - Truthful operating recipe for the feature-gated GitHub Copilot CLI
    provider lane: gate enablement, doctor preflight, `review_readonly`
    policy posture, bounded Tier B continuity, and the `--interactive`
    terminal-handoff mode.

- `ROGER_WORKER_PROTOCOL.md`
  - Canonical recipe for being the review worker inside a Roger-managed
    provider session: task-file binding, the `rr agent worker.*` call
    sequence, the `WorkerStageResult`/`StructuredFindingsPack` return
    shapes, and the hard boundaries a worker must never cross.

- `ROGER_OPERATOR_QUICKSTART.md`
  - First-session walkthrough for a new operator: install, doctor
    preflight, picking review work, starting a review, where findings
    actually come from, the local cockpit, findings/search, and the send
    chain — truthful about which provider paths are live vs feature-gated.

- `ROGER_TUI_CHEATSHEET.md`
  - Exact keys and screens for the local review cockpit (`rr open`/`rr tui`),
    taken directly from `packages/tui` key handlers and help overlay.

## Intended use

These files are for:

- planning rounds
- implementation beads
- architecture critique sessions
- performance work
- finding/routing/retrieval decision design

They should be treated as reusable operating contracts, not just background
reading.
