# Dev Machine Onboarding

This document is the practical setup guide for bringing a new development
machine online for work on Roger Reviewer.

It is intentionally operational rather than architectural. Read this when you
want a new laptop, server, or `ssh devbox` environment to be able to:

- open this repo in Codex
- use the planning and adversarial review workflow
- access the required prompts and planning artifacts
- use Agent Mail from Codex without repo-local secret files

Last validated: 2026-04-02.

## Scope

This guide covers the machine-level setup that is currently known to work for:

- Codex CLI
- the `planning-workflow` Codex skill
- Roger Reviewer planning docs and critique rounds
- Agent Mail MCP integration for Codex

It does not try to fully document every optional tool in the broader flywheel
stack. The priority here is to get a fresh machine to a working planning and
review state with minimal ambiguity.

## Expected End State

On a correctly onboarded machine:

- `codex` works and is logged in
- this repo is cloned locally
- the Agent Mail repo is available as a separate sibling checkout, for example
  `/path/to/mcp_agent_mail`
- Codex can see Agent Mail in this repo from a normal session
- the `planning-workflow` skill is available to Codex
- the Roger Reviewer planning docs and critique artifacts are present in-repo
- Codex uses user-level config under `~/.codex/`, not repo-local `.codex/`
- no secret-bearing Codex files live inside this repository

## Current Planning Assets in This Repo

These repo-local files are the planning asset set for the adversarial review
process:

- [`docs/PLAN_FOR_ROGER_REVIEWER.md`](docs/PLAN_FOR_ROGER_REVIEWER.md)
- [`docs/CRITIQUE_ROUND_01_FOR_ROGER_REVIEWER.md`](docs/CRITIQUE_ROUND_01_FOR_ROGER_REVIEWER.md)
- [`docs/CRITIQUE_ROUND_02_FOR_ROGER_REVIEWER.md`](docs/CRITIQUE_ROUND_02_FOR_ROGER_REVIEWER.md)
- [`docs/CRITIQUE_ROUND_03_FOR_ROGER_REVIEWER.md`](docs/CRITIQUE_ROUND_03_FOR_ROGER_REVIEWER.md)
- [`docs/PLANNING_WORKFLOW_PROMPTS.md`](docs/PLANNING_WORKFLOW_PROMPTS.md)
- [`docs/REPO_ONBOARDING_AND_DISCOVERY_PROMPTS.md`](docs/REPO_ONBOARDING_AND_DISCOVERY_PROMPTS.md)
- [`docs/ALIEN_ARTEFACTS_FOR_ROGER_REVIEWER.md`](docs/ALIEN_ARTEFACTS_FOR_ROGER_REVIEWER.md)
- [`AGENTS.md`](../AGENTS.md)

The machine does not need a special installer for these. Cloning the repo is
enough.

Authority note:

- `AGENTS.md` is the operational contract for agents
- `docs/PLAN_FOR_ROGER_REVIEWER.md` is the canonical current spec
- `CRITIQUE_ROUND_*` files are historical critique/integration artifacts
- `docs/PLANNING_WORKFLOW_PROMPTS.md` defines the repo-local review procedure
- `docs/REPO_ONBOARDING_AND_DISCOVERY_PROMPTS.md` defines the reusable
  pre-planning discovery workflow
- `docs/roger-reviewer-brain-dump.md` is raw intent, not the current spec

## Current Skill Availability

The main skill used for the adversarial review loop is:

- `planning-workflow`

That skill is expected to be installed for Codex at:

- `~/.codex/skills/planning-workflow/SKILL.md`

Observed on the current machine on 2026-03-29:

- Codex skill present: `planning-workflow`
- Repo prompt pack present: [`docs/PLANNING_WORKFLOW_PROMPTS.md`](docs/PLANNING_WORKFLOW_PROMPTS.md)
- Repo discovery prompt pack present: [`docs/REPO_ONBOARDING_AND_DISCOVERY_PROMPTS.md`](docs/REPO_ONBOARDING_AND_DISCOVERY_PROMPTS.md)

