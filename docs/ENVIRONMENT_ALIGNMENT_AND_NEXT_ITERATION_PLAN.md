# Environment Alignment And Next Iteration Plan

Status: active recovery support plan
Class: implementation support plan / launch-gate packet
Audience: maintainers and agents aligning local and devbox before the next Roger recovery iteration

Authority:

- `AGENTS.md` remains the operational authority.
- `docs/DEV_MACHINE_ONBOARDING.md` remains the machine setup guide.
- `docs/SELF_EXPLORATION_AND_REVIEW_WORKBENCH_RECOVERY_PLAN.md` remains the product recovery packet.
- This document adds the environment and swarm launch gate required before the next implementation iteration.

## Purpose

The next Roger iteration should not begin from a split environment. Local macOS
and `ssh devbox` currently disagree about Git history, bead state, shell command
resolution, Agent Mail usability, and swarm readiness. This plan defines the
convergence work required before agents start coding on the recovery epic.

This is not a general machine cleanup project. It is a launch-gate plan for the
specific goal of making Roger self-explorable again: reliable extension launch
and resume, exact-session terminal handoff, session picker, TUI status/memory
visibility, GitHub-context local authoring, and final-submission capture.

## Adversarial Round Summary

On 2026-06-28, an NTM adversarial round was launched as
`roger-reviewer--env-adversary` with four read-only Codex panes:

- Environment Skeptic
- Product Recovery Advocate
- Remote Devbox Realist
- Beads/Swarm Execution Critic

Agent Mail was not usable for coordination during the round. MCP entries were
visible, but project/thread operations failed with database errors locally, and
devbox did not have a running Agent Mail HTTP endpoint.

The agents disagreed on scope, but converged on the same launch decision:

- do not launch an implementation swarm yet
- align Git and bead truth first
- repair Agent Mail as a real coordination surface, not just an MCP config entry
- normalize devbox shell/PATH behavior so `br`, `rr`, and `ntm` resolve in the
  same operational contexts agents will use
- make the pre-swarm audit and launch gate green before touching product beads

## Current Evidence

### Local macOS

- repo: `/Users/cdilga/Documents/dev/roger-reviewer`
- branch: `main`
- head: `14a29753e64a76dd3cec804426a5021e721d10d6`
- relation: `main` matches `origin/main`
- dirty tracked paths include `.beads/issues.jsonl`, `AGENTS.md`,
  `apps/extension/testing/package-lock.json`, and
  `scripts/extension/validate_manifest.sh`
- untracked paths include
  `docs/SELF_EXPLORATION_AND_REVIEW_WORKBENCH_RECOVERY_PLAN.md` and
  `scripts/extension/test_validate_manifest.sh`
- `check_beads_trust.sh` passes with 552 records and 19 open beads
- `br doctor` reports a recoverable workspace but exits failed because of
  warnings including duplicate `br` on PATH, stale `.beads/.write.lock`, old
  recovery artifacts, and missing `beads.base.jsonl`
- `./scripts/swarm/br_safe.sh --print-path` resolves to
  `~/.local/bin/br-main.current`
- Agent Mail MCP tools were visible to this session but failed on
  `ensure_project` / `macro_start_session`

### Devbox

- host: `ssh devbox`
- repo: `/home/ubuntu/dev/roger-reviewer`
- branch: `main`
- head: `cef35113fc3f90221710f6d5f8ff798bda5ba1e2`
- relation: `main` is ahead of `origin/main` by 7 commits
- local versus devbox history differs: local has 8 commits not on devbox, devbox
  has 7 commits not on local
- dirty tracked paths include `.beads/issues.jsonl`, `.gitignore`, and
  `apps/extension/testing/package-lock.json`
- untracked environment/local-state paths include provider MCP config files and
  `storage.sqlite3*`
- repo-local Rust resolves to `nightly-2026-06-10`
- raw non-login `ssh devbox 'command -v br rr ntm'` does not see the tools
- login-shell probes can see `br`, `rr`, and `ntm`
- `~/.local/bin/br -> ~/.local/bin/br-0.2.15.pinned`
- `./scripts/swarm/br_safe.sh --print-path` resolves to `~/.local/bin/br`, not
  the documented `br-main.current` managed path
- Codex auth and `planning-workflow` skill are present
- Agent Mail MCP is configured in Codex, but the local Agent Mail HTTP endpoint
  is not running and notify hooks are missing

## Launch Gate

No implementation swarm should work on the Roger recovery epic until all of the
following are true.

1. Git authority is explicit.
   The operator chooses the intended source of truth for local/devbox divergence.
   No agent guesses whether local `14a2975`, devbox `cef3511`, or a reconciled
   branch is authoritative.

2. Dirty worktree inventories are captured.
   Both machines have read-only inventories of tracked changes, untracked files,
   worktrees, branch tracking, and bead counts before any repair.

3. Bead truth is aligned or intentionally scoped.
   If both machines will participate in the next iteration, they must report the
   same `.beads` truth and pass the same queue checks. If devbox is read-only
   reference only, the plan must say so.

