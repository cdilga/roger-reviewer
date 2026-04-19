# rr-5dp9: Sacrificial PR Operator-Stability Rehearsal For Explicit Outbound Post-Back

## Scope

This runbook is the operator-stability lane for bead `rr-5dp9`.

It is intentionally separate from deterministic CI/E2E closeout and exists to prove a real GitHub mutation path for Roger's explicit outbound flow (`rr draft -> rr approve -> rr post`) on a throwaway target.

## Preconditions

1. Explicit outbound command surface is landed and truthful (`rr draft`, `rr approve`, `rr post`).
2. Browser launch handoff lane is already proven (`rr-6iah.8` closed).
3. Operator has GitHub credentials with write access to a sacrificial PR target.

## Credential Isolation And Target Policy

1. Use only sacrificial or personal maintainer credentials for this run.
2. Never post to third-party maintainer PRs for this rehearsal.
3. Use a throwaway/resettable PR target in a repo you control.
4. Retain command and JSON artifacts under `out/operator-stability/rr-5dp9-*`.
5. Cleanup is mandatory: delete the rehearsal comment or document why deletion was impossible.

## Full Rehearsal Procedure

1. Identify a session with at least one `accepted` + `not_drafted` finding.
2. Materialize drafts:
   - `./target/debug/rr draft --session <session-id> --finding <finding-id> --robot`
3. Approve exact batch:
   - `./target/debug/rr approve --session <session-id> --batch <draft-batch-id> --robot`
4. Post exact approved batch:
   - `./target/debug/rr post --session <session-id> --batch <draft-batch-id> --robot`
5. Capture remote identifiers from `data.posted_action.remote_identifier` and `data.item_results[*].remote_identifier`.
6. Cleanup posted artifact (issue comment path shown):
   - `gh api -X DELETE repos/<owner>/<repo>/issues/comments/<issuecomment-id>`
7. Verify cleanup and local state:
   - `gh pr view <pr> -R <owner>/<repo> --comments --json number,title,comments`
   - `./target/debug/rr findings --session <session-id> --robot`
   - `./target/debug/rr status --session <session-id> --robot`

## 2026-04-19 Live Execution Evidence

Artifact root:

- `out/operator-stability/rr-5dp9-live-postback-20260419T051908Z-pr6/`

Target used:

- repository: `cdilga/roger-reviewer`
- PR: `6`
- session: `session-1776424142-86073-2`

Exact command sequence executed:

1. Baseline session/findings capture (`01_rr_sessions_before.json`, `02_rr_findings_before.json`).
2. Draft attempt on `finding-test-2`:
   - `./target/debug/rr draft --session session-1776424142-86073-2 --finding finding-test-2 --robot`
   - Result: blocked with `reason_code=stale_local_state` because finding already had `outbound_state=awaiting_approval` (`03_rr_draft.json`).
3. Approve existing awaiting-approval batch:
   - `./target/debug/rr approve --session session-1776424142-86073-2 --batch draft-batch-1776424747-1929-1 --robot`
   - Approval id: `approval-1776576021-46430-1` (`04_rr_approve_existing_batch.json`).
4. Post approved batch:
   - `./target/debug/rr post --session session-1776424142-86073-2 --batch draft-batch-1776424747-1929-1 --robot`
   - Posted action id: `posted-batch-1776576021-46435-1`
   - Remote identifier: `https://github.com/cdilga/roger-reviewer/pull/6#issuecomment-4275238185`
   - Evidence: `05_rr_post_existing_batch.json`, `REMOTE_IDENTIFIER.txt`.
5. Cleanup of posted comment:
   - `gh api -X DELETE repos/cdilga/roger-reviewer/issues/comments/4275238185`
   - Evidence: `06_gh_delete_comment.json` (empty body expected for HTTP 204), `COMMENT_ID.txt`.
6. Post-cleanup verification:
   - PR comments snapshot excludes `issuecomment-4275238185` (`09_gh_pr_comments_after_cleanup.json`).
   - Local Roger state preserved posted lineage (`07_rr_findings_after.json`, `08_rr_status_after.json`).

## Validation Contract (rr-5dp9)

This bead's proof remains operator-stability evidence only.

It does not add deterministic CI/E2E requirements and does not widen the base automated validation budget.
