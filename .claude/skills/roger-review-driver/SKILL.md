---
name: Roger Review Driver
description: Use when driving a pull request review with the rr CLI from outside a provider session — starting, resuming, inspecting, drafting, approving, and posting through Roger's gates. Covers the full operator loop, robot envelopes, provider selection, and fail-closed recovery.
---

# Roger Review Driver

This skill is for an agent operating `rr` as the review operator (outside any
provider session). If you are already inside a Roger-managed provider session
and need to call the worker transport, use `roger-worker-protocol` instead. If
you are in a bare-harness Roger-native command subset without a bound
`rr agent` task, use `roger-inside-roger-agent` instead.

## Core loop

```bash
rr doctor --provider <p> --robot        # fail-closed preflight for that provider
rr queue --robot                        # review queue: open PRs + local Roger state + next_command
rr review --pr <n> --provider <p> --robot
rr status --pr <n> --robot              # persisted readback: attention state + next step
rr findings --pr <n> --robot            # structured findings with fingerprints
rr review --resume --pr <n> --robot     # re-enter the same review later
```

`rr init --robot` bootstraps local state and only needs to run once per
machine/store. `rr prs`, `rr tui`, and `rr resume` remain routable compat
names for `rr queue`, `rr open`, and `rr review --resume` — prefer the forms
above.

Every `--robot` invocation returns a JSON envelope with `outcome`
(`complete` | `blocked` | `error`), `data`, `warnings`, and `repair_actions`.
Always branch on `outcome`; never scrape human text. Discover the full
machine surface with `rr api docs guide|commands|schemas|workflows --robot`
(compat name: `rr robot-docs`).

## Provider selection

Authoritative support order: opencode, codex, gemini, claude, copilot.

- `opencode` — first-class default Tier B continuity path; no gate needed.
  Supports verified start, `rr review --resume` reopen, and `rr return`.
  Prefer this unless the operator explicitly asked for another provider.
- `codex` / `gemini` / `claude` — bounded Tier A: start, reseed, raw capture
  only. They fail closed on `rr return`; do not work around that.
- `copilot` — feature-gated opt-in (not preferred, not default). Bounded
  Tier B once enabled: export `RR_ENABLE_COPILOT_PROVIDER=1`, ensure `copilot`
  binary on PATH (or `RR_COPILOT_BIN`), and run from the intended
  repo/worktree root. Then it supports verified start, locator/session-id
  reopen, `rr return`, and honest `ResumeBundle` reseed fallback. With the gate
  off, copilot is `planned_not_live`.

Run `rr doctor --provider <p> --robot` before the first review on a provider.
A green doctor proves prerequisites, not auth; auth is proven by the first
real `rr review`/`rr review --resume`.

## Outbound: triage → draft → edit → approve → post

Nothing reaches GitHub without this explicit chain, driven through
`rr send <sub>` (compat names: `rr triage|draft|approve|post` route to the
same fail-closed handlers). Findings materialize as `new`; only findings
triaged to `accepted` can be drafted:

```bash
rr findings --pr <n> --robot                                    # pick finding ids
rr send triage  --finding <id> --state accepted --robot
rr send draft   --finding <id> [--finding <id>] --robot         # or --all-findings
rr send edit    --draft <draft-id> (--body-file <path> | --editor)
rr send approve --batch <draft-batch-id> --robot                # local approval token
rr send post    --batch <draft-batch-id> --robot                # executes exactly that batch
```

Rules:

- `draft`, `edit`, and `approve` are local-only; only `post` touches GitHub.
- `rr send edit` on an approved draft revokes its approval and forces
  re-approval; it never edits a posted batch. It is a local human action and
  does not support `--robot`.
- Posting requires a prior explicit human approval decision. As an agent, do
  not chain approve+post autonomously unless the human operator told you to
  post in this session.
- Never bypass this chain with raw `gh pr review`/`gh api` writes. Reads via
  `gh` are fine; review-communication writes are not.
- Stale persisted state fails closed before draft/approve; re-enter with
  `rr review`/`rr review --resume` to reconcile, then retry.

## Continuity: resume vs return

- `rr review --resume --pr <n>` — normal re-entry into a Roger-managed
  review (compat name: `rr resume --pr <n>`).
- `rr return --pr <n>` — only after intentionally dropping out of Roger into a
  bare harness session; rebinds that session to the original review. Blessed
  on opencode and gated copilot only.
- If multiple sessions match, Roger blocks with a candidate list; pass
  `--session <id>` to disambiguate. Use `rr findings --sessions --pr <n>
  --robot` to list (compat name: `rr sessions`).

## Fail-closed recovery

When an envelope comes back `blocked`, read `repair_actions` and follow them
literally — they are the supported path. Common cases:

- store missing → `rr init`
- provider prerequisites missing → install/gate fix named by doctor
- target drift / stale state → re-enter via `rr review` or `rr review --resume`
- ambiguous session → explicit `--session` selection

Do not retry blocked mutations verbatim, do not invent flags, and do not
"repair" Roger state by editing the store directly.

## Memory

`rr findings --query <text> [--query-mode <mode>] --robot` searches prior
local review memory and evidence (compat name: `rr search --query <text>`).
Use it before re-reviewing a repo or PR you may have seen before. The full
accepted `--query-mode` set is
`auto|exact_lookup|recall|related_context|candidate_audit` (default `auto`).
