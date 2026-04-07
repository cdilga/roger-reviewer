## Devbox Remote Swarm

This setup runs Roger Reviewer on `devbox` with a Codex-only swarm and keeps the
operator entrypoints outside the repo checkout.

The reusable swarm/control-plane source of truth now lives in the tracked
sibling repo `../agent-swarm-kit`. Roger keeps only the project adapter layer:

- [`.swarm-kit/project.env`](/Users/cdilga/Documents/dev/roger-reviewer/.swarm-kit/project.env) for per-project config
- Roger-specific prompts and doctrine under [`docs/swarm`](/Users/cdilga/Documents/dev/roger-reviewer/docs/swarm)
- Roger-specific bead and validation helpers under [`scripts/swarm`](/Users/cdilga/Documents/dev/roger-reviewer/scripts/swarm)

The generic entrypoints in [`scripts/swarm`](/Users/cdilga/Documents/dev/roger-reviewer/scripts/swarm) are thin wrappers into `agent-swarm-kit`.

### Layout

- Repo checkout: `/data/projects/roger-reviewer`
- Swarm ops bundle: `~/swarm/roger-reviewer`
- Main tmux session: `roger-reviewer`
- Control sessions:
  - `roger-reviewer-controller`
  - `roger-reviewer-health`

### Bootstrap

Run the local bootstrap script from the repo root:

```bash
./scripts/swarm/setup_devbox_remote.sh --host devbox
```

If your local SSH config does not have a `devbox` alias yet, use one of the
configured hosts directly:

```bash
./scripts/swarm/setup_devbox_remote.sh --host devbox-cf
./scripts/swarm/setup_devbox_remote.sh --host devbox-triton
```

What it does:

- installs the minimal remote toolchain needed for Codex swarm work
- installs `ntm`, `am`, `br`, and `bv`
- normalizes the remote `codex` binary path so tmux workers do not fall into
  the self-update prompt
- clones and syncs the current Roger repo to `/data/projects/roger-reviewer`
- patches remote repo-local `ntm` and Codex config to the remote paths
- syncs the shared Codex skills from this machine to `devbox`
- syncs the local `mcp_agent_mail` checkout to `~/mcp_agent_mail_py`
- starts Agent Mail in its own tmux session, `mcp-agent-mail`
- creates an operator bundle under `~/swarm/roger-reviewer`
- launches a Codex-only swarm with `--no-ft` and `nudge` control mode

### Day-to-Day Commands

SSH in:

```bash
ssh devbox
```

Operate from the external bundle:

```bash
~/swarm/roger-reviewer/bin/launch.sh
~/swarm/roger-reviewer/bin/status.sh
~/swarm/roger-reviewer/bin/controller-status.sh
~/swarm/roger-reviewer/bin/view.sh
~/swarm/roger-reviewer/bin/stop.sh
```

Inspect Agent Mail directly if needed:

```bash
tmux attach -t mcp-agent-mail
curl -fsS http://127.0.0.1:8765/health/liveness
```

### Notes

- The external launch wrapper defaults to 4 Codex workers and no Claude/Gemini.
- The current remote launcher uses `--control-mode nudge` and `--no-ft`.
- The bootstrap currently launches with `--no-preflight` because the Roger bead
  frontier may be empty when you are just validating the remote environment.
- Repo-local Codex MCP config is rewritten on `devbox` to use
  `http://127.0.0.1:8765/api/`, which matches the Python Agent Mail server.
- The bootstrap expects a local checkout of `mcp_agent_mail` at
  `~/Documents/dev/mcp_agent_mail`.
