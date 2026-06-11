# Roger Reviewer

<div align="center">

<img src="docs/roger-reviewer-project.png" alt="Roger Reviewer local-first review flow illustration" width="1100" />

![Release](https://img.shields.io/github/v/release/cdilga/roger-reviewer?style=flat-square&label=release)
![Rust](https://img.shields.io/badge/rust-nightly%20toolchain-8C6A5D?style=flat-square)
![Platforms](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-7A8CA5?style=flat-square)
![Browsers](https://img.shields.io/badge/browser-Chrome%20%7C%20Edge%20%7C%20Brave-A3B18A?style=flat-square)
![License](https://img.shields.io/badge/license-MIT-C9A66B?style=flat-square)

<p><strong>Local-first pull request review for GitHub.</strong></p>
<p>Durable sessions, structured findings, explicit approval gates, and an optional PR-page launch companion.</p>
<p><a href="#install">Install</a> · <a href="#quickstart">Quickstart</a> · <a href="#commands">Commands</a> · <a href="#blessed-paths">Blessed Paths</a> · <a href="#architecture">Architecture</a> · <a href="#contributing">Contributing</a></p>

</div>

Roger Reviewer turns pull request review into a durable local workflow. Start
from the shell or a GitHub pull request page, keep findings and drafts local,
and approve before anything is posted back to GitHub.

This README tracks the public `0.1.0` product shape and the blessed workflows
we intend to support publicly. The deeper planning and implementation contracts
live under [`docs/`](docs/).

## Why Roger Reviewer

Most review tools are easy to start and hard to continue. Findings disappear
into scrollback, follow-up context fragments across sessions, and the line
between "drafted locally" and "posted remotely" is often too blurry.

Roger takes a different position:

- local state is authoritative
- the terminal and TUI are the primary review surfaces
- the browser companion is optional
- GitHub writes stay behind an explicit approval gate
- the underlying coding session remains a real fallback path

## Install

Roger ships through GitHub Releases. That is the public install surface.

### macOS / Linux

```bash
curl -fsSL https://github.com/cdilga/roger-reviewer/releases/latest/download/rr-install.sh | bash
```

### Windows (PowerShell)

```powershell
& ([scriptblock]::Create((Invoke-WebRequest -UseBasicParsing 'https://github.com/cdilga/roger-reviewer/releases/latest/download/rr-install.ps1').Content))
```

If you want a pinned release instead of `latest`, use the tagged installer
assets from [GitHub Releases](https://github.com/cdilga/roger-reviewer/releases).

### Source Build / Contributor Toolchain

Roger's source tree is pinned to the Rust `nightly` channel through
[`rust-toolchain.toml`](rust-toolchain.toml). The workspace language edition
remains `2024` because the Rust edition and the compiler channel are separate
settings.

Minimal contributor setup:

```bash
rustup update nightly
cargo --version
cargo test --workspace --all-targets
```

Run those commands from the repo root so Cargo picks up the repo-local nightly
override automatically.

## Quickstart

The local-first path is the primary Roger experience.

### 1. Run provider-aware preflight

Roger's local store bootstraps automatically on first use, so there is no
required init step (`rr init` remains available as an explicit, idempotent
bootstrap).

```bash
rr doctor --provider opencode
```

### 2. Start a review

```bash
rr review --pr 123 --provider opencode
```

### 3. Inspect what Roger found

```bash
rr status
rr findings
rr tui
```

`rr tui` opens the keyboard-driven review cockpit: sessions, findings triage,
draft approval queue, timeline, and prior-review search in one place.

### 4. Continue the same review later

```bash
rr resume --pr 123
```

Replace `123` with your pull request number. In the live `0.1.0` CLI surface,
`rr review --provider` currently supports `opencode`, `codex`, `gemini`, and
`claude` by default, and exposes `copilot` when
`RR_ENABLE_COPILOT_PROVIDER=1` is set. OpenCode remains the strongest
continuity path. Codex, Gemini, and Claude Code are live only as bounded Tier A
paths: Roger can start a review, reseed from a `ResumeBundle`, and preserve raw
capture, but it does not claim locator reopen or `rr return` for those
providers. GitHub Copilot CLI is feature-gated as a bounded Tier B continuity
path: Roger can start a review, reopen by locator or session id, return with
`rr return`, and fall back to honest `ResumeBundle` reseed when reopen is stale
or unusable, but Copilot remains outside the default public live claim. The
browser companion is optional.

The Roger store lives at `~/.roger` (one canonical store per profile; override
with `RR_STORE_ROOT`). `rr doctor` verifies local bootstrap and provider
prerequisites, but auth remains a deferred first-launch check; run
`rr review`/`rr resume` to verify auth/path fail-closed behavior.

Roger reconciles stale review state when you re-enter through `rr review`,
`rr resume`, or `rr return`. `rr status` and `rr findings` are persisted
readback surfaces: they show the last recorded Roger state, warn when target
drift means you should re-enter or start a fresh pass, and do not imply
background reconciliation that has not happened.

For the current Copilot operator contract, including the exact gate,
preflight assets, Roger-imposed restrictions, and locally recorded continuity
artifacts, see [docs/DEV_MACHINE_ONBOARDING.md](docs/DEV_MACHINE_ONBOARDING.md).

## Commands

| Command | What it does |
| --- | --- |
| `rr prs` | List open pull requests as a review queue joined with local Roger state |
| `rr review --pr 123 --provider opencode` | Start a review for a pull request |
| `rr resume --pr 123` | Re-enter the existing review for that pull request |
| `rr tui` | Open the local review cockpit (sessions, findings, drafts, timeline, search) |
| `rr init` | Optional explicit bootstrap; the store auto-creates on first use |
| `rr doctor --provider opencode` | Run local + provider preflight checks with fail-closed guidance |
| `rr status` | Show the current session, attention state, and next step |
| `rr findings` | Inspect the structured findings Roger has materialized |
| `rr triage --finding <id> --state accepted` | Record your local triage decision on a finding |
| `rr draft --finding <id>` | Materialize a local outbound draft batch from accepted findings |
| `rr approve --batch <id>` | Record a local approval token for one exact draft batch |
| `rr post --batch <id>` | Post one approved batch to GitHub and record the audit trail |
| `rr sessions` | List local review sessions |
| `rr search --query "auth"` | Search prior local review memory and evidence |
| `rr return --pr 123` | Rebind a dropped-out bare OpenCode or feature-gated Copilot session back to Roger |
| `rr update` | Self-update `rr` from the latest published GitHub release |
| `rr extension fetch` | Download and verify the published extension package for this release |
| `rr extension setup --browser edge` | Set up the optional browser companion |
| `rr extension doctor --browser edge` | Verify the browser companion path |

## Blessed Paths

### 1. Local-first review

Install `rr`, start from the repo, and do the real review work locally. This is
the primary Roger path.

`rr return` is narrower than `rr resume`. Use it only after you intentionally
drop out of Roger into a bare harness session and want Roger to rebind that
work back to the original review session. If you are already in a normal
Roger-managed session, you usually want `rr resume` or just to keep working in
that session. In `0.1.0`, `rr return` is blessed on OpenCode tier-B continuity
paths and on the feature-gated Copilot tier-B continuity path enabled with
`RR_ENABLE_COPILOT_PROVIDER=1`; Codex, Gemini, and Claude should still fail
closed rather than pretend to support it.

### 2. Browser-assisted launch

The browser companion is for convenience, not authority. It helps you start or
resume from a PR page and can show bounded mirror state only when the bridge
readback is truthful, but Roger's local state remains the source of truth.

### 3. Local triage, local draft, explicit approval, remote post

Findings materialize locally as `new`. You triage them (`rr triage --state
accepted`), draft locally from accepted findings, approve explicitly, and only
then does Roger post back to GitHub. Each step fails closed on stale state.

### 3a. Review queue

`rr prs` lists the repository's open pull requests joined with your local
Roger session state (`not_started`, `in_review`, `needs_attention`, `drafted`,
`posted`) and suggests the next command per PR. It is read-only and is the
starting point for batching review work across a repo.

### 4. Underlying session fallback

Roger sessions stay tied to an underlying coding session so the fallback path
remains real instead of decorative.

## What Roger Refuses To Do

- post review comments automatically
- fix code automatically by default
- hide mutation-capable actions inside ordinary navigation
- require a long-running daemon as the center of the system

## Architecture

Roger is built around one shared local core. The CLI, TUI, and browser
companion are surfaces over that core, not separate products with separate
truth.

```mermaid
flowchart TD
    classDef entry fill:#EAF2FF,stroke:#4F7CFF,color:#102033,stroke-width:1.4px;
    classDef gate fill:#FFF3D9,stroke:#C68A00,color:#5B3A00,stroke-width:1.7px;
    classDef core fill:#ECFDF3,stroke:#2F855A,color:#173A28,stroke-width:1.4px;
    classDef data fill:#EEF8F6,stroke:#4C7A78,color:#102322,stroke-width:1.2px;
    classDef blocked fill:#FFF0F0,stroke:#CB3A3A,color:#6B1F1F,stroke-width:1.6px;
    classDef external fill:#F4F0FF,stroke:#7B61FF,color:#2D225E,stroke-width:1.3px;

    ENTRY["Entry surfaces<br/>CLI / TUI / extension"]:::entry
    INTAKE["Review intake<br/>repo + PR + baseline + prompt"]:::entry
    PREFLIGHT{"Preflight safe<br/>and unambiguous?"}:::gate
    ATTEMPT["LaunchAttempt<br/>recorded durably"]:::gate
    VERIFY{"Real provider session<br/>verified?"}:::gate
    WORKER["Review worker via rr agent gets<br/>bounded task + context"]:::core
    PACK["Structured findings pack<br/>plus raw output"]:::core
    NORMALIZE["Roger normalizes findings,<br/>attention, and lineage"]:::core
    INSPECT["TUI / CLI inspect, triage,<br/>search, clarify, and reconcile"]:::core
    DRAFT["Local draft queue<br/>and draft materialization"]:::core
    APPROVE{"Explicit human approval?"}:::gate
    POST["GitHub adapter posts"]:::external
    AUDIT["PostedAction audit trail"]:::data
    STORE["SQLite + artifacts + search"]:::data
    BLOCK["Fail closed<br/>status / setup / repair guidance"]:::blocked

    ENTRY --> INTAKE --> PREFLIGHT
    PREFLIGHT -- no --> BLOCK
    PREFLIGHT -- yes --> ATTEMPT
    ATTEMPT --> STORE
    ATTEMPT --> VERIFY
    VERIFY -- no --> BLOCK
    VERIFY -- yes --> WORKER --> PACK --> NORMALIZE --> INSPECT
    NORMALIZE --> STORE
    INSPECT -- follow-up / re-entry reconcile --> WORKER
    INSPECT --> DRAFT --> APPROVE
    APPROVE -- not yet --> INSPECT
    APPROVE -- approved --> POST --> AUDIT --> STORE
```

At the top level:

- `rr` owns the review lifecycle, approval model, and posting boundary
- findings, artifacts, and search stay local
- the review worker runs a bounded `rr agent` task inside a verified provider session
- the browser companion is limited to launch and bounded mirror readback
- GitHub is a target surface, not Roger's source of truth

For the fuller architecture and diagram pack, see
[`docs/ROOT_LEVEL_FLOW_AND_ARCHITECTURE_DIAGRAMS.md`](docs/ROOT_LEVEL_FLOW_AND_ARCHITECTURE_DIAGRAMS.md)
and
[`docs/PLAN_FOR_ROGER_REVIEWER.md`](docs/PLAN_FOR_ROGER_REVIEWER.md).

## Browser Companion

Roger's browser companion is optional and bounded.

- supported browsers: Chrome, Edge, and Brave
- supported `0.1.0` bridge: Native Messaging only
- normal setup path: `rr extension setup --browser <edge|chrome|brave>`
- verification path: `rr extension doctor --browser <edge|chrome|brave>`
- installed (non-dev) hosts: the installer unpacks the release extension
  package into `~/.roger/bridge/extension-package/<version>/`, and
  `rr extension fetch` re-downloads and checksum-verifies it on demand; no
  Roger source checkout is required
- browser truth note: branded Google Chrome 137+ ignores `--load-extension`,
  so Chrome needs one manual "Load unpacked" pass via `chrome://extensions`;
  Edge and Brave still honor the flag-based launch
- the browser lane is for launch, resume, and bounded mirror convenience;
  approval and posting stay local and explicit

## Contributing

Roger Reviewer uses an issue-first contribution path.

If you found a bug, want a feature, or want to challenge a workflow or product
assumption, open an issue first:

- [Open an issue](https://github.com/cdilga/roger-reviewer/issues)
- [Contribution policy](CONTRIBUTING.md)

## Docs

- [Canonical plan](docs/PLAN_FOR_ROGER_REVIEWER.md)
- [Architecture diagrams](docs/ROOT_LEVEL_FLOW_AND_ARCHITECTURE_DIAGRAMS.md)
- [TUI workspace contract](docs/TUI_WORKSPACE_AND_OPERATOR_FLOW_CONTRACT.md)
- [Harness session linkage contract](docs/HARNESS_SESSION_LINKAGE_CONTRACT.md)

## License

[MIT](LICENSE)
