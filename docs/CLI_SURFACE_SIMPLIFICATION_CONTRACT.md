# CLI Surface Simplification Contract

This contract defines the intended Roger CLI shape after the current
local/devbox reconciliation. It is an implementation-facing support contract,
not a historical planning note.

## Goal

Roger's CLI should feel like one local review product, not a bag of plumbing
verbs. The operator should learn a small vocabulary:

- `rr doctor`: check whether Roger can run
- `rr queue`: choose review work
- `rr review`: start or re-enter review work
- `rr open`: use the local cockpit
- `rr findings`: inspect and search review output
- `rr send`: explicitly prepare/approve/post outbound communication
- `rr setup`: install, update, and repair local integrations

Existing commands remain supported until their replacements are live and
covered. Compatibility must be quiet and truthful: old commands can stay as
aliases, but help, README, and guided repair text should prefer the simpler
names.

## Command Units

| Unit | Preferred command | Current live backing command | Scope |
| --- | --- | --- | --- |
| Environment | `rr doctor` | `rr doctor`, `rr init`, `rr assets *` | Bootstrap, provider preflight, semantic assets, local store health |
| Work queue | `rr queue` | `rr prs` | Read-only open-PR queue joined with local Roger state |
| Review entry | `rr review` | `rr review`, `rr resume`, `rr return` | Start, resume, and explicit bare-harness return |
| Cockpit | `rr open` | `rr tui` | Interactive local workspace |
| Findings | `rr findings` | `rr findings`, `rr status`, `rr search`, `rr sessions` | Readback, attention, evidence, prior-review search |
| Outbound | `rr send` | `rr triage`, `rr draft`, `rr approve`, `rr post` | Local triage/draft/approval plus the only GitHub posting path |
| Setup | `rr setup` | `rr extension *`, `rr bridge *`, `rr update` | Browser companion, Native Messaging host, update path |
| Machine API | `rr api` | `rr robot-docs`, `rr agent` | Robot docs and worker transport |

## Current Implementation Slice

Landed in this slice:

- `rr queue` aliases the proven `rr prs` queue handler.
- `rr open` aliases the proven `rr tui` cockpit handler.
- `rr --help` presents the simplified vocabulary first.
- README command guidance presents `queue` and `open` as preferred names while
  keeping `prs` and `tui` visible as compatibility names.

Still intentionally not moved in this slice:

- outbound commands remain top-level `triage`, `draft`, `approve`, and `post`
  until `rr send` has a fail-closed parser and robot-schema compatibility plan
- extension/bridge/update/assets commands remain separate until `rr setup`
  can delegate without hiding mutation-capable flows
- `rr robot-docs` and `rr agent` remain explicit because they are machine
  interfaces, not normal operator workflow

## Target Syntax

The desired final operator syntax is:

```sh
rr doctor [--provider opencode|codex|gemini|claude|copilot]
rr queue [--repo owner/repo] [--limit n]
rr review [--pr n] [--repo owner/repo] [--provider p]
rr review --resume [--pr n|--session id]
rr open [--pr n|--session id]
rr findings [--pr n|--session id] [--query text]
rr send accept --finding id...
rr send draft (--finding id...|--all)
rr send approve --batch id
rr send post --batch id
rr setup extension --browser edge|chrome|brave
rr setup update [--dry-run|--yes]
rr api docs guide|commands|schemas|workflows
```

Compatibility mapping:

- `rr prs` -> `rr queue`
- `rr tui` -> `rr open`
- `rr sessions` -> `rr findings --sessions` or a cockpit picker
- `rr search` -> `rr findings --query`
- `rr triage|draft|approve|post` -> `rr send *`
- `rr extension|bridge|assets|update` -> `rr setup *`
- `rr robot-docs` -> `rr api docs`

## Non-Negotiables

- `rr send post` must remain visibly elevated and bound to an exact locally
  approved draft batch.
- No alias may bypass stale-state, target-binding, approval-token, or provider
  capability checks.
- Browser setup and Native Messaging repair must not be presented as ordinary
  review actions.
- Robot schemas must remain stable. New preferred names may be aliases over
  existing schema ids until an explicit schema migration exists.
- Machine surfaces must not crowd normal `rr --help`.

## Reconciliation Decisions

Keep from the local branch:

- Edge `connectNative` launch path
- live CDP panel-interaction E2E harness
- Docker install/update E2E
- release UX/help polish
- undated `nightly` Rust toolchain policy until dated nightly has current CI
  proof across cross-target release lanes

Keep from devbox:

- PR-listing route detection and row-level review kickoff controls
- PR-listing row bridge dispatch and full-page fixture coverage
- manifest validator fix that removes unsupported `refresh_review`
- bead JSONL/rebuild repair helper scripts, updated as repair tooling rather
  than product scope

Do not merge wholesale:

- the dated-nightly release-toolchain commit remains a separate decision
  because prior release history shows cross-target builds can fail when the
  local toolchain pin and CI target installation channel diverge

## Acceptance

A slice that claims CLI simplification progress must prove:

- `rr --help` leads with the simplified vocabulary
- preferred aliases work and route to the same fail-closed handlers
- README uses preferred names first
- robot docs remain truthful about the underlying command/schema ids
- extension and outbound mutation flows remain visibly explicit
