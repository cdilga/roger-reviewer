# Verified `br` Repros - 2026-04-01

This document lists only the `br` bugs that were validated from:

- a fresh upstream checkout
- a freshly built binary
- a fresh temporary workspace
- a clean Docker container

If a behavior did not reproduce cleanly in isolation, it is not included here
as a bug-report-ready item.

## Environment notes

- Docker validation is now the primary source of truth for the bugs in this
  document.
- Host-isolated repros were useful during investigation, but the items below
  are included only because they also reproduced in fresh Docker containers.

## Source instances used

### Stock release tag

- repo: `https://github.com/Dicklesworthstone/beads_rust.git`
- clean worktree: `/tmp/beads-rust-upstream-v0134`
- tag: `v0.1.34`
- built binary: `/tmp/br-target-v0134/release/br`

Build command:

```bash
git clone https://github.com/Dicklesworthstone/beads_rust.git /tmp/beads-rust-upstream-fresh-20260401
git -C /tmp/beads-rust-upstream-fresh-20260401 worktree add /tmp/beads-rust-upstream-v0134 v0.1.34
CARGO_TARGET_DIR=/tmp/br-target-v0134 cargo build --release --bin br
```

### Docker helper scripts

- [docker_repro_v0134_fresh_init_corruption.sh](/Users/cdilga/Documents/dev/roger-reviewer/scripts/br-repros/docker_repro_v0134_fresh_init_corruption.sh)
- [docker_repro_v0134_ready_mutates_state.sh](/Users/cdilga/Documents/dev/roger-reviewer/scripts/br-repros/docker_repro_v0134_ready_mutates_state.sh)
- [docker_repro_v0134_foreign_key_parent_child_metadata.sh](/Users/cdilga/Documents/dev/roger-reviewer/scripts/br-repros/docker_repro_v0134_foreign_key_parent_child_metadata.sh)

## Verified bug 1: fresh `br init` creates an integrity-check-failing DB on stock `v0.1.34`

Severity: high

Why this is a real bug:

- it reproduces in a brand-new temp workspace
- no Roger workspace state is involved
- native `sqlite3` integrity check fails immediately after `br init`

Repro:

```bash
tmp=$(mktemp -d /tmp/br-repro-v0134-init.XXXXXX)
cd "$tmp"
/tmp/br-target-v0134/release/br init
sqlite3 .beads/beads.db 'PRAGMA integrity_check;'
```

Observed output:

```text
*** in database main ***
Page 17: never used
```

Extended repro showing the problem worsens after ordinary creates:

```bash
tmp=$(mktemp -d /tmp/br-repro-v0134-seq.XXXXXX)
cd "$tmp"
/tmp/br-target-v0134/release/br init
/tmp/br-target-v0134/release/br create 'one'
/tmp/br-target-v0134/release/br create 'two'
sqlite3 .beads/beads.db 'PRAGMA integrity_check;'
```

Observed output:

```text
*** in database main ***
Page 17: never used
Page 57: never used
Page 58: never used
Page 60: never used
```

Helper script:

- [repro_v0134_fresh_init_corruption.sh](/Users/cdilga/Documents/dev/roger-reviewer/scripts/br-repros/repro_v0134_fresh_init_corruption.sh)
- [docker_repro_v0134_fresh_init_corruption.sh](/Users/cdilga/Documents/dev/roger-reviewer/scripts/br-repros/docker_repro_v0134_fresh_init_corruption.sh)

Docker-validated output on 2026-04-01:

```text
workspace=/tmp/tmp.eyCgxsKbQy
version=br 0.1.34
integrity_check:
*** in database main ***
Page 17 is never used
```

## Verified bug 2: `br ready` performs a write-side repair on the read path

Severity: medium to high

Why this is a real bug:

- it reproduces in a fresh temp workspace
- it proves that a routine read command mutates local DB state
- that behavior plausibly explains lock contention and queue distrust under
  multi-agent use

Repro:

```bash
tmp=$(mktemp -d /tmp/br-repro-readmut.XXXXXX)
cd "$tmp"
BR=/tmp/br-target-v0134/release/br
$BR init
id1=$($BR create 'alpha' --silent)
id2=$($BR create 'beta' --silent)
$BR dep add "$id1" "$id2"
sqlite3 .beads/beads.db "select key,value from metadata where key='blocked_cache_state';"
$BR ready
sqlite3 .beads/beads.db "select key,value from metadata where key='blocked_cache_state';"
```

Observed behavior:

- before `br ready`, metadata contains `blocked_cache_state|stale`
- after `br ready`, that metadata row is gone

That means `br ready` repaired blocked-cache state by writing to the DB during
what users and agents naturally treat as a read-only queue inspection command.

Helper script:

- [repro_v0134_ready_mutates_state.sh](/Users/cdilga/Documents/dev/roger-reviewer/scripts/br-repros/repro_v0134_ready_mutates_state.sh)
- [docker_repro_v0134_ready_mutates_state.sh](/Users/cdilga/Documents/dev/roger-reviewer/scripts/br-repros/docker_repro_v0134_ready_mutates_state.sh)

Docker-validated output on 2026-04-01:

```text
workspace=/tmp/tmp.tV2UHDwbyW
version=br 0.1.34
ids=tmptv2uhdwbyw-iv3,tmptv2uhdwbyw-bty
before:
blocked_cache_state|stale
ready:
📋 Ready work (1 issue with no blockers):

1. [● P2] [task] tmptv2uhdwbyw-bty: beta
after:
```

## Verified bug 3: routine parent-child metadata maintenance can fail with `FOREIGN KEY constraint failed`

Severity: high

Why this is a real bug:

- it reproduces in a fresh temp workspace inside a fresh Docker container
- it does not depend on Roger's long-lived workspace state
- it fails during ordinary create, dependency-add, and metadata-update flows

Repro shape:

```bash
ITERATIONS=120 scripts/br-repros/docker_repro_v0134_foreign_key_parent_child_metadata.sh
```

Observed behavior:

- repeated create/update/parent-child operations succeed for a while
- then a normal parent issue note update fails with:
  `Error: Database error: FOREIGN KEY constraint failed`

Helper script:

- [repro_foreign_key_parent_child_metadata.sh](/Users/cdilga/Documents/dev/roger-reviewer/scripts/br-repros/repro_foreign_key_parent_child_metadata.sh)
- [docker_repro_v0134_foreign_key_parent_child_metadata.sh](/Users/cdilga/Documents/dev/roger-reviewer/scripts/br-repros/docker_repro_v0134_foreign_key_parent_child_metadata.sh)

Docker-validated output on 2026-04-02:

```text
workspace=/tmp/tmp.pyY0hXYzbD
progress=25
progress=50
progress=75
progress=100
FAILED_PARENT_NOTES:109
Error: Database error: FOREIGN KEY constraint failed
```

## Candidate behaviors not yet validated cleanly enough to file

### `br doctor` lock-race false failure

Status: not isolated cleanly enough to file as a separate bug

Why:

- stock `v0.1.34` already has fresh-workspace integrity-check corruption
- that corruption contaminates `doctor` output in a brand-new workspace
- I do not yet have a clean upstream build without the fresh-init corruption
  available in this environment to isolate the lock-specific behavior

Conclusion:

- do not file this as a separate upstream bug yet from this evidence set

## Recommended upstream filing order

1. Fresh-init corruption on stock `v0.1.34`
2. `br ready` mutates DB state on the read path
3. Parent-child metadata maintenance hits `FOREIGN KEY constraint failed`

Those three are clean, isolated, reproducible, and supported by exact steps.
