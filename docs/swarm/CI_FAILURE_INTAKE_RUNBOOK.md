## CI Failure Intake Watcher

Roger already requires remote CI failures to become explicit local backlog
ownership. This runbook makes that policy operational.

### What it does

`scripts/swarm/watch_ci_failures.sh` polls failed GitHub Actions runs for this
repo and feeds them through
[`scripts/swarm/ingest_failed_actions_runs.py`](/Users/cdilga/Documents/dev/roger-reviewer/scripts/swarm/ingest_failed_actions_runs.py).

The watcher:

- watches all repo workflows under `.github/workflows/` by default
- creates or updates deduplicated `ci-failure-intake` beads
- stores incremental state under `.beads/ci-failure-intake-state.json` so the
  same run does not rewrite the same bead every poll
- appends configurable follow-up instructions from
  [`.github/ci-failure-intake.json`](/Users/cdilga/Documents/dev/roger-reviewer/.github/ci-failure-intake.json)

### Default command

```bash
./scripts/swarm/watch_ci_failures.sh
```

One-shot mode:

```bash
./scripts/swarm/watch_ci_failures.sh --once
```

Dry-run mode:

```bash
./scripts/swarm/watch_ci_failures.sh --once --dry-run
```

### Config

The default config file is:

- [`.github/ci-failure-intake.json`](/Users/cdilga/Documents/dev/roger-reviewer/.github/ci-failure-intake.json)

Supported knobs:

- `parent_id`: parent bead for new CI intake work, or `none`
- `labels`: labels applied to created intake beads
- `workflow_prefixes`: workflow path prefixes to ingest
- `instructions_md`: Markdown appended to the bead description so workers know
  the required follow-up protocol

### Triggering behavior

`validation-nightly` now runs on:

- push to `main`
- scheduled nightly cron
- manual dispatch

The workflow is also debounced:

- it uses a workflow concurrency group keyed by branch
- `cancel-in-progress: false` means the active run is preserved, but GitHub
  collapses stale queued runs in the same concurrency group so the latest queued
  follow-up wins

This is intentional. The goal is to get feedback shortly after pushes to
`main` without piling up obsolete heavyweight runs or burning runner minutes on
sleep-only debounce steps.
