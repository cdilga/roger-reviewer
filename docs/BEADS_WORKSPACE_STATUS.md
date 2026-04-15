# Beads Workspace Status

As of 2026-04-15, the Roger Reviewer Linux workspace uses a source-built
`0.1.40.pinned` as the default `br`, and the live workspace trust story is
clean again after rebuilding `.beads/beads.db` from canonical
`.beads/issues.jsonl`.

## Current state

- default automation path now resolves through
  `~/.local/bin/br -> ~/.local/bin/br-0.1.40.pinned`
- `scripts/swarm/resolve_br.sh` and `scripts/swarm/br_pinned.sh` now default to
  `0.1.40`
- the latest official upstream release is still `v0.1.39`
  (published `2026-04-14T21:11:16Z`)
- Roger now pins a locally built `0.1.40` from upstream `main` commit
  `32f4a1616deea380c4f47ea40c542fb26e7e6e59`
  (`2026-04-14T22:48:28-04:00`,
  `fix(import): reject --dry-run with --file, fix 3 broken tests`)
- local Linux `x86_64` repro history now shows:
  - `br-0.1.36.pinned` failed the fresh temp-workspace matrix with
    `Page 17: never used` corruption
  - upstream `v0.1.38` failed the same matrix with
    `Tree 50 page 50: free space corruption`
  - upstream `v0.1.39` passed the fresh-init matrix
  - source-built `0.1.40` passed the fresh-init matrix and successfully rebuilt
    the Roger workspace from canonical JSONL
- the live workspace currently reports `456` issues in
  [`.beads/beads.db`](.beads/beads.db)
  (`open=87`, `closed=364`, `deferred=5`)
- the 2026-04-15 live repair preserved a full backup snapshot under
  [`.beads/.manual_repair_20260415_033050`](.beads/.manual_repair_20260415_033050)
- the repaired live workspace now passes the canonical trust check:
  `./scripts/swarm/check_beads_trust.sh` reports
  `TRUST_STATUS=pass` and `TRUST_REASON=db and jsonl agree on current issue truth`
- `br doctor` now passes both internal and native SQLite integrity checks
- `br sync --status --json` reports `dirty_count=0`,
  `jsonl_newer=false`, and `db_newer=false`
- exact issue lookup is restored on the live workspace; for example,
  `br show rr-x51h.1.2 --json` now succeeds again
- both DB-backed and JSONL-backed ready queues currently report 13 ready
  issues
- the open graph remains concentrated in Round 06 provider-truth,
  outbound-flow, search-planner, release/update, and proof-mapping lanes
- `bv --robot-insights` still reports `0` dependency cycles
- upstream regression report for the earlier fresh-init failures remains:
  `Dicklesworthstone/beads_rust#213`

## Validation performed

- `git ls-remote https://github.com/Dicklesworthstone/beads_rust.git HEAD refs/heads/main`
  -> `32f4a1616deea380c4f47ea40c542fb26e7e6e59`
- local source build:
  - clone upstream `beads_rust` main
  - fast-forward sibling `frankensqlite` checkout to upstream main
  - `cargo build --release --bin br`
  - resulting binary version -> `br 0.1.40`
- fresh-init matrix:
  - `0.1.36` failed
  - `0.1.38` failed
  - `0.1.39` passed
  - source-built `0.1.40` passed
- throwaway Roger workspace tests against the malformed live-state copy:
  - DB-backed exact issue lookup still failed on the copied malformed DB
  - `--no-db` update paths succeeded
  - `br sync --import-only --rebuild` on the malformed copy did not fix it in place
- clean-room rebuild validation:
  - create a fresh temp workspace
  - copy in canonical `.beads/issues.jsonl`
  - `br init`
  - copy live `config.yaml`
  - `br sync --import-only --rebuild --json`
  - `sqlite3 .beads/beads.db "PRAGMA wal_checkpoint(TRUNCATE); VACUUM; PRAGMA integrity_check; PRAGMA foreign_key_check;"`
  - result: native SQLite `ok`, clean sync state, exact lookup restored, and
    DB-backed update succeeded
- live repair and promotion:
  - backup snapshot before swap:
    [`.beads/.manual_repair_20260415_033050`](.beads/.manual_repair_20260415_033050)
  - replace live `.beads/beads.db` with the clean rebuilt DB
  - install the source-built binary to
    `~/.local/bin/br-0.1.40.pinned`
  - repoint `~/.local/bin/br`
- post-repair validation:
  - `br --version` -> `br 0.1.40`
  - `readlink -f ~/.local/bin/br`
    -> `/home/ubuntu/.local/bin/br-0.1.40.pinned`
  - `./scripts/swarm/check_beads_trust.sh`
    -> `TRUST_STATUS=pass`, `TRUST_REASON=db and jsonl agree on current issue truth`
  - `br doctor`
    -> `OK sqlite.integrity_check` and `OK sqlite3.integrity_check`
  - `br sync --status --json`
    -> `dirty_count=0`, `jsonl_newer=false`, `db_newer=false`
  - `sqlite3 .beads/beads.db "select status, count(*) from issues group by status order by status;"`
    -> `closed|364`, `deferred|5`, `open|87`
  - DB-backed and JSONL-backed ready counts both remain `13`

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

- As of 2026-04-15, the bead graph is execution-ready and the live workspace
  trust has been restored. The current blocker is no longer workspace health;
  it is only the normal question of which ready beads to execute next.
- `br doctor` still reports preserved recovery artefacts in
  [`.beads/.br_recovery`](.beads/.br_recovery).
  That is currently a cleanup warning, not a functional error.
- If the workspace regresses again, prefer a fresh DB rebuild from canonical
  JSONL over repeated in-place vacuum-style repair. This repo has now seen
  vacuum clear corruption once and then later regress, while the clean rebuild
  path restored both integrity and exact-ID lookup.
- If DB-backed `br ready`/`show`/`list` start returning `snapshot conflict`
  immediately after repair or checkpoint work, restart long-lived `bv` or other
  DB readers first. Until they release the stale snapshot, use `br ... --no-db`
  only for read-only queue inspection; do not treat `--no-db` as a safe
  mutation path.
- The active local `br` path is intentional, but it is not a permanent
  version-policy claim. Reevaluate future upstream versions explicitly rather
  than assuming `0.1.40` will stay safe forever.
- Do not replace the default `br` symlink with a newer upstream build unless it
  has been repro-verified locally against the same fresh-init matrix and a
  Roger workspace rebuild test.
- The pre-repair database files were preserved in
  [`.beads/.manual_repair_20260328_2056`](.beads/.manual_repair_20260328_2056)
  for audit and rollback purposes.
- A second repair snapshot from the 2026-03-29 sync pass was preserved in
  [`.beads/.manual_repair_20260329_201553`](.beads/.manual_repair_20260329_201553).
- A third post-diagnosis snapshot was preserved in
  [`.beads/.manual_repair_20260329_2146`](.beads/.manual_repair_20260329_2146).
- A fourth backup snapshot was preserved immediately before the successful
  2026-04-14 live repair in
  [`.beads/.manual_repair_20260414_223207`](.beads/.manual_repair_20260414_223207).
- A fifth backup snapshot was preserved immediately before the successful
  2026-04-15 rebuild-and-swap repair in
  [`.beads/.manual_repair_20260415_033050`](.beads/.manual_repair_20260415_033050).
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
