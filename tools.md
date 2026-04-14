# Roger Reviewer Tools

This file is the practical agent-side environment guide for working on Roger
Reviewer from another workstation such as a devbox.

Read this if you want a fresh machine to be able to:

- open the repo in Codex or Claude Code
- read and follow `AGENTS.md`
- use the core Roger planning and validation workflow
- use the beads and swarm-adjacent toolchain without guessing
- install the minimum useful skills for agent work

This document focuses on the agent environment, not on end-user Roger install.

## Scope

This is the working toolchain for developing Roger, not the shipped Roger
product surface.

Core rule:

- prefer user-level agent config and skills under home-directory state
- do not put secret-bearing agent config inside this repository

## Minimum Agent Stack

Required for serious Roger work:

- `git`
- `cargo` and `rustc`
- `codex`
- `tmux`
- `rg`
- `br`
- `cass`

Strongly recommended:

- `claude`
- `ntm`
- Agent Mail server and Codex MCP registration
- `cargo llvm-cov`
- `fd`
- `bun`

Optional but useful:

- `bv`
- `cargo fuzz`
- `criterion`
- `loom`

## What This Repo Assumes

For the current Roger workflow, the important assumptions are:

- `AGENTS.md` is authoritative for repo workflow
- `docs/PLAN_FOR_ROGER_REVIEWER.md` is the canonical product plan
- `docs/TESTING.md` is the testing doctrine
- `br` is the backlog system
- `cass` is the quick memory/history lookup tool
- `ntm` is optional orchestration, not a hard dependency
- Agent Mail is the preferred coordination layer when using multiple agents

## Fastest Setup Paths

There are two sensible ways to bring up the agent-side environment.

### 1. Manual local workstation setup

Use the install and verification sections in this file when you want a normal
laptop or local workstation ready for repo work.

This path is best when:

- you are setting up one machine by hand
- you want to understand each tool instead of inheriting a swarm bundle
- you do not need the remote `devbox` swarm shape

### 2. Remote `devbox` bootstrap

If you are bringing up the known remote swarm environment, the fastest path is
the repo bootstrap:

```bash
./scripts/swarm/setup_devbox_remote.sh --host devbox
```

Alternate configured hosts:

```bash
./scripts/swarm/setup_devbox_remote.sh --host devbox-cf
./scripts/swarm/setup_devbox_remote.sh --host devbox-triton
```

That path is valuable because it already knows the Roger devbox shape and
installs or syncs the agent-side pieces that are otherwise easy to miss:

- `ntm`
- Agent Mail
- `br`
- `bv`
- Codex skills
- the remote Roger checkout
- the remote Agent Mail checkout

If you want the remote path, read this file first and then use:

- [`docs/swarm/DEVBOX_REMOTE_RUNBOOK.md`](docs/swarm/DEVBOX_REMOTE_RUNBOOK.md)
- [`docs/DEV_MACHINE_ONBOARDING.md`](docs/DEV_MACHINE_ONBOARDING.md)

## Core CLIs

### Rust

Install Rust with `rustup`, then verify:

```bash
cargo --version
rustc --version
cargo fmt --version
cargo clippy --version
```

Roger's Rust validation baseline is:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo llvm-cov --workspace --all-targets --summary-only
```

Install the coverage tool once:

```bash
cargo install cargo-llvm-cov --locked
```

### Shell and repo utilities

Install and verify:

```bash
git --version
rg --version
fd --version
tmux -V
```

If `fd` is unavailable, Roger work can continue with `rg --files`, but `fd` is
worth installing.

Typical package-manager installs:

```bash
# macOS
brew install ripgrep fd tmux

# Debian/Ubuntu
sudo apt-get update
sudo apt-get install -y ripgrep fd-find tmux
```

## Agent CLIs

### Codex

Install Codex CLI, log in, and verify:

```bash
codex --version
test -f ~/.codex/auth.json && echo "codex auth ok"
```

Codex should use user-level config under `~/.codex/`, not repo-local config.

### Claude Code

Optional but recommended if you use mixed-agent workflows:

```bash
claude --version
```

## Roger workflow tools

### `br` — beads

Roger uses `br` as the live backlog interface.

Verify:

```bash
br --version
br info
```

Common commands:

```bash
br ready
br list --status open
br show <id>
br update <id> --status in_progress
br close <id>
```

Important Roger-specific note:

- this repo has historically used a pinned `br` binary because of upstream
  regressions
- before automating `br`, read `AGENTS.md` and
  `docs/DEV_MACHINE_ONBOARDING.md`
- if you are standing up the remote `devbox` workflow, let
  `./scripts/swarm/setup_devbox_remote.sh` install and wire the expected `br`
  path for you

Local verification for the pinned-repo expectation:

```bash
./scripts/swarm/resolve_br.sh --print-path
readlink ~/.local/bin/br 2>/dev/null || command -v br
```

### `cass` — coding-agent history search

Use `cass` for fast context lookup across prior agent sessions.

Install options known from the local exploration copy of the upstream project:

```bash
# macOS
brew install dicklesworthstone/tap/cass