Practical implication:

- the general planning methodology comes from the Codex skill
- the Roger-specific prompts and critique history come from this repo
- repeatable repo onboarding/discovery now has its own reusable prompt pack in
  this repo
- there is no separate required `/adversarial-review` installer here; the
  workflow is the `planning-workflow` skill plus the repo-local prompt pack and
  planning artifacts

## Codex Setup

Install Codex and log in first. The exact install path may vary by machine, but
the critical requirement is that `codex` works and `~/.codex/auth.json` exists.

Minimal verification:

```bash
codex --version
test -f ~/.codex/auth.json && echo "auth ok"
```

## Rust Toolchain Setup

Roger's source tree is pinned to the Rust `nightly` channel through the
repo-local [`rust-toolchain.toml`](../rust-toolchain.toml). The workspace still
uses the `2024` edition; that is the language edition, not the compiler
channel.

On a fresh machine, install or update nightly before running Cargo commands in
this repo:

```bash
rustup update nightly
cargo +nightly --version
```

From the repo root, plain `cargo ...` commands should then resolve through the
repo-local nightly override automatically.

## Agent Mail Setup for Codex

### Important design choice

For Roger Reviewer, Codex should be configured at the **user level**, not by
writing repo-local `.codex/` files into this repository.

That means:

- MCP registration lives in `~/.codex/config.toml`
- any repo-aware notify routing lives in `~/.codex/hooks/`
- the repo itself should not contain `.codex/` or `codex.mcp.json`

### Current working shape

User-level Codex config:

- `~/.codex/config.toml`

User-level Agent Mail notify scripts:

- `~/.codex/hooks/agent_mail_notify_dispatch.sh`
- `~/.codex/hooks/agent_mail_notify_inbox.sh`

The dispatcher maps repo paths to Agent Mail identities.

### Register Agent Mail in Codex

Assuming the Agent Mail HTTP server is running locally on `127.0.0.1:8765`:

```bash
codex mcp add mcp_agent_mail --url http://127.0.0.1:8765/mcp/
```

Then verify:

```bash
codex mcp list
codex mcp get mcp_agent_mail
```

Expected shape:

```text
Name            Url                         Status
mcp_agent_mail  http://127.0.0.1:8765/mcp/  enabled
```

If an older `mcp-agent-mail` entry pointing at `/api/` already exists, remove it:

```bash
codex mcp remove mcp-agent-mail
```

### Notify hook model

The user-level notify hook should point to a dispatcher, not directly to a
repo-local wrapper.

Expected top-level entry in `~/.codex/config.toml`:

```toml
notify = ["~/.codex/hooks/agent_mail_notify_dispatch.sh"]
```

The dispatcher should inspect the current working directory and export:

- `AGENT_MAIL_PROJECT`
- `AGENT_MAIL_AGENT`
- `AGENT_MAIL_URL`
- `AGENT_MAIL_INTERVAL`

Then it should invoke the shared inbox check script.

## Roger Reviewer Repo Verification

From a fresh Codex process rooted in this repo, Agent Mail should still be
visible without any repo-local `.codex/` directory.

Before checking MCP visibility, verify Roger's shipped local bootstrap and
preflight surfaces:

```bash
cd /path/to/roger-reviewer
tmp_store="$(mktemp -d)"
RR_STORE_ROOT="$tmp_store" cargo run -q -p roger-cli -- init --robot
RR_STORE_ROOT="$tmp_store" cargo run -q -p roger-cli -- doctor --provider opencode --robot
```

Use the repo-built `rr` for this verification step rather than an ambient
installed `rr` on `PATH`. That keeps repo verification tied to the current
checkout and avoids mistaking a stale published install for a command-surface
regression in the repo itself. Reuse the same fresh `RR_STORE_ROOT` across the
two commands so old long-lived store state in this repo does not turn an
expected migration gate into a false quickstart failure.

Contract note:

