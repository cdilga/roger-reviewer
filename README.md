# Roger Reviewer

> Local-first pull request review with durable sessions and explicit approval
> gates before any GitHub write.

## Status (As Of April 7, 2026)

Roger Reviewer is in active `0.1.0` implementation.

What is real in this repo right now:

- local-first CLI/domain workflow is under active implementation
- OpenCode is the blessed continuity path for the quickstart flow
- extension support is optional and launch-focused, not the source of truth
- safety model is explicit: no automatic posting and no hidden mutation path

What is not fully shipped yet:

- no GA signed installer distribution channel yet
- no GA local-state/schema migration automation in `rr update` (migration-capable updates are deferred/fail-closed in `0.1.x`)
- no in-extension approval or posting controls

## Who This Is For

Primary audience for this slice:

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
| Published `rr` CLI install | Available | One-line release installer ships on GitHub Releases |
| `rr` CLI from source | Available for local/dev use | Commands are implemented in `packages/cli`; run via `cargo run` |
| OpenCode provider flow | Blessed quickstart path | Primary documented continuity path for this slice |
| Codex provider path | Bounded/non-primary | Exists in command surface but is not the primary supported onboarding lane |
| Browser extension | Optional launch helper | PR-page launch panel; local Roger remains authoritative |
| In-extension posting controls | Not shipped | Approval/posting stays local and explicit |

## Install Reality

Installer/update metadata contracts now exist in-repo:

- `scripts/release/rr-install.sh` and `scripts/release/rr-install.ps1` consume
  `release-install-metadata-<version>.json` plus matching core/checksum assets
- `rr update` is the Roger-owned updater for published releases and applies the
  replacement in place after metadata, target, and checksum verification
- default interactive apply is confirmation-gated; `--yes` / `-y` bypasses
  only the confirmation prompt and does not skip provenance or safety checks
- `--dry-run` remains a non-mutating metadata/preflight path, and `--robot`
  apply is blocked unless `--yes` / `-y` is also provided
- local/unpublished builds fail closed without embedded release markers; local
  state/schema migrations remain deferred in `0.1.x`

Public one-line installer entrypoints (CLI base product):

- Stable/latest (Unix):
  - `curl -fsSL https://github.com/cdilga/roger-reviewer/releases/latest/download/rr-install.sh | bash`
- Stable/latest (PowerShell):
  - `& ([scriptblock]::Create((Invoke-WebRequest -UseBasicParsing 'https://github.com/cdilga/roger-reviewer/releases/latest/download/rr-install.ps1').Content))`
- Pinned release (Unix, example `2026.04.07`):
  - `curl -fsSL https://github.com/cdilga/roger-reviewer/releases/download/v2026.04.07/rr-install.sh | bash -s -- --version 2026.04.07`
- Pinned release (PowerShell, example `2026.04.07`):
  - `& ([scriptblock]::Create((Invoke-WebRequest -UseBasicParsing 'https://github.com/cdilga/roger-reviewer/releases/download/v2026.04.07/rr-install.ps1').Content)) -Version '2026.04.07'`

Optional follow-on workflow (separate from base CLI install):

- bridge and extension packaging assets are optional; base `rr` install does not require them
- use optional lanes only when browser launch/helper integration is needed
- the browser extension is not shipped as a release artifact in this slice; pack/sideload it from source if you need the PR-page launch panel
- when browser integration is needed, use `rr extension setup --browser <edge|chrome|brave>` followed by
  `rr extension doctor --browser <edge|chrome|brave>`; normal setup should provision bridge registration
  without requiring manual `rr bridge install`

Safe isolated install example:

```bash
mkdir -p "$HOME/.local/rr-test-bin"
curl -fsSL https://github.com/cdilga/roger-reviewer/releases/latest/download/rr-install.sh | \
  bash -s -- --install-dir "$HOME/.local/rr-test-bin"
alias rr-rel="$HOME/.local/rr-test-bin/rr"
```

Source-run onboarding remains the developer path:

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

- a Git repository with a GitHub `origin` remote
- OpenCode CLI available on `PATH` (or `RR_OPENCODE_BIN` override)

### 2. Install `rr`

Stable/latest:

```bash
curl -fsSL https://github.com/cdilga/roger-reviewer/releases/latest/download/rr-install.sh | bash
```

Safe isolated install:

```bash
mkdir -p "$HOME/.local/rr-test-bin"
curl -fsSL https://github.com/cdilga/roger-reviewer/releases/latest/download/rr-install.sh | \
  bash -s -- --install-dir "$HOME/.local/rr-test-bin"
alias rr="$HOME/.local/rr-test-bin/rr"
```

### 3. Check the installed CLI

```bash
rr --help
rr update --dry-run
```

### 4. Start a review session (OpenCode)

