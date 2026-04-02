# Overnight Swarm Runbook

This repo now has a native-upstream NTM swarm launcher so you can run a Jeffrey-style overnight swarm without the old local wrapper. It uses the agent CLIs already on this machine plus the local Agent Mail server for durable coordination.

## What Is Already Set Up

- `am` is now a real executable at `~/.local/bin/am`.
- `ubs`, `caam`, and `slb` are installed locally.
- Agent Mail is running in tmux session `mcp-agent-mail`.
- Agent Mail readiness now passes on `http://127.0.0.1:8765/health/readiness`.
- The Roger Reviewer project can be created and reached through the live Agent Mail MCP server.
- `ntm` now points directly at the upstream binary.
- The repo-local scripts under `scripts/swarm/` are thin Roger-specific launch/control helpers layered on top of upstream NTM.
- The canonical native session name for this repo is `roger-reviewer`. Use `roger-reviewer--<label>` if you need a second swarm on the same repo.
- Frankenterm / `ft` is part of the default observer path. Install it with `./scripts/swarm/install_frankenterm.sh` before swarm launch unless you are explicitly choosing degraded mode (`--no-ft`).
- WezTerm visibility limit is real: `ft` can only observe panes visible through WezTerm CLI; tmux-internal panes not surfaced to WezTerm are out-of-scope for `ft` and remain observable via tmux/NTM logs only.
- The helper now defaults to `assign` mode, which starts a persistent `ntm assign --watch --auto` lane plus a controller nudge lane so idle panes keep moving without manual typing.

## Important Roger-Specific State

Roger Reviewer has passed readiness and is now in active implementation. Workers should pick their own unblocked beads from `br ready`; do not hand-assign beads unless you are deliberately steering around a blocker. The marching orders file already bakes that in:

- [docs/swarm/overnight-marching-orders.md](/Users/cdilga/Documents/dev/roger-reviewer/docs/swarm/overnight-marching-orders.md)
- The above file is the concise worker startup prompt. The long-form doctrine is:
  [docs/swarm/worker-operating-doctrine.md](/Users/cdilga/Documents/dev/roger-reviewer/docs/swarm/worker-operating-doctrine.md)
- Latest bounded queue-trust rehearsal record:
  [docs/SWARM_QUEUE_TRUST_REHEARSAL_20260331.md](/Users/cdilga/Documents/dev/roger-reviewer/docs/SWARM_QUEUE_TRUST_REHEARSAL_20260331.md)
- For operator read checks under contention, prefer read-safe `br` flags:
  `--no-auto-import --no-auto-flush`.

## Commands

Validate the machine and repo first:

```bash
./scripts/swarm/preflight_swarm.sh --codex 6 --claude 0 --gemini 0 --opencode 0
```

Install Frankenterm (`ft`) for the default observer path:

```bash
./scripts/swarm/install_frankenterm.sh
```

For a deeper diagnostic dump (after preflight passes), run:

```bash
./scripts/swarm/check_prereqs.sh --codex 6 --claude 0 --gemini 0 --opencode 0
```

Audit the next bead batch before launch (operator prep, not worker rediscovery):

```bash
./scripts/swarm/audit_bead_batch.sh --limit 20 --strict
```

If this audit reports a thin or empty `br ready` while useful open work still
exists, run the queue-repair playbook it prints before launching a large swarm.

Launch a 12-agent-style swarm in tmux:

```bash
./scripts/swarm/launch_swarm.sh --session roger-reviewer --codex 6 --claude 0 --gemini 0 --opencode 0 --delay-seconds 45
```

Current low-limit local mix:

```bash
./scripts/swarm/launch_swarm.sh --session roger-reviewer --codex 3 --claude 1 --gemini 0 --opencode 0 --delay-seconds 45
```

If you want to launch manually with pure upstream NTM instead of the helper:

```bash
ntm spawn roger-reviewer --cod=6 --no-user --auto-restart
ntm send roger-reviewer --broadcast --file docs/swarm/overnight-marching-orders.md --no-cass-check
ntm assign roger-reviewer --watch --auto --strategy dependency --reserve-files
```

Attach to the swarm session:

```bash
tmux attach -t roger-reviewer
```

Inspect swarm status without attaching:

```bash
./scripts/swarm/status.sh
```

Inspect native upstream state:

```bash
ntm status roger-reviewer
ntm activity roger-reviewer --watch
ntm health roger-reviewer --watch
```

Attach to the real live pane grid:

```bash
ntm view roger-reviewer
```

## Session Layout