- `rr init` only bootstraps Roger-owned local state (store root and marker)
- `rr doctor` verifies local and provider prerequisites with fail-closed repair
  guidance
- provider auth remains a deferred first-launch check verified during
  `rr review`/`rr resume`

## Copilot Admission Lane

GitHub Copilot CLI is not part of Roger's default public live claim in `0.1.0`.
Its current truthful posture is narrower:

- feature-gated bounded Tier B continuity lane only
- enable it with `RR_ENABLE_COPILOT_PROVIDER=1`
- Roger still treats OpenCode as the strongest default continuity path

### Copilot Prerequisites

Minimum local prerequisites before Roger should claim Copilot is available:

- `RR_ENABLE_COPILOT_PROVIDER=1`
- a reachable Copilot binary on `PATH`, or `RR_COPILOT_BIN=/abs/path/to/copilot`
- repo-owned Copilot instruction assets:
  - `.github/copilot-instructions.md`
  - `.github/instructions/*.instructions.md`
- repo-owned Roger hook assets:
  - `.github/hooks/roger-review.json`
  - `scripts/copilot-hooks/session-start.sh`
  - the rest of `scripts/copilot-hooks/*`
- run from the intended repo/worktree root; Roger treats worktree isolation as
  required, not optional

User-level hook truth: Copilot CLI only honors repo-level `.github/hooks/*.json`
once they are merged into the reviewed repo's default branch, which Roger cannot
require of arbitrary review targets. `rr review --provider copilot` therefore
installs and refreshes Roger-owned user-level hook assets under
`$COPILOT_HOME/hooks` (default `~/.copilot/hooks`) before every launch; those
hooks no-op unless Roger's launch environment is present. `rr doctor --provider
copilot` reports their installed/stale/missing state as the
`copilot_user_level_hooks_installed` check. The repo-level assets remain the
contract source of truth that the user-level copies are generated from.

Verification packet:

```bash
cd /path/to/roger-reviewer
tmp_store="$(mktemp -d)"
export RR_ENABLE_COPILOT_PROVIDER=1
export RR_COPILOT_BIN="${RR_COPILOT_BIN:-$(command -v copilot)}"

RR_STORE_ROOT="$tmp_store" cargo run -q -p roger-cli -- init --robot
RR_STORE_ROOT="$tmp_store" cargo run -q -p roger-cli -- doctor --provider copilot --robot
```

Use `rr doctor --provider copilot --robot` as the truthful preflight. It
verifies the gate, binary reachability, instruction assets, hook assets,
routine-surface worktree root, policy digest, hook digest, and custom
instructions digest. It does not claim provider auth is already proven.

### Roger-Imposed Copilot Restrictions

Roger's current Copilot policy profile is `review_readonly`.

That profile means:

- no raw GitHub writes through Copilot by default
- no implicit shell execution, write tools, or external URL/network access
- no built-in GitHub MCP or broad MCP access
- no provider-memory writes, allow-all mode, raw `gh` writes, or remote delegation
- provider-local state is treated as continuity evidence only, not as Roger authority
- worktree-root isolation is required so Roger can verify the launched session
  belongs to the intended repo/worktree

If a later policy profile widens any of those capabilities, Roger must surface
that change explicitly through `rr doctor`, `rr status`, `rr robot-docs`, and
the provider support matrix. Hidden widening is not allowed.

### What Roger Records Locally

Roger's Copilot lane is durable because it records local evidence rather than
trusting ambient provider state.

Current local evidence includes:

- launch-attempt and provider-session linkage in Roger's store
- `policy_profile_digest_sha256`, `hook_profile_digest_sha256`, and
  `custom_instructions_digest_sha256` in doctor/status/provider-capability
  surfaces
- routine-surface context including `worktree_root` and `launch_profile_id`
- session-start hook artifacts containing provider id, verified `session_id`,
  `worktree_root`, `launch_profile_id`, `attempt_nonce`, and `policy_digest`
- audit artifact classes `copilot_tool_denial` and
  `copilot_transcript_reference`
- `SessionLocator`, `ResumeBundle`, and continuity state when Roger has enough
  evidence to persist them truthfully

### First-Launch And Continuity Truth

Copilot support is fail-closed and staged:

- if the feature gate is off, Copilot remains `planned_not_live`
- if the gate is on but the instruction/hook assets are missing, `rr doctor`
  blocks with repair guidance
- provider auth remains a deferred first-launch check; a green doctor result
  does not mean Roger already proved the live Copilot auth/session path
- `rr review --provider copilot` proves verified start only when the Roger hook
  emits a real session id and matching worktree evidence
- `rr resume` can reopen by locator/session id on the current feature-gated
  Tier B lane
- if reopen is stale or unusable, Roger degrades honestly to `ResumeBundle`
  reseed instead of pretending reopen succeeded
- `rr return` is supported on the same feature-gated Tier B lane
- if the stale/unusable continuity case has no valid `ResumeBundle`, Roger
  fails closed rather than inventing a fallback

When you need a live proof after preflight, use a disposable PR target and run
`rr review --provider copilot` or `rr resume` from the same worktree. That is
the step that proves real provider auth and hook emission, not `rr doctor`.

Useful checks:

```bash
codex -C /path/to/roger-reviewer mcp list
codex exec --ephemeral -C /path/to/roger-reviewer -s read-only -o /tmp/rr-last.txt \
  'State whether an Agent Mail MCP server is available in this session. If yes, list exactly three Agent Mail tool names from the available MCP namespace and nothing else.'
