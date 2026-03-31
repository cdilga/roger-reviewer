# Overnight Swarm Runbook

This repo now has a repo-local tmux swarm launcher so you can run a Jeffrey-style overnight swarm without installing the full ACFS stack. It uses the agent CLIs already on this machine plus the local Agent Mail server for durable coordination.

There is also a local `ntm` wrapper in `~/.local/bin/ntm` that preserves the repo-specific swarm commands and delegates any unknown command to the upstream NTM binary installed at `~/.local/lib/acfs/bin/ntm`.

## What Is Already Set Up

- `am` is now a real executable at `~/.local/bin/am`.
- `ubs`, `caam`, and `slb` are installed locally.
- Agent Mail is running in tmux session `mcp-agent-mail`.
- Agent Mail readiness now passes on `http://127.0.0.1:8765/health/readiness`.
- The Roger Reviewer project can be created and reached through the live Agent Mail MCP server.
- `ntm` local commands (`spawn`, `send`, `status`, `grid`, `observe`, `supervise`) work against the repo tmux swarm.
- Upstream NTM subcommands such as `list`, `activity`, `attach`, and `dashboard` are available through the same `ntm` entrypoint when they do not collide with the repo-local commands.
- Metadata-aware upstream NTM commands still expect an adopted or natively spawned NTM session. For the current live swarm, prefer `ntm observe ...` or `wa state ...` unless you deliberately rebuild or adopt the session into upstream NTM.

## Important Roger-Specific State

Roger Reviewer has passed readiness and is now in active implementation. Workers should pick their own unblocked beads from `br ready`; do not hand-assign beads unless you are deliberately steering around a blocker. The marching orders file already bakes that in:

- [docs/swarm/overnight-marching-orders.md](/Users/cdilga/Documents/dev/roger-reviewer/docs/swarm/overnight-marching-orders.md)

## Commands

Validate the machine and repo first:

```bash
./scripts/swarm/check_prereqs.sh --codex 4 --claude 4 --gemini 2 --opencode 2
```

Launch a 12-agent-style swarm in tmux:

```bash
./scripts/swarm/launch_swarm.sh --codex 4 --claude 4 --gemini 2 --opencode 2 --delay-seconds 45
```

Or use the `ntm` shim:

```bash
ntm spawn roger-reviewer-swarm --cc=4 --cod=4 --gmi=2 --opc=2 --delay=45
```

Current low-limit local mix:

```bash
ntm spawn roger-reviewer-swarm --cc=1 --cod=6 --delay=45
```

Wait about 30 to 60 seconds for the CLIs to settle, then broadcast the shared marching orders:

```bash
./scripts/swarm/broadcast_marching_orders.sh
```

Or through the `ntm` shim:

```bash
ntm send roger-reviewer-swarm --file docs/swarm/overnight-marching-orders.md
```

Attach to the swarm session:

```bash
tmux attach -t roger-reviewer-swarm
```

Inspect swarm status without attaching:

```bash
./scripts/swarm/status.sh
```

Or:

```bash
ntm status roger-reviewer-swarm --lines 20
```

Inspect machine-readable swarm state:

```bash
ntm observe roger-reviewer-swarm --json --lines 30 --include-text
wa state roger-reviewer-swarm --json --lines 30 --include-text
```

Attach to the real live pane grid:

```bash
ntm grid roger-reviewer-swarm
```

## Session Layout

- `mcp-agent-mail`: detached tmux session running the Agent Mail server
- `roger-reviewer-swarm`: swarm session
- `roger-reviewer-swarm:control`: operator shell in the repo root
- `roger-reviewer-swarm:supervisor`: re-entry loop that nudges idle panes back into the backlog
- `roger-reviewer-swarm:grid`: optional live tiled view of the real panes
- one window per agent, named like `codex-01`, `claude-03`, `gemini-02`, `opencode-01`

## Recommended Tonight Flow

1. Run the prereq check.
2. Launch the swarm.
3. Broadcast the shared prompt.
4. Detach and let it run.
5. Periodically check `./scripts/swarm/status.sh`, `ntm observe roger-reviewer-swarm --json`, and `br list --status in_progress`.

## Monitoring Shortcuts

Check current in-progress work:

```bash
br list --status in_progress
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

Run UBS before high-signal commits or when a bead asks for a scan:

```bash
ubs .
```

## Recovery

If Agent Mail stops responding:

```bash
tmux attach -t mcp-agent-mail
```

If the swarm session is gone, launch it again with a fresh session name:

```bash
./scripts/swarm/launch_swarm.sh --session roger-reviewer-night2 --codex 4 --claude 4 --gemini 2 --opencode 2
./scripts/swarm/broadcast_marching_orders.sh --session roger-reviewer-night2
```

If a single agent window wedges, kill that tmux window and create a replacement window manually from the control shell. The other agents can keep going because durable state lives in beads and Agent Mail.
