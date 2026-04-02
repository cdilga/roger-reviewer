# rr-1ab.4 Fresh-Init Fix Carry and Upstream Prep (2026-04-01)

This note is the `rr-1ab.4` closeout artifact. It captures the exact matrix
used to prove the fresh-init integrity regression is still present in stock
`0.1.34`-line binaries and absent in the local fixed binary.

## Goal

1. Keep Roger's default `br` path on the known-good local fix.
2. Preserve a copy-pastable repro matrix for upstream follow-up
   (`Dicklesworthstone/beads_rust#213`).

## Local carry action taken

Detected default-path drift:

- before: `/Users/cdilga/.local/bin/br -> /Users/cdilga/.local/bin/br-0.1.34.pinned`
- after: `/Users/cdilga/.local/bin/br -> /Users/cdilga/.local/bin/br-0.1.34.localfix`

Command used:

```bash
ln -sfn /Users/cdilga/.local/bin/br-0.1.34.localfix /Users/cdilga/.local/bin/br
```

## Repro matrix run

Shared repro steps:

```bash
git init -q
<br-bin> init
<br-bin> create --title "repro one"
<br-bin> create --title "repro two"
sqlite3 .beads/beads.db "PRAGMA integrity_check;"
<br-bin> doctor
```

### A) stock release-line binary (`/Users/cdilga/.local/bin/br-0.1.34.bak`)

- version: `br 0.1.34`
- result: reproducible integrity warnings
- `sqlite3` output:
  `Page 17: never used; Page 57: never used; Page 58: never used; Page 60: never used`
- `br doctor` output includes:
  `WARN sqlite.integrity_check: database disk image is malformed: page 17 is never used`

### B) pinned binary that temporarily became default (`/Users/cdilga/.local/bin/br-0.1.34.pinned`)

- version: `br 0.1.34`
- result: same regression as stock `0.1.34`
- `sqlite3` output:
  `Page 17: never used; Page 57: never used; Page 58: never used; Page 60: never used`
- `br doctor` output includes:
  `WARN sqlite.integrity_check: database disk image is malformed: page 17 is never used`

### C) local fixed binary (`/Users/cdilga/.local/bin/br-0.1.34.localfix`)

- version: `br 0.1.34`
- result: no integrity-check failure in the same matrix
- `sqlite3` output: `ok`
- `br doctor` output: `OK sqlite.integrity_check` and `OK sqlite3.integrity_check`

## Upstream handoff package

Use this minimum package against `Dicklesworthstone/beads_rust#213`:

1. Fresh-workspace repro commands from this note.
2. Matrix result summary: stock `0.1.34` repro-bad, pinned `0.1.34` repro-bad,
   localfix `0.1.34` repro-good.
3. Root-cause summary from Roger trust docs: fresh-schema bootstrap runs legacy
   migration path before `PRAGMA user_version` is set on truly fresh DBs.
4. Regression guard requirement: add an automated fresh-init + sequential-create
   integrity-check test so this class of corruption cannot silently regress.
