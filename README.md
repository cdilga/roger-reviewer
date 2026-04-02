# Roger Reviewer

> Pre-release, local-first pull request review with durable sessions and explicit
> approval gates before any GitHub write.

## Status (As Of April 1, 2026)

Roger Reviewer is in active `0.1.0` implementation.

What is real in this repo right now:

- local-first CLI/domain workflow is under active implementation
- OpenCode is the blessed continuity path for the quickstart flow
- extension support is optional and launch-focused, not the source of truth
- safety model is explicit: no automatic posting and no hidden mutation path

What is not shipped as a polished user release yet:

- no GA signed installer distribution channel yet (release lanes are still pre-release)
- no automatic in-place binary mutation path from `rr update` (manual install step remains explicit)
- no in-extension approval or posting controls

## Who This Is For

Primary audience for this pre-release slice:

- engineers reviewing GitHub PRs from terminal-first workflows
- teams that want durable local review continuity rather than one-shot prompt output
- users who require explicit approval gates before any remote write action

## Product Shape (Current Slice)

Roger Reviewer is:

- local-first (`SQLite` state is authoritative)
- session-aware (`review`, `resume`, `refresh`, `return`, `sessions`)
- approval-gated (draft + approve + post boundaries are explicit)
- continuity-focused (OpenCode fallback stays real)

Roger Reviewer is not:

- an automatic bug-fix bot by default
- an automatic GitHub posting bot
- a daemon-centered architecture

## Support Snapshot

| Surface | Current reality | Notes |
| --- | --- | --- |
| `rr` CLI from source | Available for local/dev use | Commands are implemented in `packages/cli`; run via `cargo run` |
| OpenCode provider flow | Blessed quickstart path | Primary documented continuity path for this slice |
| Codex provider path | Bounded/non-primary | Exists in command surface but is not the primary supported onboarding lane |
| Browser extension | Optional launch helper | PR-page launch panel; local Roger remains authoritative |
| In-extension posting controls | Not shipped | Approval/posting stays local and explicit |

## Install Reality (Pre-Release)

Installer/update metadata contracts now exist in-repo:

- `scripts/release/rr-install.sh` and `scripts/release/rr-install.ps1` consume
  `release-install-metadata-<version>.json` plus matching core/checksum assets
- `rr update` validates published release metadata and fails closed for
  local/unpublished builds without embedded release markers

Source-run onboarding is still the default developer path:

1. install Rust toolchain
2. clone repo and run `rr` through Cargo
3. keep state local (default `.roger/` store)

Optional env overrides:

```bash
export RR_STORE_ROOT="$PWD/.roger"
export RR_OPENCODE_BIN="opencode"
```

## Quickstart (Blessed Local Path)

### 1. Prerequisites

- Rust toolchain
- a Git repository with a GitHub `origin` remote
- OpenCode CLI available on `PATH` (or `RR_OPENCODE_BIN` override)

### 2. Run `rr` from source

```bash
cargo run -p roger-cli --bin rr -- help
```

Optional shell alias:

```bash
alias rr='cargo run -q -p roger-cli --bin rr --'
```

### 3. Start a review session (OpenCode)

```bash
rr review --pr 123 --provider opencode
```

### 4. Inspect local state

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

If candidate sessions are ambiguous, Roger fails closed and requires explicit
selection (for example `--session <id>`).

## Optional Browser Launch Surface

The extension is optional and currently launch-oriented.

Current source artifact:

- `apps/extension/manifest.template.json`

Current panel actions on GitHub PR pages:

- `Start`
- `Resume`
- `Findings`
- `Refresh`

Bridge dispatch order:

1. Native Messaging (`com.roger_reviewer.bridge`)
2. `roger://launch/...` fallback

Launch-only honesty in this slice:

- no in-extension authoritative local status mirror
- no in-extension approval/posting controls

## Safety Model (Non-Negotiable)

- No automatic GitHub posting.
- No automatic bug-fixing unless explicitly enabled by the user.
- No raw direct review writes outside Roger's approval/posting flow.
- Mutation-capable flows must be explicit and visibly elevated.

## Known Boundaries

- `rr update` currently validates metadata and emits a manual install command;
  fully automatic binary replacement is intentionally deferred
- extension readback/status parity is not shipped yet
- support claims prioritize the OpenCode continuity lane
- degraded continuity states are expected to fail closed where ambiguity exists

## Repo Orientation

| Path | Purpose |
| --- | --- |
| `packages/cli` | `rr` command implementation |
| `packages/app-core` | Domain model, finding lifecycle, approval/posting contracts |
| `packages/session-opencode` | OpenCode session linkage and return model |
| `packages/bridge` | Native Messaging + URL launch bridge |
| `apps/extension` | GitHub PR launch panel |
| `packages/storage` | Canonical local store and retrieval |

## Canonical Docs

- [`AGENTS.md`](AGENTS.md)
- [`docs/PLAN_FOR_ROGER_REVIEWER.md`](docs/PLAN_FOR_ROGER_REVIEWER.md)
- [`docs/HARNESS_SESSION_LINKAGE_CONTRACT.md`](docs/HARNESS_SESSION_LINKAGE_CONTRACT.md)
- [`docs/RELEASE_AND_TEST_MATRIX.md`](docs/RELEASE_AND_TEST_MATRIX.md)
