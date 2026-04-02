Read `AGENTS.md`, then `README.md`.

Before claiming your first bead, skim `docs/PLAN_FOR_ROGER_REVIEWER.md` only far enough to confirm the current phase, architecture direction, and safety model. Do not spend your first turn on a full line-by-line plan read while the live queue is waiting. Once you choose a bead, read the relevant plan sections and `br show <id>` in full.

Implementation is active. Work from live queue truth (`br ready`), not launcher hints.

## Worker Loop (short form)

1. Check Agent Mail first and acknowledge any `ack_required` messages.
2. Run `br ready`.
3. Inspect candidate with `br show <id>`.
4. Claim with `br update <id> --status in_progress`.
5. Reserve files with Agent Mail before editing.
6. Announce claim + reserved files in Agent Mail.
7. Implement exactly to acceptance criteria (no extra scope).
8. Run the required validation layer and record exact command/suite in bead close reason or notes.
9. Close bead and run `br sync --flush-only` after bead state/note changes.

If `br` reports `database is busy`, back off and retry.
If normal `br` queue commands still fail after a few retries, use the direct fallback path for queue truth and claiming:

- `br ready --no-daemon`
- `br show <id> --no-daemon`
- `br update <id> --status in_progress --no-daemon`

Treat the first clean `--no-daemon` result as authoritative rather than idling on lock contention.
When scripting pure queue inspection (`ready/list/show`), prefer read-safe flags `--no-auto-import --no-auto-flush` to minimize hidden write work during contention.
If `br ready` is empty but useful work exists, run `./scripts/swarm/audit_bead_batch.sh --limit 20 --strict` and follow its queue-repair playbook.

## Non-negotiables

- Preserve Roger approval safety: no automatic GitHub posting and no direct GitHub write bypasses.
- Do not mutate external/dev/test environments without explicit user authorization.
- Keep Agent Mail + file reservations in sync with real work.
- Use Frankenterm (`ft`) as the observer default when available; if absent, install via `scripts/swarm/install_frankenterm.sh` or declare explicit degraded `--no-ft` mode.
- Use `rch exec -- <command>` for CPU-heavy cargo tasks when available.

## Authority Links

- Worker doctrine (long form): `docs/swarm/worker-operating-doctrine.md`
- Operator runbook: `docs/OVERNIGHT_SWARM_RUNBOOK.md`
- Canonical authority order and safety rules: `AGENTS.md`
