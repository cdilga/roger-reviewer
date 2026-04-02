# Beads Workspace Status

As of 2026-04-01, the Roger Reviewer beads workspace is healthy after a second
`br` path correction: queue-critical claim mutations now run through a known
good pinned runtime while upstream/localfix regressions remain documented.

## Current state

- default automation path now resolves through
  `/Users/cdilga/.local/bin/br -> /Users/cdilga/.local/bin/br-0.1.28.pinned`
- this rollback was applied on 2026-04-01 because `br-0.1.34.localfix`
  introduced claim-mutation FK failures on ready-bead updates in the active
  Roger workspace
- stock/pinned `0.1.34` remains repro-bad for fresh-init integrity checks
  (`Page ...: never used`)
- `br-0.1.28.pinned` currently satisfies both trust predicates used by Roger:
  fresh-init integrity and routine mutation-path updates
- [`.beads/beads.db`](/Users/cdilga/Documents/dev/roger-reviewer/.beads/beads.db)
  passes SQLite integrity checks
- [`.beads/issues.jsonl`](/Users/cdilga/Documents/dev/roger-reviewer/.beads/issues.jsonl)
  and the DB are in sync with 109 issues
- `rr-012` is now closed in the live beads graph, matching the Round 04
  reconciliation outcome
- `rr-025` is now closed; `VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md` is the
  artifact that satisfies its acceptance criteria
- `rr-025.1`, `rr-025.2`, and `rr-025.3` are now closed, so the upfront
  validation-harness scaffold, fixture corpus, and CI-tier entrypoint lane is
  no longer a planning gap
- `rr-q18` and `rr-3ve` are closed, so the remaining open beads are
  implementation work rather than planning-gate work
- the graph now includes additional independent leaf tasks under harness and
  prompt execution, including `rr-003.7`, `rr-003.8`, and `rr-016.1` through
  `rr-016.3`
- `rr-003.1` is currently `in_progress`, and `br ready` currently surfaces
  `rr-003.7` and `rr-003.8` as the next unblocked implementation leaves
- fresh-workspace `br init`, repeated `br create`, `br doctor`, repo-local
  `br info`, and repo-local `br show` all complete successfully through the
  default `br` path
- stock upstream `br 0.1.29` through `0.1.34` were repro-bad locally: a fresh temp workspace
  failed native `sqlite3` integrity checks after ordinary sequential
  `br create` operations
- upstream `main` at commit `1130411b1dfa646c769b1f56735d9dd9942b8db0` was
  still repro-bad on 2026-03-31
- upstream regression report filed:
  `Dicklesworthstone/beads_rust#213`
- local source investigation found the fresh-schema bootstrap was still running
  legacy v3/v4 migrations before `PRAGMA user_version` was set, which caused
  fresh DBs to drop and recreate `idx_issues_ready`; with current
  `frankensqlite`, that leaked the old root page and triggered the
  `Page 17: never used` integrity failure
- the practical local fix was:
  1. repair the current workspace DB with SQLite checkpoint plus `VACUUM`
  2. install a patched local `br` build that skips legacy migrations on a
     truly fresh DB

## Validation performed

- `br --version` -> `br 0.1.34`
- `br info`
- `br ready`
- `br doctor`
- `readlink /Users/cdilga/.local/bin/br` ->
  `/Users/cdilga/.local/bin/br-0.1.34.localfix`
- temp repro with `/Users/cdilga/.local/bin/br-0.1.34.bak` (stock `0.1.34`)
  on `init -> create -> create -> sqlite3 integrity_check`
  -> `Page 17: never used; Page 57: never used; Page 58: never used; Page 60: never used`
- temp repro with `/Users/cdilga/.local/bin/br-0.1.34.pinned` on the same
  steps -> `Page 17: never used; Page 57: never used; Page 58: never used; Page 60: never used`
- temp repro with `br 0.1.34`:
  `git init && br init && br create ... && sqlite3 .beads/beads.db "PRAGMA integrity_check;"`
  -> `Page 17: never used`
- temp repro with upstream `main` on the same steps
  -> `Page 17: never used`
- temp repro with local `br-0.1.34.localfix` on the same steps
  -> `ok`
- temp repro with local `br-0.1.34.localfix` plus `br doctor`
  -> clean doctor run except expected frankensqlite WAL-sidecar warning
- repo-local `br show rr-001.6`
- `sqlite3 .beads/beads.db "PRAGMA wal_checkpoint(TRUNCATE); VACUUM; PRAGMA integrity_check;"`
  -> `ok`

## Canonical workspace-trust check

The canonical trust report command for DB-vs-JSONL parity and open-count sanity
is:

```sh
./scripts/swarm/check_beads_trust.sh
```

For deterministic fixture validation (without using the live workspace DB), the
same command accepts explicit input paths:

```sh
./scripts/swarm/check_beads_trust.sh --db /tmp/beads.db --jsonl /tmp/issues.jsonl
```

When explicit DB/JSONL paths are supplied, `br doctor` is skipped automatically
so only the direct parity and integrity checks are evaluated.

## Notes

- `br doctor` still reports preserved recovery artefacts in
  [`.beads/.br_recovery`](/Users/cdilga/Documents/dev/roger-reviewer/.beads/.br_recovery).
  That is currently a cleanup warning, not a functional error.
- The active local `br` path is intentional, but it is not a permanent
  version-policy claim. Reevaluate future upstream versions explicitly rather
  than assuming the local patch will be needed forever.
- Do not replace the default `br` symlink with stock upstream `0.1.34` or
  current upstream `main` unless they have been repro-verified locally against
  the same fresh-init and doctor matrix.
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
