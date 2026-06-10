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
Once you choose a bead, read the relevant plan sections and
`br show <id>` in full.

If you are shaping beads, writing prompts, or resuming after compaction, also
read `docs/beads/BEAD_AND_PROMPT_FAILURE_PATTERNS.md`.

Implementation is active. Work from live queue truth
(`br ready`), not launcher hints.
If an operator nudge or pane-specific reminder names one bead, that sets the
current priority only. It does not mean you are permanently restricted to that
single bead for the rest of the session. Own one bead at a time, finish or
truthfully block it, then continue to the next safe bead from the live queue.

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
8. Add or update the cheapest truthful automated tests for the changed behavior.
   Default to unit or parameterized tests. Escalate to narrow integration tests
   for real boundaries such as storage, migrations, adapters, CLI/TUI
   controller seams, prompt execution, and bridge envelopes. Use manual smoke
   only when the governing docs explicitly make it the right proof layer.
9. Run the required validation layer and record exact command/suite in bead close reason or notes.
10. For CI-sensitive beads (labels `ci`/`github-actions`/`release`/`publish`), record remote run evidence before close:
   - `scripts/swarm/check_ci_closeout_evidence.sh --bead <id> --run-url <url> --outcome <outcome>`
   For non-CI-sensitive beads, local-only evidence is allowed with:
   - `scripts/swarm/check_ci_closeout_evidence.sh --bead <id> --local-only-reason "<reason>"`
11. Do not close an implementation bead if no new or updated tests landed and no explicit no-test rationale was recorded. If the missing proof is still one truthful slice, land it now; otherwise create or claim the testing follow-on bead before closeout.
12. Close bead and run `br sync --flush-only` after bead state/note changes.
13. In a persistent tmux pane, do not stop after validation or closeout. Treat green tests and truthful closure as one checkpoint, then immediately re-check Agent Mail, rerun `br ready`, and claim the next safe bead.

If `br` reports `database is busy`, back off and retry. Default to
`br ...` for both common queue reads and explicit mutations.
If `br` reports degraded trust, it will fall back to `--no-db` queue
inspection for `ready/list/show/blocked` automatically. Do not use `--no-db`
for mutation paths.
If native trust is degraded, repair with:

```bash
./scripts/swarm/rebuild_beads_db_safe.sh --install
```

If `br ready` is empty but useful work exists, run
`./scripts/swarm/audit_bead_batch.sh --limit 20 --strict` and follow its
queue-repair playbook.

After compaction or any long interruption, re-read `AGENTS.md`, reopen the
relevant plan sections, and re-check `br ready` before
acting. Do not resume
from memory alone.

If this pane is part of a persistent interactive tmux swarm session, do not
stop after a single checkpoint. After each useful checkpoint, immediately
re-check Agent Mail, rerun `br ready`, verify the next
candidate with `br show <id>`, claim it, and keep
going. Only stop when the queue is
genuinely exhausted for you, a real blocker prevents more progress, or the
user explicitly redirects you.

If this prompt is being used in a headless one-shot launcher instead, stop
cleanly after a durable checkpoint and let the outer launcher re-invoke you.

## Testing Bar

- almost every implementation bead should add or update tests
- default to `unit` first, then narrow `integration`, and reach for `e2e` only
  when the budgeted multi-surface journey itself is under test
- treat testing as the proof stage inside a continuing work loop, not as a reason
  to end the pane's run once one bead goes green
- do not use docs, metadata, or manual smoke as a substitute for deterministic
  unit/integration coverage when that cheaper proof is feasible
- if the test story is unclear, reread `docs/TESTING.md`,
  `docs/TEST_HARNESS_GUIDELINES.md`, and
  `docs/TEST_EXECUTION_TIERS_AND_E2E_BUDGET.md` before closing
- if a missing integration or E2E proof is the real remaining gap, do not
  imply it exists; implement it if still one truthful slice or bead it
  immediately

## Non-negotiables

- Preserve Roger approval safety: no automatic GitHub posting and no direct GitHub write bypasses.
- Do not use a PR-based development workflow for this swarm run. Work directly in the checked-out repo/worktree using beads, local commits, and the current branch unless the user explicitly asks for branches or PRs.
- Unrelated in-progress changes elsewhere in the worktree are not, by
  themselves, a reason to avoid a local commit. If your owned slice can be
  staged cleanly with path-specific or hunk-specific adds, do that and commit
  the validated slice. Only leave work uncommitted when there is real hunk
  overlap, missing validation, or explicit user direction not to commit.
- Do not open, update, or manage GitHub pull requests for your own swarm work. No `gh pr`, no PR creation, no PR review/comment workflow, and no "I'll open a PR next" closeout language unless the user explicitly redirects you there.
- Do not mutate external/dev/test environments without explicit user authorization.
- Keep Agent Mail + file reservations in sync with real work.
- Use Frankenterm (`ft`) as the observer default when available; if absent, install via `scripts/swarm/install_frankenterm.sh` or declare explicit degraded `--no-ft` mode.
- Use `rch exec -- <command>` for CPU-heavy cargo tasks when available. If no worker fleet is configured, local fail-open execution is still acceptable; do not wait for remote capacity that does not exist.

## Authority Links

- Worker doctrine (long form): `docs/swarm/worker-operating-doctrine.md`
- Operator cockpit guidance: `docs/swarm/HUMAN_OPERATOR_FLYWHEEL_GUIDE.md`
- Canonical authority order and safety rules: `AGENTS.md`
