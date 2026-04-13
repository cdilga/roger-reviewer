# Beads Workspace Status

As of 2026-04-12, the Roger Reviewer beads workspace is back on the vetted
`0.1.34.pinned` runtime and the live DB has been repaired with the documented
SQLite checkpoint plus `VACUUM` flow. The current trust issue is no longer
"which `br` binary is selected?" but DB-backed `br` reads failing with
`SQLITE_BUSY_SNAPSHOT` while a long-lived `bv` reader still holds an older
snapshot.

## Current state

- default automation path now resolves through
  `/Users/cdilga/.local/bin/br -> /Users/cdilga/.local/bin/br-0.1.34.pinned`
- `scripts/swarm/resolve_br.sh` is the reason terminals keep snapping back to
  `0.1.34`: its current hard pin is `PINNED_VERSION=0.1.34`
- the latest official upstream release is `v0.1.38` (published 2026-04-10) and
  its Apple Silicon asset matches the local
  `/Users/cdilga/.local/bin/br-0.1.38.release` byte-for-byte
- upstream `main` is newer than `v0.1.38` by 5 commits (`v0.1.38-5-g6d121a1`),
  but a local head build still reproduces the same fresh-init integrity failure
- `v0.1.38` was not promoted because a fresh temp workspace repro still failed
  native `sqlite3 integrity_check`
  (`Tree 50 page 50: free space corruption`)
- stock/pinned `0.1.34` still reproduces the older fresh-init corruption
  signature (`Page ...: never used`) in a fresh temp workspace, so `0.1.34`
  remains "known workaround pin for mutation behavior", not "clean upstream fix"
- [`.beads/beads.db`](/Users/cdilga/Documents/dev/roger-reviewer/.beads/beads.db)
  now passes native `sqlite3` integrity checks again after the documented repair:
  `PRAGMA wal_checkpoint(TRUNCATE); VACUUM; PRAGMA integrity_check;`
- the live workspace currently has 270 issues in
  [`.beads/issues.jsonl`](/Users/cdilga/Documents/dev/roger-reviewer/.beads/issues.jsonl)
  (`open=1`, `closed=269`, `in_progress=0`)
- DB-backed `br doctor` and `br ready` can still fail immediately after the
  repair with `database is busy (snapshot conflict ...)` while a long-lived
  `bv` process keeps the pre-repair snapshot open
- at the time of the 2026-04-12 check, `lsof` showed `bv` as the only live
  holder on `.beads/beads.db`
- JSONL-only queue inspection still works during that state:
  `br ready --no-db` returned `rr-1pz7`
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

- `readlink /Users/cdilga/.local/bin/br` ->
  `/Users/cdilga/.local/bin/br-0.1.34.pinned`
- `br --version` -> `br 0.1.34`
- `curl -fsSL https://api.github.com/repos/Dicklesworthstone/beads_rust/releases/latest`
  -> latest upstream release is `v0.1.38` with asset
     `br-v0.1.38-darwin_arm64.tar.gz`
- `curl -fsSL -o /tmp/br-v0.1.38-darwin_arm64.tar.gz <release-url>` plus
  `curl -fsSL <release-url>.sha256`
  -> downloaded release asset checksum matched published checksum
- `shasum -a 256 /tmp/br-v0.1.38-darwin_arm64/br /Users/cdilga/.local/bin/br-0.1.38.release`
  -> identical hashes; local `br-0.1.38.release` matches the official release
- `git clone https://github.com/Dicklesworthstone/beads_rust.git /tmp/beads_rust-main-test`
  plus `cargo build --release`
  -> built upstream head `6d121a1` (`v0.1.38-5-g6d121a1`)
- `br info`
- `br doctor` before repair
  -> `WARN sqlite.integrity_check ... Page 17: never used ...`
- `sqlite3 .beads/beads.db "PRAGMA wal_checkpoint(TRUNCATE); VACUUM; PRAGMA integrity_check;"`
  -> `0|0|0` then `ok`
- `br doctor` after repair
  -> native `sqlite3.integrity_check` passes, but DB-backed doctor path hits
     `database is busy (snapshot conflict ...)`
- `br ready`
  -> `database is busy (snapshot conflict on pages: page 7864320 > snapshot db_size 374 (latest: 374))`
- `br ready --no-db`
  -> ready queue still visible; returned `rr-1pz7`
- `lsof .beads/beads.db .beads/beads.db-wal .beads/beads.db-shm`
  -> `bv` was the only remaining live holder
- temp repro with `/Users/cdilga/.local/bin/br-0.1.34.pinned` on
  `init -> create -> create -> sqlite3 integrity_check`
  -> `Page 17: never used; Page 57: never used; Page 58: never used; Page 60: never used`
- temp repro with `/Users/cdilga/.local/bin/br-0.1.38.release` on the same
  steps
  -> `Tree 50 page 50: free space corruption`
- temp repro with `/tmp/beads_rust-main-test/target/release/br` on the same
  steps
  -> `Tree 50 page 50: free space corruption`

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
- If DB-backed `br ready`/`show`/`list` start returning `snapshot conflict`
  immediately after repair or checkpoint work, restart long-lived `bv` or other
  DB readers first. Until they release the stale snapshot, use `br ... --no-db`
  only for read-only queue inspection; do not treat `--no-db` as a safe
  mutation path.
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
