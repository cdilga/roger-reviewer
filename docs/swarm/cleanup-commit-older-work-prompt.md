Read `AGENTS.md` and `README.md`, check Agent Mail, then inspect `git status --short`.

This pass is for cleaning up and committing older repo work, with freedom to split the work sensibly across the swarm. Coordinate in Agent Mail, claim non-overlapping slices, reserve files, and turn validated older work into logical local commits. Do not push.

One worker should assess stale backup cleanup, especially:

- `.beads/.manual_repair_*`
- `.beads/.metadata_repair_backups`
- `.beads/beads.db.pre_*`
- `.beads/beads.db.corrupt.bak`
- `.beads/issues.jsonl.pre_flush_repair_*`
- `.roger.backup_v9/`
- `findings_backup_v9.json`
- `review_sessions_backup_v9.json`

Do not delete blindly. Verify whether an artefact is still needed by current scripts/docs/runbooks. If it is stale, remove it and commit that cleanup. If it should stay, say why and move on.

Everyone else: find coherent older tracked/untracked work, validate it truthfully, and commit owned slices with detailed messages. After each checkpoint, check Agent Mail again and keep moving until the repo is materially cleaner.