```bash
rr review --pr 123 --provider opencode
```

### 5. Inspect local state

```bash
rr status
rr findings
rr sessions
rr search --query "null pointer"
```

### 6. Continue the same review safely

```bash
rr resume --pr 123
rr refresh --pr 123
rr return --pr 123
```

If candidate sessions are ambiguous, Roger fails closed and requires explicit
selection (for example `--session <id>`).

## Developer Path (Run From Source)

Prerequisites:

- Rust toolchain

Run `rr` from source:

```bash
cargo run -p roger-cli --bin rr -- help
```

Optional shell alias:

```bash
alias rr='cargo run -q -p roger-cli --bin rr --'
```

## Optional Browser Launch Surface

The extension is optional and currently launch-oriented.

Current source artifact:

- `apps/extension/manifest.template.json`

Current panel actions on GitHub PR pages:

- `Start`
- `Resume`
- `Findings`
- `Refresh`

Current UX reality:

- the extension is a bounded launch surface, not the source of truth for Roger state
- PR-page entry is under active UX refinement toward a right-rail `Roger Reviewer`
  host, lower-click primary actions, and more contextual secondary actions
- `Refresh` exists as a real Roger command today, but its long-term UX direction
  is contextual rather than always-primary

Guided setup contract (normal path):

1. run `rr extension setup --browser <edge|chrome|brave>`
2. load the unpacked extension package in your browser
3. run `rr extension doctor --browser <edge|chrome|brave>` to verify identity,
   host registration, and helper reachability

Normal onboarding should not require manual `rr bridge install`. Keep
`rr bridge install [--extension-id <id>] [--bridge-binary <path>]` as a
repair/admin path only (for example: newly added browser profile, local
registration drift, or explicit dev override), and keep those override flags
out of first-time setup instructions.

Current artifact truth:

- `0.1.x` uses the unpacked extension artifact as the primary setup/testing
  surface
- Roger still intends to keep a real path to future packed/shippable extension
  artifacts without changing the setup contract

Bridge dispatch order:

1. Native Messaging (`com.roger_reviewer.bridge`)
2. Fail closed when Native Messaging is unavailable; rerun `rr extension setup` and `rr extension doctor`

Launch-only honesty in this slice:

- no in-extension authoritative local status mirror
- no in-extension approval/posting controls

## Agent Mail Watch Over ngrok

For a browser-readable, read-only Agent Mail view over ngrok, expose the local
watcher instead of the raw Agent Mail MCP endpoint.

Start the watcher:

```bash
AGENT_MAIL_WATCH_AGENTS=BlueCat scripts/run_agent_mail_watch.sh
```

It prints the local URL, the Basic Auth username (`watch`), the generated
browser password, and the watcher port.

Tunnel the watcher port, not Agent Mail's `8765` MCP port:

```bash
ngrok http 8781
```

The watcher proxies read-only inbox fetches to Agent Mail server-side, so your
phone browser never needs the Agent Mail bearer token and never talks to the
raw SSE/MCP endpoint directly.

## Safety Model (Non-Negotiable)

- No automatic GitHub posting.
- No automatic bug-fixing unless explicitly enabled by the user.
- No raw direct review writes outside Roger's approval/posting flow.
- Mutation-capable flows must be explicit and visibly elevated.

## Known Boundaries

- `rr update` now performs Roger-owned in-place replacement for published
  releases, gated by explicit confirmation on an interactive TTY or `--yes` /
  `-y` for non-interactive apply
- local/unpublished builds still fail closed and require reinstall from a
  published release
- migration-capable update steps are intentionally deferred in `0.1.x`; future
  migration-required releases must fail closed with explicit backup/export +
  reinstall guidance
- extension readback/status parity is not shipped yet
- support claims prioritize the OpenCode continuity lane
- degraded continuity states are expected to fail closed where ambiguity exists

## Repo Orientation

| Path | Purpose |
| --- | --- |
| `packages/cli` | `rr` command implementation |
| `packages/app-core` | Domain model, finding lifecycle, approval/posting contracts |
| `packages/session-opencode` | OpenCode session linkage and return model |
| `packages/bridge` | Native Messaging bridge |
| `apps/extension` | GitHub PR launch panel |
| `packages/storage` | Canonical local store and retrieval |

## Canonical Docs

- [`AGENTS.md`](AGENTS.md)
- [`docs/PLAN_FOR_ROGER_REVIEWER.md`](docs/PLAN_FOR_ROGER_REVIEWER.md)
- [`docs/HARNESS_SESSION_LINKAGE_CONTRACT.md`](docs/HARNESS_SESSION_LINKAGE_CONTRACT.md)
- [`docs/RELEASE_AND_TEST_MATRIX.md`](docs/RELEASE_AND_TEST_MATRIX.md)
