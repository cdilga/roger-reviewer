# Roger Reviewer ADRs

This directory holds architecture decision records that turn the planning-stage
questions in
[`PLAN_FOR_ROGER_REVIEWER.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/PLAN_FOR_ROGER_REVIEWER.md)
into explicit implementation contracts.

## Current ADR set

- `001-rust-first-local-runtime.md` — `accepted`
- `002-harness-and-session-durability-contract.md` — `accepted`
- `003-browser-bridge-and-extension-dependency-policy.md` — `accepted`
- `004-scope-and-memory-promotion-policy.md` — `accepted`
- `005-multi-instance-and-resource-isolation.md` — `accepted`
- `006-structured-findings-contract-and-repair-loop.md` — `accepted`
- `007-harness-native-roger-command-surface.md` — `accepted`
- `008-tui-runtime-and-concurrency-boundary.md` — `accepted`
- `009-prompt-preset-and-outcome-events.md` — `accepted`

## Status meanings

- `accepted`: chosen and should guide implementation
- `proposed`: recommended direction, but still open to revision
- `superseded`: replaced by a later ADR
- `rejected`: considered and deliberately not chosen

## Usage

- Update the canonical plan if an ADR changes the planning baseline.
- Once an ADR is accepted, higher-level docs should describe the broad
  direction as decided and leave only the residual sub-questions open.
- Keep the decision and the consequences short and implementable.
- Move unresolved sub-questions into follow-up tasks rather than leaving them
  ambient in the architecture.