4. `br doctor` and queue-trust checks are green enough for mutation.
   `check_beads_trust.sh` passing is not sufficient by itself when `br doctor`
   still fails with operator-actionable warnings.

5. Shell command resolution is stable.
   On devbox, the exact command contexts used by agents must resolve `br`, `rr`,
   `ntm`, `codex`, `cargo`, and `rustup` predictably. If automation uses a login
   shell, runbooks must say that. If it uses raw SSH commands, raw SSH must work.

6. Agent Mail works end to end.
   A green `codex mcp list` is not enough. The gate requires project ensure,
   agent registration, inbox fetch, thread send, and thread read.

7. NTM surfaces are reliable enough for operation.
   `ntm health`, `ntm status`, `ntm activity`, and the repo observer should not
   materially disagree about whether panes are working, idle, or broken. If they
   do, the operator guide must identify the authoritative surface for this run.

8. The ready recovery frontier has structured acceptance.
   Recovery beads that will be assigned to agents need explicit acceptance and
   validation contracts, not only prose descriptions. The strict pre-swarm bead
   audit should pass or produce only consciously accepted warnings.

## Repair Sequence

### Phase 0: Freeze And Compare

This phase is read-only.

Required outputs:

- local and devbox `git status --short --branch`
- local and devbox `git rev-parse HEAD`
- local and devbox `git branch -vv`
- local and devbox `git worktree list --porcelain`
- local and devbox untracked-file inventory
- local and devbox `br info`
- local and devbox `check_beads_trust.sh`
- local and devbox `br doctor`
- local and devbox command-resolution matrix for login and non-login shells
- Agent Mail endpoint and MCP tool-call proof

### Phase 1: Choose Authority

The operator chooses one of these paths:

- Local-first: local `origin/main` plus current recovery-plan/bead work is the
  authority; devbox commits become a branch or patch queue for review.
- Devbox-first: devbox's ahead commits are authoritative and local recovery
  planning is replayed on top.
- Reconcile branch: create a dedicated reconciliation branch that merges both
  sides and proves the combined state before either machine continues.

No destructive command is allowed as part of this choice.

### Phase 2: Repair Control Plane

Repair Agent Mail before relying on multi-agent coordination.

Required outcomes:

- local Agent Mail database schema is current enough for project/thread calls
- devbox Agent Mail HTTP server is running when Codex expects it
- devbox user-level notify hooks match `docs/DEV_MACHINE_ONBOARDING.md`
- `macro_start_session`, inbox fetch, thread send, and thread read work locally
  and on devbox when devbox is participating
- NTM mail surfaces either work or honestly fail closed with clear guidance

### Phase 3: Normalize Tool Front Doors

Normalize command resolution before running agents.

Required outcomes:

- `br`, `rr`, `ntm`, `codex`, `cargo`, and `rustup` resolve in the shell context
  used by automation
- devbox raw SSH versus login shell behavior is intentional and documented
- `br_safe.sh --print-path` agrees with the intended managed-path contract on
  every participating machine
- duplicate or misleading PATH entries are reduced or documented with a clear
  precedence rule

### Phase 4: Align Git And Beads

Do not run `git reset`, `git clean`, destructive checkout, or live rsync until
the operator approves the authority decision and the dirty inventories are saved.

Required outcomes:

- both participating machines share the chosen Git authority or one machine is
  explicitly demoted to read-only reference
- `.beads/issues.jsonl` and `.beads/beads.db` agree on each participating
  machine
- bead totals and open counts either match or the difference is explicitly
  explained by branch scope
- `br doctor`, `check_beads_trust.sh`, and the strict bead-batch audit are green
  enough for the next swarm

### Phase 5: Prepare Next Iteration

After the environment gate is green, run a small implementation swarm.

Recommended shape:

- 2 agents first:
  - Agent A: bridge-origin launch binding and explicit-session readback
  - Agent B: real progress/status vocabulary and session-candidate projection
- add Agent C only after Agent Mail is stable:
  - terminal preference, exact-session handoff, and picker-facing bridge envelope
- keep GitHub-context authoring, final-submission indexing, and stitched E2E
  blocked until the earlier recovery primitives are proven

## Definition Of Ready For Implementation

The next Roger recovery implementation iteration is ready when:

- local/devbox authority is explicit
- participating machines can run the same `br`, `rr`, and `ntm` command surfaces
- Agent Mail coordination works end to end
- `br doctor`, queue trust, and strict bead audit are green or have accepted,
  bead-tracked exceptions
- the recovery frontier has structured acceptance criteria
- the operator can state exactly which machine is allowed to mutate which files
- the first implementation agents have narrow, non-overlapping beads

## Definition Of Done For Environment Alignment

Environment alignment is done when a fresh agent on local macOS and, if included,
devbox can run the same documented verification packet and get the same support
truth:

- same intended Git authority
- same intended bead frontier
- same `br` front door semantics
- same `rr` command surface
- same Agent Mail project/thread usability
- same NTM operation contract
- same explicit statement of which browser/terminal/provider proofs are local
  macOS-only versus devbox-capable