- `mcp-agent-mail`: detached tmux session running the Agent Mail server
- `roger-reviewer`: native upstream NTM swarm session
- `roger-reviewer-control-plane`: upstream `ntm assign --watch --auto` lane
- `roger-reviewer-controller`: controller nudge lane (runs `scripts/swarm/supervise_swarm.sh`)
- `roger-reviewer-health`: stuck-agent auto-restart lane
- `roger-reviewer-ft`: Frankenterm watcher (`ft watch --foreground`) when observer mode is enabled
- one pane per agent, titled by upstream NTM like `roger-reviewer__cod_1`

## Recommended Tonight Flow

1. Run the prereq check.
2. Run the bead-batch audit and address warnings that would make startup noisy.
3. Launch the swarm.
4. Broadcast the shared prompt.
5. Detach and let it run.
6. Periodically check `./scripts/swarm/status.sh`, `ntm activity roger-reviewer`, and `br list --status in_progress`.

## Monitoring Shortcuts

Check current in-progress work:

```bash
br list --status in_progress --no-auto-import --no-auto-flush
```

See what `bv` thinks is highest leverage:

```bash
bv --robot-triage
```

Inspect the Agent Mail server:

```bash
tmux capture-pane -p -t mcp-agent-mail:0 | tail -n 40
```

If `rch` is installed and configured, use it for CPU-heavy cargo work:

```bash
rch exec -- cargo build --release
rch exec -- cargo test
```

If the local `rch` helper is present but no worker fleet is configured, it should fail open and run locally instead of blocking the swarm.

Before closing CI-sensitive beads, verify remote closeout evidence explicitly:

```bash
scripts/swarm/check_ci_closeout_evidence.sh \
  --bead <id> \
  --run-url https://github.com/<owner>/<repo>/actions/runs/<id> \
  --outcome success
```

CI-sensitive categories are defined by bead labels:

- `ci` or `github-actions` (remote CI validation category)
- `release` or `publish` (release/publication truth category)

For non-CI-sensitive beads, local-only closeout is still allowed with an explicit reason:

```bash
scripts/swarm/check_ci_closeout_evidence.sh --bead <id> --local-only-reason "<reason>"
```

Run UBS before high-signal commits or when a bead asks for a scan:

```bash
ubs .
```

## Remote CI Failure Routing

Treat failing remote GitHub Actions runs as local actionable intake with single
owner assignment.

Ownership model:

1. first reporter claims ownership by attaching the run to one local bead
2. if an equivalent run/workflow failure is already owned, update that bead and
   do not create a duplicate repair bead
3. ownership remains with that worker until they close the repair bead or
   explicitly hand off in Agent Mail

Routing rules:

1. send an Agent Mail claim/update on topic `ci-failure`
2. include these fields in every message:
   - `run_id`
   - `run_url`
   - `workflow_name` or workflow path
   - `ref` (branch/tag)
   - `owner_agent`
   - `bead_id`
3. if no claim is posted within 15 minutes from first observation, any worker
   may claim and announce to break ambiguity

Minimal message template:

```text
topic: ci-failure
subject: Claimed CI failure <run_id> -> <bead_id>
body:
- run_id: <id>
- run_url: <url>
- workflow: <name-or-path>
- ref: <ref>
- owner: <agent>
- bead: <bead_id>
- status: claimed|investigating|fixed|closed
```

## Fresh-Eyes Finding Intake (Testing-Only)

Use this workflow for onboarding/devx rehearsals so findings become actionable
work instead of chat-only notes.

Intake rules:

1. create or update a repair bead as soon as a finding is reproducible and
   impacts a documented onboarding or first-use step
2. dedupe first: if an equivalent open/in-progress bead already exists for the
   same command/path + symptom, update it instead of creating a speculative duplicate
3. post an Agent Mail update on topic `fresh-eyes` with:
   - source rehearsal (`README` or `DEV_MACHINE_ONBOARDING` step)
   - finding summary
   - linked repair bead id
4. record one test-follow-up decision per finding:
   - `test-added` when a deterministic lower-layer test is added in the same bead, or
   - `no-test` with an explicit reason + exact manual/int command when lower-layer
     automation is not truthful for that failure mode

Bounded rehearsal evidence requirement:

1. at least one linked repair bead from the rehearsal
2. at least one linked test-follow-up decision (`test-added` or `no-test`)

## Recovery

If Agent Mail stops responding:

```bash
tmux attach -t mcp-agent-mail
```

If the swarm session is gone, launch it again with a fresh session name:

```bash
./scripts/swarm/launch_swarm.sh --session roger-reviewer--night2 --codex 6 --claude 0 --gemini 0 --opencode 0
```

If a single agent window wedges, kill that tmux window and create a replacement window manually from the control shell. The other agents can keep going because durable state lives in beads and Agent Mail.
