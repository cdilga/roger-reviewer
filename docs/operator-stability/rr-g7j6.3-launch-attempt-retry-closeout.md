# rr-g7j6.3 Launch Attempt Retry Closeout

## Purpose

`rr-g7j6.3` closes the stale-launch ambiguity gap by forcing explicit
abandonment of older in-flight launch attempts when a new retry attempt is
created for the same session/action tuple.

This prevents hidden replay behavior and keeps retry lineage inspectable.

## Implementation

- `packages/storage/src/lib.rs`
  - `create_launch_attempt` now marks older non-terminal attempts for the same
    `requested_session_id + action` as `abandoned` before inserting the new
    attempt.
  - abandoned rows now receive:
    - `state = abandoned`
    - `failure_reason` with retry linkage (`abandoned in favor of retry attempt <id>`)
    - `finalized_at` timestamp and `row_version` bump

- `packages/cli/tests/lifecycle_transaction_smoke.rs`
  - added `resume_retries_abandon_inflight_attempt_before_creating_new_attempt_id`
    to prove:
    - stale in-flight attempt is finalized as `abandoned`
    - retry creates a different/new launch attempt id
    - abandonment record points to the new retry attempt id

## Acceptance Mapping

1. Pending/in-flight attempts are now explicitly classified as `abandoned`
   during retry creation, with stored guidance in `failure_reason`.
2. Retry semantics remain new-attempt-id based; the regression explicitly checks
   that retry id differs from the stale in-flight id.
3. Stale binding/evidence protections continue to hold via existing stale-binding
   and lifecycle transaction coverage; retry does not reuse stale launch rows.

## Validation Evidence (2026-04-19)

- `cargo test -q -p roger-cli --test lifecycle_transaction_smoke -- --nocapture`
- `cargo test -q -p roger-cli --test stale_launch_binding_smoke -- --nocapture`
- `cargo test -q -p roger-storage --test storage_smoke -- --nocapture`

All commands passed on current `main`.
