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

## Candidate behaviors not yet validated cleanly enough to file

### Foreign-key failures during routine maintenance

Status: reproduced in fresh temp workspaces on Roger's current local `br` path;
not yet reconfirmed as an upstream-stock filing candidate in this document.

Repro now carried in repo:

- [repro_foreign_key_parent_child_metadata.sh](/Users/cdilga/Documents/dev/roger-reviewer/scripts/br-repros/repro_foreign_key_parent_child_metadata.sh)
- minimal command:

```bash
ITERATIONS=40 READ_PATH_STRESS=0 scripts/br-repros/repro_foreign_key_parent_child_metadata.sh
```

Observed on 2026-04-01 from this workspace:

- command failed during ordinary metadata update on the parent issue:
  `br update <parent_id> --notes ...`
- error:
  `Database error: FOREIGN KEY constraint failed`

Containment path now available:

- `scripts/swarm/check_beads_trust.sh --mutation-probe --probe-iterations 200`
- this probe fails fast when the mutation sequence hits the FK failure mode

Conclusion:

- this is now a real Roger-local reproducible bug class
- keep upstream filing conservative until the same script is rerun against a
  fresh upstream-stock build in an isolated environment and produces matching
  failure evidence

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

Those two are clean, isolated, reproducible, and supported by exact steps.
