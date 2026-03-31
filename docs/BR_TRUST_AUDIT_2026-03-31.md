# br Trust Audit - 2026-03-31

This document records the current reasons Roger should not blindly trust `br`
yet, even after the local patched binary install. It separates:

1. the intended `br` contract from upstream planning and operator docs
2. the bugs and trust failures observed locally in Roger
3. the recurring pain surfaced in Cass agent history

The goal is to stop relitigating whether the problem is "real" and instead
track a finite remediation set.

## Current Roger workspace truth

The Roger workspace does not currently show evidence of literal row loss.

- `.beads/beads.db` issue count: `128`
- `.beads/issues.jsonl` line count: `128`
- DB vs JSONL missing IDs: `0`
- DB vs JSONL status mismatches: `0`
- Open issues: `19`
- Closed issues: `109`

So the immediate problem is not "proof that beads vanished from storage". The
problem is that `br` has repeatedly behaved in ways that make operators and
agents unable to trust queue truth, diagnostics, and mutation safety.

## Intended `br` contract

From upstream planning and docs under `/tmp/beads-rust-investigate/`, `br`
claims all of the following:

- Non-invasive by default
  - no daemon
  - no git hooks
  - no automatic git commands
- Local-first SQLite + JSONL hybrid with SQLite as source of truth
- Safe sync behavior with strict path guards and atomic export/import
- Agent-friendly machine surfaces
  - `--json`
  - `--robot`
  - stable machine-readable output
- Deterministic enough for automation
- Trustworthy diagnostics via `br doctor`
- Concurrency tolerance via WAL mode and busy timeout
- Safe read-side behavior for routine agent flows like `ready`, `list`, and `show`

The relevant upstream documents making those claims are:

- `PLAN_TO_PORT_BEADS_WITH_SQLITE_AND_ISSUES_JSONL_TO_RUST.md`
- `PROPOSED_ARCHITECTURE_FOR_BR_USING_RUST_BEST_PRACTICES.md`
- `docs/ARCHITECTURE.md`
- `docs/AGENT_INTEGRATION.md`
- `docs/TROUBLESHOOTING.md`
- `README.md`
- `AGENT_FRIENDLINESS_REPORT.md`

## Audit findings

### 1. Fresh-workspace initialization was corrupting DBs on released and current upstream builds

Observed locally:

- fresh `br init` on upstream `0.1.34` produced SQLite integrity-check failure
- current upstream `main` reproduced the same failure
- the symptom was `Page 17: never used`

Root cause found in upstream source:

- `apply_schema()` runs `run_migrations()` before setting `PRAGMA user_version`
- on a fresh DB, `user_version` is still `0`
- `run_migrations()` therefore executes legacy v3/v4 migration logic on a brand-new schema
- that path drops and recreates `idx_issues_ready`
- on current `fsqlite`, the old root page is leaked instead of reclaimed

Impact:

- upstream release trust is broken
- a clean install can start from a malformed DB
- any operator claim that stock `0.1.34` is safe for Roger is false

Current local status:

- locally contained by the patched `~/.local/bin/br`
- not fixed upstream by the installed release alone

### 2. Read commands are not actually cheap or predictably read-only

Upstream source shows that many read-style commands open storage through
`open_storage_ctx_with_auto_import()`, which can mutate local state during the
read path. Separately, blocked-cache freshness is repaired lazily on reads via
`ensure_blocked_cache_fresh()`, which acquires a write transaction.

This means common reads are not truly "just reads":

- `ready`
- `list`
- `show`
- other read-style commands using the same storage opening path

Impact:

- hidden write work happens during ordinary queue inspection
- lock contention becomes normal rather than exceptional
- operators cannot assume a failed read means "no data" rather than "read path tried to mutate"

### 3. Lock contention is common enough that Roger swarm doctrine had to normalize it

Cass evidence and repo-local doctrine both show this pattern:

- agents were explicitly told not to treat `database is busy` as "no work exists"
- retry loops were baked into swarm instructions and preflight logic

That is evidence of a control-plane bug, not a normal steady-state UX.

Impact:

- `br ready` is not cheap enough to be a trustworthy queue oracle under swarm load
- agents burn time retrying queue inspection before doing real work
- queue emptiness and queue unavailability become hard to distinguish

### 4. Upstream tests themselves encode that default reads are unsafe under contention

The upstream concurrency suite repeatedly uses `--no-auto-import` for post-write
reads specifically to avoid `SYNC_CONFLICT` after concurrent flushes.

That is direct evidence that the default read path is not safe enough for
multi-actor use.

Examples called out in upstream tests:

- verify state with `--no-auto-import` after concurrent writes
- use `--no-auto-import` after contention to avoid `SYNC_CONFLICT`
- use `--no-auto-import` for post-contention reads to keep the workspace readable

