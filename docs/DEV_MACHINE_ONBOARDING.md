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
codex mcp add mcp-agent-mail --url http://127.0.0.1:8765/api/
```

Then verify:

```bash
codex mcp list
codex mcp get mcp-agent-mail
```

Expected shape:

```text
Name            Url                         Status
mcp-agent-mail  http://127.0.0.1:8765/api/  enabled
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
5. Register Agent Mail with Codex using `codex mcp add mcp-agent-mail --url http://127.0.0.1:8765/api/`.
6. Install the user-level notify dispatcher under `~/.codex/hooks/`.
7. Add a repo-path mapping for Roger Reviewer in that dispatcher.
8. Clone this repo.
9. Verify `planning-workflow` is available under `~/.codex/skills/`.
10. Run the Roger Reviewer verification commands above.

## Beads CLI Pin for This Repo

This repo currently resolves `br` to a local patched build while upstream
regression `Dicklesworthstone/beads_rust#213` remains unresolved.

Swarm automation resolves and repairs the default path through:

- `/path/to/roger-reviewer/scripts/swarm/resolve_br.sh`

Canonical expected path shape on this machine as of 2026-04-15:

- `~/.local/bin/br -> ~/.local/bin/br-0.1.40.pinned`

Minimal verification:

```bash
/path/to/roger-reviewer/scripts/swarm/resolve_br.sh --print-path
br --version
readlink ~/.local/bin/br
```

Do not run backup binary filenames directly in automation or runbooks.

Current validated pin update (2026-04-15):

- latest upstream `beads_rust` release is `v0.1.40`
  (published `2026-04-15T00:45:55Z`)
- Roger now pins a locally built `0.1.40` from upstream `main` commit
  `766559a4207e30cab0680ae814a668c7961fb027`
- fresh temp-workspace
  `git init -> br init -> br create -> br create -> sqlite3 integrity_check -> br doctor`
  passed with the source-built `0.1.40`
- live Roger workspace `br ready`, `br sync --status`, and `br doctor`
  also passed with the same source-built `0.1.40`, so use the
  `0.1.40.pinned` expectation above for live setup work

## Rehearsal Transcript Summary (2026-04-02)

This is a historical single-machine transcript. Keep the procedural lessons, but
do not literalize the paths, pin versions, or machine-specific outputs below as
the current cross-machine contract.

Manual smoke commands run from this repo:

- `codex --version` -> `codex-cli 0.118.0`
- `test -f ~/.codex/auth.json` -> pass
- `test -f ~/.codex/skills/planning-workflow/SKILL.md` -> pass
- `codex mcp list` and `codex mcp get mcp-agent-mail` -> pass (`enabled`)
- `codex -C /path/to/roger-reviewer mcp list` -> pass
- `codex exec --ephemeral ...` Agent Mail tool probe -> pass
- `bash -n /path/to/mcp_agent_mail/scripts/integrate_codex_cli.sh` -> pass
- `scripts/swarm/resolve_br.sh --print-path` -> `~/.local/bin/br`
- `readlink ~/.local/bin/br` -> historical 2026-04-02 output:
  `~/.local/bin/br-0.1.34.pinned` (superseded by the current
  `~/.local/bin/br-0.1.40.pinned` pin above)
- `br --version` -> historical 2026-04-02 output:
  `br 0.1.34` (superseded by the current `br 0.1.40` pin above)

Fixes applied from this rehearsal:

- updated stale `br` pin guidance from `0.1.28` to the then-current
  `0.1.34.pinned` (superseded by the 2026-04-15 `0.1.40.pinned` update above)
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
