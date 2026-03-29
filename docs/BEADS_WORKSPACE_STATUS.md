# Beads Workspace Status

As of 2026-03-29, the Roger Reviewer beads workspace has been repaired again
after diagnosing an upstream `br` regression and is healthy.

## Current state

- `br` is pinned locally to `0.1.28`
- [`.beads/beads.db`](/Users/cdilga/Documents/dev/roger-reviewer/.beads/beads.db)
  passes SQLite integrity checks
- [`.beads/issues.jsonl`](/Users/cdilga/Documents/dev/roger-reviewer/.beads/issues.jsonl)
  and the DB are back in sync with 44 issues
- `rr-012` is now closed in the live beads graph, matching the Round 04
  reconciliation outcome
- `br doctor`, `br info`, and clean-path `br sync --flush-only` complete
  successfully from the current workspace state
- `br 0.1.29` through `0.1.34` were repro-bad locally: a fresh temp workspace
  failed native `sqlite3` integrity checks after ordinary sequential
  `br create` operations
- upstream regression report filed:
  `Dicklesworthstone/beads_rust#213`
- the practical local fix was to downgrade back to `0.1.28`, then run SQLite
  checkpoint plus `VACUUM` on this workspace DB before re-validating

## Validation performed

- `br --version` -> `br 0.1.28`
- `br info`
- `br list --status open`
- `br ready`
- `br doctor`
- temp repro with `br 0.1.34`:
  `git init && br init && br create ... && sqlite3 .beads/beads.db "PRAGMA integrity_check;"`
  -> `Page 17: never used`
- temp repro with `br 0.1.28` on the same steps
  -> `ok`
- `br doctor`
- `br show rr-012 rr-015`
- `br sync --flush-only -v`
- `sqlite3 .beads/beads.db "PRAGMA wal_checkpoint(TRUNCATE); VACUUM; PRAGMA integrity_check;"`
  -> `ok`

## Notes

- `br doctor` still reports preserved recovery artefacts in
  [`.beads/.br_recovery`](/Users/cdilga/Documents/dev/roger-reviewer/.beads/.br_recovery).
  That is currently a cleanup warning, not a functional error.
- The active local pin is intentional. Do not upgrade `br` past `0.1.28` in
  this workspace until upstream issue `#213` is resolved or a newer version is
  repro-verified locally.
- The pre-repair database files were preserved in
  [`.beads/.manual_repair_20260328_2056`](/Users/cdilga/Documents/dev/roger-reviewer/.beads/.manual_repair_20260328_2056)
  for audit and rollback purposes.
- A second repair snapshot from the 2026-03-29 sync pass was preserved in
  [`.beads/.manual_repair_20260329_201553`](/Users/cdilga/Documents/dev/roger-reviewer/.beads/.manual_repair_20260329_201553).
- A third post-diagnosis snapshot was preserved in
  [`.beads/.manual_repair_20260329_2146`](/Users/cdilga/Documents/dev/roger-reviewer/.beads/.manual_repair_20260329_2146).
- A narrower `br` derived-cache bug still exists on some maintenance paths that
  bypass normal `br update` auto-flush behavior. In a temp repro, a dirty issue
  exported the JSONL successfully and then failed while repopulating
  `export_hashes` with `UNIQUE constraint failed: export_hashes.issue_id`.
  This is a derived-state bug, not evidence that the canonical issue data was
  lost.
- If that exact failure resurfaces after the JSONL write has already completed,
  the local repair is to treat the JSONL as canonical for that moment and
  rebuild the derived export state:

  ```sh
  sqlite3 .beads/beads.db "
    BEGIN;
    DELETE FROM dirty_issues;
    DELETE FROM export_hashes;
    INSERT INTO export_hashes(issue_id, content_hash, exported_at)
    SELECT id, content_hash, CURRENT_TIMESTAMP FROM issues;
    COMMIT;
  "
  br doctor
  br sync --flush-only -v
  ```

  This workaround should be used only for this specific derived-cache failure,
  not as a routine sync path.
- `bv --robot-*` still emits `bd ...` example commands in its output. In this
  repo, translate those to `br ...`; the workspace state itself is no longer the
  cause of that mismatch.
- The approved `bd` compatibility installed by `mcp_agent_mail` is an
  interactive-shell alias (`bd='br'`), not a standalone binary for
  non-interactive tool invocations.
