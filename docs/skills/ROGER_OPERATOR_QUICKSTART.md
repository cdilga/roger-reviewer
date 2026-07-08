# Roger Operator Quickstart

Status: reusable Roger skill.

Purpose:
First-session walkthrough of Roger Reviewer for a new operator — install,
doctor preflight, picking review work, starting a review, where findings
actually come from, the local cockpit, findings/search, and the outbound
send chain. Truthful about which provider paths are live vs feature-gated.

This is the first-session tour of `rr`. For the full operator loop and robot
envelopes, see `ROGER_REVIEW_DRIVER.md`. For the feature-gated Copilot lane,
see `ROGER_COPILOT_HARNESS.md`. For the worker-side transport used inside a
provider session, see `ROGER_WORKER_PROTOCOL.md`. For cockpit keybindings,
see `ROGER_TUI_CHEATSHEET.md`.

## Install

```bash
curl -fsSL https://github.com/cdilga/roger-reviewer/releases/latest/download/rr-install.sh | bash
```

Installs `rr` into `$HOME/.local/bin` (override with `--install-dir` or
`RR_INSTALL_DIR`), auto-bootstraps the local store via `rr init --robot`, and
— when the release published one — installs the optional browser extension
package and the `roger-*` skills bundle into `$HOME/.claude/skills` (override
`RR_SKILLS_DIR`). Missing optional bundles degrade to a warning, not a failed
install.

## Preflight a provider

```bash
rr doctor --provider opencode   # or codex | gemini | claude | copilot
```

A green doctor proves prerequisites, not auth — the first real
`rr review`/`rr review --resume` proves auth. Provider truth, honestly:

- `opencode` — first-class default; the only live Tier B continuity path
  (verified start, `rr review --resume` reopen, `rr return`).
- `codex` / `gemini` / `claude` — bounded Tier A: start, reseed, raw capture
  only. No locator reopen, no `rr return`.
- `copilot` — feature-gated, off by default. Requires
  `RR_ENABLE_COPILOT_PROVIDER=1`; once enabled it is bounded Tier B. Never
  assume it is live without checking the gate.

## Pick review work

```bash
rr queue [--repo owner/repo] [--limit <n>]
```

Lists open PRs needing review (compat name: `rr prs`).

## Start a review

```bash
rr review --pr <n> [--provider <p>]
```

What actually happens under the hood: Roger records a `ReviewIntake`, creates
a durable `ReviewRun`, and schedules one or more `ReviewTask` rows inside it.
It launches the chosen provider session and hosts the review worker inside
that session through the `rr agent` transport (see `ROGER_WORKER_PROTOCOL.md`
for the worker side of this). The worker returns a `WorkerStageResult`; Roger
validates the nested findings pack and materializes canonical `Finding` rows
from it. Findings are not a black-box prompt output — they only become
canonical after this validated worker → manager handoff.

Re-enter an existing review later with:

```bash
rr review --resume [--pr <n> | --session <id>]
```

(compat name: `rr resume`).

## The local cockpit

```bash
rr open [--repo owner/repo] [--pr <n> | --session <id>]
```

Opens the local TUI cockpit (compat name: `rr tui`) for browsing sessions,
findings, drafts, and the timeline without leaving the terminal. See
`ROGER_TUI_CHEATSHEET.md` for the exact screens and keys — it never posts to
GitHub and it does not launch providers from inside the TUI.

## Inspect and search findings

```bash
rr findings [--pr <n> | --session <id>]
rr findings --query <text> [--repo owner/repo]      # prior-review search across sessions
rr findings --sessions [--repo owner/repo]          # session listing
```

## Send to GitHub — explicitly gated

```bash
rr send triage  --finding <id>... --state accepted|ignored|needs_follow_up|resolved
rr send draft   (--finding <id>... | --all-findings)
rr send edit    --draft <draft-id> (--body-file <path> | --editor)
rr send approve --batch <draft-batch-id>
rr send post    --batch <draft-batch-id>
```

`triage`, `draft`, `edit`, and `approve` are local-only. `rr send post` is the
only step that reaches GitHub, and only for one exact, locally approved
batch. Editing an approved draft revokes its approval and forces
re-approval.

## Browser companion (optional)

```bash
rr setup extension [--browser edge|chrome|brave]
rr setup doctor [--browser edge|chrome|brave] [--live]
```

Sets up and repairs the browser extension that surfaces Roger's attention
state next to a PR in GitHub's own UI. Chrome 137+ and Edge 150+ ignore
`--load-extension`, so the unpacked package needs one manual load via
`chrome://extensions` / `edge://extensions` before it works; Brave still
honored the flag-based launch at last verification.

## Staying current

```bash
rr setup update [--channel stable|rc] [--dry-run | --yes]
```

## What's live vs gated — the honest summary

- Live by default: the seven-verb CLI, the OpenCode provider, findings /
  cockpit / search, and the full send chain.
- Bounded Tier A live: `codex`, `gemini`, `claude` (no reopen, no return).
- Feature-gated, not default: the GitHub Copilot CLI provider — needs
  `RR_ENABLE_COPILOT_PROVIDER=1`. See `ROGER_COPILOT_HARNESS.md`.
- `rr agent` is a separate in-session transport for the review worker, not
  an operator command. It is not part of your quickstart loop as an
  operator — see `ROGER_WORKER_PROTOCOL.md` if you end up inside a session.
