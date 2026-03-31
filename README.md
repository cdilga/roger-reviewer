# Roger Reviewer

> Local-first pull request review with durable sessions and explicit approval
> gates before any GitHub write.

## Who This Is For

Primary audience for the current `0.1.0` slice:

- engineers who review GitHub PRs from the terminal
- teams that want durable review continuity (not one-shot chat output)
- users who want strict local safety defaults before posting anything remotely

This repo is still pre-release, but implementation is active and the core
review loop is now real on the blessed OpenCode path.

## What Roger Is (And Is Not)

Roger Reviewer is:

- local-first (`SQLite` state is authoritative)
- session-aware (start, resume, refresh, and return are durable)
- approval-gated (no silent posting path)
- continuity-focused (OpenCode fallback stays real)

Roger Reviewer is not:

- an auto-fix bot by default
- an auto-posting GitHub bot
- a daemon-centered architecture

## Current `0.1.0` Support Snapshot

| Surface | Status now | Notes |
| --- | --- | --- |
| `rr` CLI | Active, validated | Start/resume/refresh/return/sessions/findings/status/search are implemented |
| OpenCode harness path | Blessed (`Tier B`) | Locator reopen + reseed fallback + dropout/return continuity model |
| Browser extension | Bounded launch surface | PR-page launch panel; Native Messaging primary, `roger://` fallback |
| Extension live status | Not in this slice | Launch-only honesty: open Roger locally for authoritative status |
| In-extension posting controls | Not in this slice | Approval/posting remains local and explicit |

Provider policy for this README:

- blessed quickstart path: OpenCode
- bounded/experimental provider paths may exist in code and tests, but are not
  documented here as the primary supported user flow

## Quickstart (Blessed Local Path)

### 1. Prerequisites

- Rust toolchain installed
- Git repo with a configured GitHub `origin` remote
- OpenCode CLI available on `PATH` (or set `RR_OPENCODE_BIN`)

Optional environment overrides:

```bash
export RR_STORE_ROOT="$PWD/.roger"
export RR_OPENCODE_BIN="opencode"
```

### 2. Run `rr` from source

From this repo:

```bash
cargo run -p roger-cli --bin rr -- help
```

If you prefer shorter commands in your shell session:

```bash
alias rr='cargo run -q -p roger-cli --bin rr --'
```

### 3. Start a review session

```bash
rr review --pr 123 --provider opencode
```

### 4. Inspect and triage from local state

```bash
rr status
rr findings
rr sessions
rr search --query "null pointer"
```

### 5. Continue the same review safely

```bash
rr resume --pr 123
rr refresh --pr 123
rr return --pr 123
```

If multiple candidate sessions exist, Roger fails closed and asks for explicit
selection (for example `--session <id>`).

## Optional GitHub Launch Surface (Extension)

The extension is optional. Roger stays usable from CLI without it.

### Load unpacked extension

Use:

- `apps/extension/manifest.template.json`

The current panel injects on GitHub PR pages and offers:

- `Start`
- `Resume`
- `Findings`
- `Refresh`

Bridge dispatch order:

1. Native Messaging (`com.roger_reviewer.bridge`)
2. `roger://launch/...` fallback

Current behavior is intentionally launch-only:

- no in-extension live local status claim
- no in-extension approval/posting controls

## Safety Model (Non-Negotiable)

- No automatic GitHub posting.
- No automatic bug-fixing unless explicitly enabled by the user.
- No raw direct review writes outside Roger’s approval/posting flow model.
- Mutation-capable flows must be explicit and visibly elevated.

## Known Boundaries In This Slice

- Extension readback/status parity is not shipped yet.
- Dedicated polished standalone TUI app packaging is still in progress.
- Support claims prioritize the blessed OpenCode continuity path.
- Degraded outcomes are expected and explicit when continuity or context is
  ambiguous.

## Repo Orientation

| Path | Purpose |
| --- | --- |
| `packages/cli` | `rr` command implementation |
| `packages/app-core` | Domain model, findings lifecycle, approval/posting contracts |
| `packages/session-opencode` | OpenCode session linkage and return model |
| `packages/bridge` | Native Messaging + URL launch bridge |
| `apps/extension` | GitHub PR launch panel |
| `packages/storage` | Canonical local store and retrieval |

## Canonical Docs

- [`AGENTS.md`](AGENTS.md) (operating rules)
- [`docs/PLAN_FOR_ROGER_REVIEWER.md`](docs/PLAN_FOR_ROGER_REVIEWER.md) (product/architecture authority)
- [`docs/HARNESS_SESSION_LINKAGE_CONTRACT.md`](docs/HARNESS_SESSION_LINKAGE_CONTRACT.md)
- [`docs/RELEASE_AND_TEST_MATRIX.md`](docs/RELEASE_AND_TEST_MATRIX.md)
