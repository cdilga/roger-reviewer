# Roger Copilot Harness

Status: reusable Roger skill.

Purpose:
Operate Roger's feature-gated GitHub Copilot CLI provider lane truthfully:
gate enablement, doctor preflight, the review_readonly policy profile, and
bounded Tier B continuity (reopen, rr return, honest reseed fallback).

GitHub Copilot CLI is a feature-gated, bounded Tier B provider — an opt-in,
not Roger's default or preferred provider. OpenCode is the first-class Tier B
continuity path; Copilot is only available once you explicitly enable the
gate. This skill is the truthful operating recipe for that gated lane.

## Enable and preflight

```bash
export RR_ENABLE_COPILOT_PROVIDER=1
export RR_COPILOT_BIN="${RR_COPILOT_BIN:-$(command -v copilot)}"
rr doctor --provider copilot --robot
```

Doctor verifies, fail-closed: the gate, binary reachability, repo-owned
instruction assets (`.github/copilot-instructions.md`,
`.github/instructions/*.instructions.md`), Roger hook assets
(`.github/hooks/roger-review.json`, `scripts/copilot-hooks/*`), worktree-root
isolation, and the policy/hook/instructions digests. It does not prove auth —
the first `rr review --provider copilot` does, via a real session id emitted
by the session-start hook.

Run from the intended repo/worktree root. Worktree isolation is required, not
optional; Roger verifies the launched session belongs to that root.

## review_readonly policy profile

The active Copilot policy profile denies, by design:

- raw GitHub writes, raw `gh` writes, remote delegation
- general shell execution, write tools, external URL/network access
- built-in GitHub MCP and broad MCP access
- provider-memory writes and allow-all mode

Fail-closed worker-transport carve-out. The pre-tool-use hook still permits a
narrow allowlist so the in-session agent can stay inside Roger truth:

- `rr agent <op> --task-file <path>` — the dedicated in-session worker
  transport (`worker.get_status`, `worker.get_review_context`,
  `worker.search_memory`, `worker.list_findings`, `worker.submit_stage_result`,
  …). All operations read/submit within Roger's own nonce-gated boundary;
  `rr agent` rejects `--robot`.
- Read-only robot surfaces `rr status|findings|sessions|search … --robot`.

The matcher is STRICT: it rejects any command with shell chaining/quoting
metacharacters (`&& || | ; \` `` ` `` $ < > ( ) "`), `cd`/env-var prefixes, or
any non-allowlisted `rr` subcommand (`init`, `triage`, `draft`, `approve`,
`post`, `update`, `setup`, …) or `gh` write. Issue one clean allowlisted
command at a time. Bumping the allowlist bumps the hook contract version
(`copilot_review_readonly_hooks.v2`) and its digest.

Provider-local state is continuity evidence only — never Roger authority. If
a denial fires, Roger records a `copilot_tool_denial` audit artifact; that is
expected behavior, not an error to route around. Allow and deny decisions are
both logged to the hook audit dir (`pre-tool-use-decisions.jsonl`).

## Tier B continuity

- `rr review --pr <n> --provider copilot --robot` — verified start; the hook
  must emit a real session id plus matching worktree evidence.
- `rr resume --pr <n> --robot` — reopens by locator/session id
  (`copilot --resume <session-id>` under the hood). If the locator is stale or
  unusable, Roger degrades honestly to `ResumeBundle` reseed and says so in
  the envelope (`continuity_quality: degraded`), instead of pretending the
  reopen succeeded.
- `rr return --pr <n> --robot` — supported on this gated lane to rebind a
  bare Copilot session back to its Roger review.
- If reseed has no valid `ResumeBundle`, Roger fails closed. Do not fabricate
  a fallback.

## `--interactive` terminal handoff

`rr review|resume|return --provider copilot --interactive` hands the
terminal directly to Copilot (inherited stdio) instead of capturing batch
output. The parser enforces this fail-closed:

- only valid on `rr review`, `rr resume` (`rr review --resume`), and
  `rr return` — any other command rejects `--interactive`
- only with `--provider copilot`, and only when the gate is on
  (`RR_ENABLE_COPILOT_PROVIDER=1`); otherwise Roger rejects the flag rather
  than silently falling back to batch mode
- never combinable with `--robot` — `--interactive` hands off the terminal,
  `--robot` promises a machine-readable envelope, and the two are mutually
  exclusive by design

After the interactive session exits, Roger does not just trust that it
happened: it re-runs the same verified-start checks and session-binding
proof used for batch mode, and records hook audit events for the exchange.
A broken or incomplete interactive session still surfaces truthfully instead
of being assumed successful.

## What Roger records locally

Launch-attempt and session linkage, policy/hook/instructions digests,
`worktree_root` + `launch_profile_id`, session-start hook artifacts
(session id, attempt nonce, policy digest), denial/transcript audit
artifacts, and `SessionLocator`/`ResumeBundle` continuity state. Trust these
local records over any ambient Copilot state.

## Honesty rules

- Gate off → copilot is `planned_not_live`; never claim otherwise.
- Green doctor ≠ proven auth/session path.
- Do not widen the policy profile silently; any widening must surface through
  `rr doctor`, `rr status`, `rr robot-docs`, and the support matrix.