# older Linux distributions where prebuilt binaries are not a fit
cargo install --git https://github.com/Dicklesworthstone/coding_agent_session_search cass
```

Verify:

```bash
cass --version
cass status --json 2>/dev/null
```

High-signal usage:

```bash
cass search "planning-workflow" --workspace "$PWD" --robot-format toon --limit 5 --max-content-length 300 2>/dev/null
cass search "beads OR ntm OR cass" --workspace "$PWD" --robot-format toon --limit 10 --max-content-length 300 2>/dev/null
```

Observed caveat on the primary machine:

- `cass status` may report degraded or stale state if the local index needs
  refresh or if the environment restricts access to the DB
- when `cass` works but is stale, refresh it outside critical-path work

Optional semantic-model follow-up after install:

```bash
cass models install
```

### `bv` — graph-aware beads triage

Recommended for multi-bead prioritization:

```bash
bv --robot-triage
bv --robot-next
bv --robot-plan
```

Do not run bare `bv` from agent flows unless you intentionally want the TUI.

If you are using the remote `devbox` bootstrap, it installs `bv` for you.
Otherwise, verify availability explicitly:

```bash
bv --version
```

### `ntm` — Named Tmux Manager

`ntm` is optional but is the best fit when you want an orchestrated multi-agent
tmux session.

Install:

```bash
curl -fsSL https://raw.githubusercontent.com/Dicklesworthstone/ntm/main/install.sh | bash
```

Shell integration:

```bash
echo 'eval "$(ntm init zsh)"' >> ~/.zshrc
source ~/.zshrc
```

Verify:

```bash
ntm deps -v
ntm tutorial
```

Useful commands:

```bash
ntm spawn roger --cc=2 --cod=1
ntm send roger --all "Read AGENTS.md and introduce yourself in Agent Mail."
ntm palette roger
ntm dashboard roger
```

For the Roger-specific operator surface after install, read:

- [`docs/swarm/NTM_OPERATOR_GUIDE.md`](docs/swarm/NTM_OPERATOR_GUIDE.md)
- [`docs/swarm/HUMAN_OPERATOR_FLYWHEEL_GUIDE.md`](docs/swarm/HUMAN_OPERATOR_FLYWHEEL_GUIDE.md)

## Agent Mail

Agent Mail is not part of Roger's shipped product, but it is part of the
preferred development environment.

Recommended shape:

- keep `mcp_agent_mail` as a sibling checkout, not inside this repo
- run the server locally
- register it with Codex at the user level

The best manual deep-dive is:

- [`docs/DEV_MACHINE_ONBOARDING.md`](docs/DEV_MACHINE_ONBOARDING.md)

The best remote-operator path is:

- [`docs/swarm/DEVBOX_REMOTE_RUNBOOK.md`](docs/swarm/DEVBOX_REMOTE_RUNBOOK.md)

Codex MCP registration:

```bash
codex mcp add mcp-agent-mail --url http://127.0.0.1:8765/api/
codex mcp list
```

Expected result should show the Agent Mail server as enabled.

For fuller details, read:

- [`docs/DEV_MACHINE_ONBOARDING.md`](docs/DEV_MACHINE_ONBOARDING.md)

## Skills

### Skills observed in Roger workspace history

Using `cass` against this workspace, the clearest repeatedly observed skills are:

- `planning-workflow`
- `cass`

These are the only ones I would currently call clearly observed from workspace
history without overclaiming.

### Skills this repo materially benefits from

For a useful devbox agent environment, install or make available:

- `planning-workflow`
- `cass`
- `beads`
- `bv`
- `ntm`
- `agent-swarm-workflow`
- `agent-fungibility`
- `mermaid-diagrams`

Practical split:

- `planning-workflow` for plan shaping and critique
- `cass` for prior-session lookup
- `beads` and `bv` for backlog and graph triage
- `ntm` and `agent-swarm-workflow` for multi-agent execution
- `agent-fungibility` to keep the swarm model honest
- `mermaid-diagrams` for workflow or architecture explanation when useful

### Codex skills path

Expected user-level skill root:

```bash
~/.codex/skills/
```

Quick verification:

```bash
ls ~/.codex/skills
test -f ~/.codex/skills/planning-workflow/SKILL.md && echo "planning-workflow ok"
```

If you maintain skills through marketplace installs or local clones, keep the
result visible from that root or from your Codex user-level configuration.

If you are using the remote `devbox` bootstrap, the setup script is already
designed to sync the shared Codex skills from the primary machine.

## Suggested verification on a fresh machine

Run this in order:

```bash
git --version
cargo --version
rustc --version
codex --version
tmux -V
rg --version
br --version
cass --version
test -f ~/.codex/auth.json && echo "codex auth ok"
test -f ~/.codex/skills/planning-workflow/SKILL.md && echo "planning-workflow ok"
```

Then from the repo:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cass search "planning-workflow" --workspace "$PWD" --robot-format toon --limit 3 --max-content-length 200 2>/dev/null
br ready
```

If using Agent Mail:

```bash
codex mcp list
```

## Current primary-machine snapshot

Observed on the current machine during this session:

- `br` available
- `cass` available
- `ntm` available
- `codex` available
- `claude` available
- `bun` available
- `cargo` and `rustc` available
- `git`, `rg`, and `tmux` available
- `fd` missing

That is enough for a strong Roger dev environment, but a fresh devbox should
install `fd` as well.

## Related docs

- [`AGENTS.md`](AGENTS.md)
- [`docs/TESTING.md`](docs/TESTING.md)
- [`docs/DEV_MACHINE_ONBOARDING.md`](docs/DEV_MACHINE_ONBOARDING.md)
- [`docs/PLAN_FOR_ROGER_REVIEWER.md`](docs/PLAN_FOR_ROGER_REVIEWER.md)