Impact:

- the default operator path is weaker than the test harness already knows
- multi-agent automation must know hidden escape hatches to remain stable

### 5. `br doctor` is not consistently trustworthy enough to be the single source of health truth

Observed historically and locally:

- earlier Roger incidents showed `br doctor` reporting clean state while mutation operations still hit `FOREIGN KEY constraint failed`
- current runs can be noisy because recovery artifacts are preserved indefinitely
- a recent local run showed `br doctor` passing internal checks, then exiting nonzero because the external `sqlite3` integrity probe hit a transient lock

Impact:

- "doctor says OK" is not sufficient proof that the workspace is healthy
- automation can fail because the diagnostic path races on the same DB it is checking
- preserved recovery artifacts blur the difference between evidence retention and active breakage

### 6. Parent-child and mutation integrity are still not trustworthy enough

Historical Roger evidence:

- `rr-04u` existed because parent-child closure logic disagreed with actual child state and force-close attempts hit foreign-key errors
- Cass memory includes repeated `FOREIGN KEY constraint failed` incidents during routine bead maintenance

Current local evidence:

- `br update rr-2tq.1 --acceptance-criteria ...` failed with `FOREIGN KEY constraint failed` immediately after the child issue was created

Impact:

- routine metadata updates can fail for reasons unrelated to user intent
- parent/child maintenance remains a trust-eroding surface
- "create worked, update failed" is not an acceptable automation contract

### 7. Workspace trust has depended on manual repair and local operator folklore

Cass memory and repo history show repeated dependence on:

- `VACUUM`
- `wal_checkpoint(TRUNCATE)`
- manual DB/JSONL comparison
- broken-path repair
- re-reading `AGENTS.md` for caveats about `database is busy`

Impact:

- the system is learnable only through tribal knowledge
- swarm throughput drops because agents must be partial DB janitors
- queue truth depends on human/operator repair judgment too often

### 8. The machine/agent contract is stronger in prose than in practice

Upstream agent-facing materials still admit gaps:

- command output consistency is not fully uniform
- some machine surfaces are static artifacts rather than live self-description
- output envelopes are not consistently `{data, metadata, errors}`

This is less severe than corruption or lock contention, but it matters because
`br` is explicitly sold as an agent-first primitive.

## Cass-derived bug memory

Cass memory does not prove every incident root cause, but it does show stable
recurring failure patterns in real agent use:

- lock contention was common enough that retry behavior had to be documented in swarm instructions
- agents observed bead-state inconsistencies such as issues reverting state or parent/child state disagreeing
- agents repeatedly suspected DB/JSONL desynchronization after normal `br` usage
- agents saw `FOREIGN KEY constraint failed` during routine bead maintenance while `br doctor` was not a sufficient early warning
- repair flow often meant "run sqlite3 repair, then see if queue truth looks sane again"

Those memories align with the local source/repro evidence above rather than
contradicting it.

## Consolidated bug list

This is the remediation inventory Roger should track.

1. Upstream fresh-init migration ordering corrupts new DBs on stock `0.1.34` and current upstream `main`.
2. Read commands hide write behavior through auto-import and lazy blocked-cache rebuild.
3. Lock contention is common enough to break the trustworthiness of `br ready` as a queue oracle.
4. Default read flows are not robust under multi-actor concurrency without `--no-auto-import`.
5. `br doctor` is not trustworthy enough as a sole health signal under contention and historical corruption.
6. Parent-child / metadata updates can still hit foreign-key failures during ordinary maintenance.
7. Recovery artifacts and repair expectations create ongoing operator noise and ambiguous health status.
8. The machine-facing contract is not fully consistent with the agent-first promise.

## Recommended remediation shape

Treat this as a real dependency-hardening program, not a single bug.

Recommended lanes:

1. Roger-side trust tools
   - add a canonical DB-vs-JSONL parity check
   - add an operator-grade queue-trust report before swarm launch
2. Local `br` fork hardening
   - keep the fresh-init corruption fix
   - reduce or eliminate hidden writes on read paths
   - harden mutation integrity for parent/child updates
3. Diagnostic hardening
   - make `doctor` distinguish transient lock contention from actual corruption
   - downgrade or better classify preserved recovery-artifact noise
4. Upstream patch set
   - prepare a concrete issue/patch series rather than reporting a vague "br feels broken"

## Bottom line

Roger does not currently have evidence that the beads workspace literally lost
issues. But it does have enough evidence to conclude that `br` still violates
its intended trust contract in several ways, especially around fresh-init
correctness, hidden write behavior on reads, lock contention, and mutation or
diagnostic reliability.
