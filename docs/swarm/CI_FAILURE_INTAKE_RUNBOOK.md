## CI Failure Intake Watcher

Roger already requires remote CI failures to become explicit local backlog
ownership. This runbook makes that policy operational.

### What it does

`scripts/swarm/watch_ci_failures.sh` polls failed GitHub Actions runs for this
repo and feeds them through
[`scripts/swarm/ingest_failed_actions_runs.py`](../../scripts/swarm/ingest_failed_actions_runs.py).

The watcher:

- watches all repo workflows under `.github/workflows/` by default
- creates or updates deduplicated `ci-failure-intake` beads
- stores incremental state under `.beads/ci-failure-intake-state.json` so the
  same run does not rewrite the same bead every poll
- sanitizes untrusted GitHub run text before it becomes bead description, notes,
  or Agent Mail body content, and quarantines suspicious prompt-like fields
- appends configurable follow-up instructions from
  [`.github/ci-failure-intake.json`](../../.github/ci-failure-intake.json)
- sends Agent Mail notifications on topic `ci-failure` for create/update events
  so active agents see failures without waiting for a manual `br ready`

### Default command

```bash
./scripts/swarm/watch_ci_failures.sh
```

Preferred operational bootstrap:

```bash
./scripts/swarm/ensure_ci_failure_watch.sh
```

That script keeps the watcher in a dedicated tmux session named
`ci-failure-watch`.

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

- [`.github/ci-failure-intake.json`](../../.github/ci-failure-intake.json)

Supported knobs:

- `parent_id`: parent bead for new CI intake work, or `none`
- `labels`: labels applied to created intake beads
- `workflow_prefixes`: workflow path prefixes to ingest
- `instructions_md`: Markdown appended to the bead description so workers know
  the required follow-up protocol
- `agent_mail.enabled`: turn Agent Mail broadcast on or off
- `agent_mail.sender_name`: stable adjective+noun identity used by the watcher
- `agent_mail.active_within_minutes`: only notify agents seen active within
  this window unless explicit `recipients` are configured
- `agent_mail.recipients`: optional explicit recipient list; leave empty to
  auto-target recent active agents
- `agent_mail.topic`: topic tag used for the failure announcement

### Sanitization behavior

The watcher treats GitHub run metadata as untrusted text.

- control characters and multiline payloads are normalized before persistence
- suspicious prompt-like content is quarantined instead of copied through
- bead notes record `sanitization_reasons` and `quarantined_fields` when this
  happens so agents can inspect the intake truthfully
- dedupe still keys off workflow path, ref, and event; sanitization does not
  create duplicate intake beads

### Triggering behavior

`validation-main` is now a manual operator workflow only:

- manual dispatch selects `gated`, `nightly`, or `release`

The workflow uses a single branch-scoped concurrency group with
`cancel-in-progress: false`, so the active run is preserved while stale queued
follow-ups are collapsed.

### Swarm integration

This watcher is independent of the `ntm` operator cockpit. If you are running a
swarm, start it explicitly in parallel with your `ntm spawn ...` session rather
than depending on repo-local launch wrappers.

Manual checks:

```bash
./scripts/swarm/ensure_ci_failure_watch.sh --status
ntm status roger-reviewer
tmux attach -t ci-failure-watch
```
