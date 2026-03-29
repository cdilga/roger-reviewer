# Overnight Swarm Runbook

This repo now has a repo-local tmux swarm launcher so you can run a Jeffrey-style overnight swarm without installing the full ACFS stack. It uses the agent CLIs already on this machine plus the local Agent Mail server for durable coordination.

There is also a lightweight `ntm` shim in `~/.local/bin/ntm` that wraps these repo-local swarm scripts for the subset of `spawn`, `send`, and `status` needed for overnight runs.

## What Is Already Set Up

- `am` is now a real executable at `~/.local/bin/am`.
- Agent Mail is running in tmux session `mcp-agent-mail`.
- Agent Mail readiness now passes on `http://127.0.0.1:8765/health/readiness`.
- The Roger Reviewer project can be created and reached through the live Agent Mail MCP server.

## Important Roger-Specific Constraint

Roger Reviewer is still in the planning and bead-polishing phase. The overnight swarm should work planning beads and docs, not implementation. The marching orders file already bakes that in:

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

## Session Layout

- `mcp-agent-mail`: detached tmux session running the Agent Mail server
- `roger-reviewer-swarm`: swarm session
- `roger-reviewer-swarm:control`: operator shell in the repo root
- one window per agent, named like `codex-01`, `claude-03`, `gemini-02`, `opencode-01`

## Recommended Tonight Flow

1. Run the prereq check.
2. Launch the swarm.
3. Broadcast the shared prompt.
4. Detach and let it run.
5. Periodically check `./scripts/swarm/status.sh` and `br list --status in_progress`.

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