cat /tmp/rr-last.txt
```

Expected result should mention Agent Mail tools such as:

- `mcp__mcp_agent_mail__ensure_project`
- `mcp__mcp_agent_mail__register_agent`
- `mcp__mcp_agent_mail__send_message`

## Notes About `mcp_agent_mail`

Example local shape on one maintainer machine:

- Roger Reviewer checkout:
  `/path/to/roger-reviewer`
- Agent Mail checkout:
  `/path/to/mcp_agent_mail`

Keep Agent Mail outside this repo. It is a separate project used to support the
development environment, not part of Roger Reviewer's source tree.

`scripts/integrate_codex_cli.sh` previously had an upstream syntax-regression
lane. On this machine, the same script passed `bash -n` on 2026-04-02.

Operational rule:

- always re-run the syntax check on your machine rather than assuming current
  upstream state from this document

Minimal verification:

```bash
bash -n /path/to/mcp_agent_mail/scripts/integrate_codex_cli.sh
```

If this fails, do not trust the Codex integration script as-is.

## Optional `rch` Helper

`rch` is not part of Roger Reviewer's canonical toolchain. The repo does not
require it for normal build, test, planning, or bead work.

Use it only if you already have an `rch` worker fleet installed and want to
offload CPU-heavy Cargo tasks during swarm execution. The swarm runbooks treat
it as optional and should degrade cleanly to direct local execution when it is
absent.

Minimal verification:

```bash
command -v rch || echo "rch not installed"
```

If `rch` is absent, continue with direct local `cargo ...` commands.

## Recommended Onboarding Sequence for `ssh devbox`

1. Install Codex and log in until `~/.codex/auth.json` exists.
2. Clone `mcp_agent_mail` as a sibling checkout, for example to `/path/to/mcp_agent_mail`.
3. Verify whether upstream `scripts/integrate_codex_cli.sh` passes `bash -n`.
4. Start the local Agent Mail server.
5. Register Agent Mail with Codex using `codex mcp add mcp_agent_mail --url http://127.0.0.1:8765/mcp/`.
6. Install the user-level notify dispatcher under `~/.codex/hooks/`.
7. Add a repo-path mapping for Roger Reviewer in that dispatcher.
8. Clone this repo.
9. From the repo root, run the repo-local verification commands above with one
   fresh `RR_STORE_ROOT` shared across both commands.
