Read `AGENTS.md`, then `README.md`.

If Codex asks whether to trust this directory, choose `Yes, continue` immediately and proceed.
If Agent Mail is reachable but says the project or your identity is missing, bootstrap it before claiming work:

- call `ensure_project` for the current repo path
- register yourself with Agent Mail using the stable pane identity already attached to this session
- then continue with the normal worker loop

Before claiming your first bead, re-anchor on
`docs/PLAN_FOR_ROGER_REVIEWER.md` enough to confirm the current phase,
authority order, architecture direction, and support-claim truthfulness model.
Do not spend your first turn on a full line-by-line plan read while the live
queue is waiting, but do not treat the canonical plan as optional context.
Once you choose a bead, read the relevant plan sections and `br show <id>` in
full.

If you are shaping beads, writing prompts, or resuming after compaction, also
read `docs/beads/BEAD_AND_PROMPT_FAILURE_PATTERNS.md`.

Implementation is active. Work from live queue truth (`br ready`), not launcher hints.

## Worker Loop (short form)

1. Check Agent Mail first and acknowledge any `ack_required` messages.
2. Run `br ready`.
3. Inspect candidate with `br show <id>`.
4. Claim with `br update <id> --status in_progress`.
5. Reserve files with Agent Mail before editing.
6. Announce claim + reserved files in Agent Mail.
7. Finish the bead truthfully. Meet the acceptance criteria, but do not stop
   mechanically if honest closeout also requires a missing child bead,
   dependency correction, support-claim correction, or another adjacent bounded
   slice. Complete it or bead it immediately.
8. Run the required validation layer and record exact command/suite in bead close reason or notes.
9. For CI-sensitive beads (labels `ci`/`github-actions`/`release`/`publish`), record remote run evidence before close:
   - `scripts/swarm/check_ci_closeout_evidence.sh --bead <id> --run-url <url> --outcome <outcome>`
   For non-CI-sensitive beads, local-only evidence is allowed with:
   - `scripts/swarm/check_ci_closeout_evidence.sh --bead <id> --local-only-reason "<reason>"`
10. Close bead and run `br sync --flush-only` after bead state/note changes.

If `br` reports `database is busy`, back off and retry.
For scripted or bulk mutation paths (`create`/`update`/`close`/`sync`), prefer
`./scripts/swarm/br_pinned.sh ...` over raw `br ...`; the wrapper serializes
mutating calls behind a repo-local advisory lock and injects a longer
`--lock-timeout` unless one is already set.
If normal `br` queue commands still fail after a few retries, use the direct fallback path for queue truth and claiming:

- `br ready --no-daemon`
- `br show <id> --no-daemon`
- `br update <id> --status in_progress --no-daemon`

Treat the first clean `--no-daemon` result as authoritative rather than idling on lock contention.
When scripting pure queue inspection (`ready/list/show`), prefer read-safe flags `--no-auto-import --no-auto-flush` to minimize hidden write work during contention.
If `br ready` is empty but useful work exists, run `./scripts/swarm/audit_bead_batch.sh --limit 20 --strict` and follow its queue-repair playbook.

After compaction or any long interruption, re-read `AGENTS.md`, reopen the
relevant plan sections, and re-check `br ready` before acting. Do not resume
from memory alone.

## Non-negotiables

- Preserve Roger approval safety: no automatic GitHub posting and no direct GitHub write bypasses.
- Do not use a PR-based development workflow for this swarm run. Work directly in the checked-out repo/worktree using beads, local commits, and the current branch unless the user explicitly asks for branches or PRs.
- Do not open, update, or manage GitHub pull requests for your own swarm work. No `gh pr`, no PR creation, no PR review/comment workflow, and no "I'll open a PR next" closeout language unless the user explicitly redirects you there.
- Do not mutate external/dev/test environments without explicit user authorization.
- Keep Agent Mail + file reservations in sync with real work.
- Use Frankenterm (`ft`) as the observer default when available; if absent, install via `scripts/swarm/install_frankenterm.sh` or declare explicit degraded `--no-ft` mode.
- Use `rch exec -- <command>` for CPU-heavy cargo tasks when available. If no worker fleet is configured, local fail-open execution is still acceptable; do not wait for remote capacity that does not exist.

## Authority Links

- Worker doctrine (long form): `docs/swarm/worker-operating-doctrine.md`
- Operator cockpit guidance: `docs/swarm/NTM_OPERATOR_GUIDE.md`
- Canonical authority order and safety rules: `AGENTS.md`