10. Verify `planning-workflow` is available under `~/.codex/skills/`.
11. Run the remaining Roger Reviewer verification commands above.

## Beads CLI Pin for This Repo

This repo currently resolves `br` to a local patched build while upstream
regression `Dicklesworthstone/beads_rust#213` remains unresolved.

Swarm automation now expects a usable `br` on `PATH`. The machine-level `br`
entry should resolve to the Roger-safe front door; `scripts/swarm/br_safe.sh`
is the implementation detail behind that surface and the maintainer/debug path.

Canonical expected path shape on this machine as of 2026-04-19:

- `~/.local/bin/br -> ~/.local/bin/br-main.current`

Minimal verification:

```bash
command -v br
./scripts/swarm/br_safe.sh --print-path
br --version
readlink ~/.local/bin/br
```

Do not run backup binary filenames directly in automation or runbooks.

Current wrapper update (2026-04-19):

- Roger now expects the chosen default `br` install to already be present on
  the machine and reachable via `PATH`
- repo automation no longer carries a dedicated path-repair or source-build
  installer script; install or rebuild `br` using your normal machine-level
  process, then verify it through `./scripts/swarm/br_safe.sh --print-path`
- fresh temp-workspace
  `git init -> br init -> br create -> br create -> sqlite3 integrity_check -> br doctor`
  must pass before widening support claims for a newly built binary
- live Roger workspace `br ready`, `br sync --status`, and `br doctor`
  must also pass before using that newly built binary as the default live path

## Rehearsal Transcript Summary (2026-04-02)

This is a historical single-machine transcript. Keep the procedural lessons, but
do not literalize the paths, pin versions, or machine-specific outputs below as
the current cross-machine contract.

Manual smoke commands run from this repo:

- `codex --version` -> `codex-cli 0.118.0`
- `test -f ~/.codex/auth.json` -> pass
- `test -f ~/.codex/skills/planning-workflow/SKILL.md` -> pass
- `codex mcp list` and `codex mcp get mcp_agent_mail` -> pass (`enabled`)
- `codex -C /path/to/roger-reviewer mcp list` -> pass
- `codex exec --ephemeral ...` Agent Mail tool probe -> pass
- `bash -n /path/to/mcp_agent_mail/scripts/integrate_codex_cli.sh` -> pass
- `./scripts/swarm/br_safe.sh --print-path` -> `~/.local/bin/br`
- `readlink ~/.local/bin/br` -> historical 2026-04-02 output:
  `~/.local/bin/br-0.1.34.pinned` (superseded by the current
  `~/.local/bin/br-main.current` managed path above)
- `br --version` -> historical 2026-04-02 output:
  `br 0.1.34` (superseded by the current managed latest-main path above)

Fixes applied from this rehearsal:

- updated stale `br` pin guidance from `0.1.28` to the then-current
  `0.1.34.pinned` (superseded by the current `br-main.current` managed-path
  contract above)
- updated stale Agent Mail integration-script status text

Fresh-eyes intake evidence from this rehearsal:

- linked repair bead: `rr-1f4.5` (default `br` claim-mutation FK mismatch)
- linked test-follow-up decision: `no-test` for a new lower-layer unit/integration
  suite in this bead, because the failure is a binary-selection/runtime-path issue;
  validation stays at int/manual-smoke using the explicit 3-step repro command set
  recorded in `rr-1f4.5` acceptance/validation contract.

## Quick Checklist

```bash
codex --version
test -f ~/.codex/auth.json && echo "codex auth ok"
test -f ~/.codex/skills/planning-workflow/SKILL.md && echo "planning-workflow ok"
codex mcp list
test -f ~/.codex/hooks/agent_mail_notify_dispatch.sh && echo "notify dispatcher ok"
test -f /path/to/roger-reviewer/docs/PLANNING_WORKFLOW_PROMPTS.md && echo "repo prompts ok"
```

If all of the above pass, the machine is in good shape for planning and
adversarial review work on Roger Reviewer.
