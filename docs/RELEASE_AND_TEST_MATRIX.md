# Release And Test Matrix

This document turns the current plan into a common-sense support and validation
matrix for Roger `0.1.0`.

It is intentionally opinionated. The goal is to choose a small number of
high-value release targets and tests rather than pretending every combination is
equally important.

The canonical plan remains
[`PLAN_FOR_ROGER_REVIEWER.md`](docs/PLAN_FOR_ROGER_REVIEWER.md).

The implementation-facing harness contract lives in
[`TEST_HARNESS_GUIDELINES.md`](docs/TEST_HARNESS_GUIDELINES.md).
The automated E2E budget file lives in
[`AUTOMATED_E2E_BUDGET.json`](docs/AUTOMATED_E2E_BUDGET.json).
The user-language scenario source for those journeys lives in
[`PERSONA_JOURNEYS_AND_CHAOS_RECOVERY.md`](docs/PERSONA_JOURNEYS_AND_CHAOS_RECOVERY.md).

Roger now recognizes only three validation lanes:

- `unit`
- `integration`
- `e2e`

Names such as `fast-local`, `pr`, `gated`, `nightly`, and `release` are
execution policies or gates, not extra lanes.

## Engineering Posture

Rules:

- prefer one blessed path over a wide shallow support claim
- automate the highest-risk workflow first, not the easiest one
- treat OS/browser/provider packaging as product work, not release cleanup
- treat Native Messaging as the only supported `0.1.0` browser bridge
- do not treat manifest installation or `rr extension doctor` success as proof
  that the registered `rr` binary actually works as a Native Messaging host
- keep the support matrix explicit so unsupported combinations are truthful
- use stubs and fixtures where they increase determinism, but keep at least one
  real boundary test for each major external surface
- do not add heavyweight automated E2Es outside the declared budget without an
  explicit justification record
- treat release as an explicit operator gate backed by prerequisites and
  current evidence, not as an ambient fourth validation lane

## `0.1.0` Provider Matrix

| Provider | `0.1.0` status | Minimum expectation |
|----------|----------------|---------------------|
| GitHub Copilot CLI | Feature-gated bounded Tier B | Exposed only with `RR_ENABLE_COPILOT_PROVIDER=1`; verified start, locator/session-id reopen, `rr return`, and honest `ResumeBundle` reseed fallback, but still withheld from the default public live claim |
| OpenCode | First-class fallback and current strongest landed path | Real locator-based resume, Roger ledger integration, bare-harness dropout, `rr return` |
| Codex | Secondary, bounded | Exposed via `rr review --provider codex`; truthful Tier A reseed/raw-capture path, no locator reopen or `rr return` claim |
| Gemini | Secondary, bounded | Exposed via `rr review --provider gemini`; truthful Tier A reseed/raw-capture path, no locator reopen or `rr return` claim |
| Claude Code | Secondary, bounded | Exposed via `rr review --provider claude`; truthful Tier A reseed/raw-capture path, no locator reopen or `rr return` claim |
| Pi-Agent | Not in `0.1.0` | Planning-only future harness candidate; no live support claim, no `rr review --provider pi-agent`, and no Tier A/Tier B language until a later admission spike proves direct-CLI launch, Roger-safe policy control, audit capture, and truthful continuity behavior |

Bounded-provider coverage in `0.1.0` should stay common-sense:

- the authoritative provider support order is GitHub Copilot CLI, OpenCode,
  Codex, Gemini, then Claude Code
- that order does not widen a live claim before the relevant proof exists
- Roger owns the continuity model
- Codex, Gemini, and Claude Code do not require transcript-isomorphic resume
  parity with OpenCode to earn truthful Tier A claims
- if a bounded provider lacks a stable reopen path, Roger should still support
  truthful reseed/resume through `ResumeBundle` without widening the claim

Provider claim rule:

- Roger should only claim **bounded support** for a harness that satisfies the
  Tier A contract
- Roger should only claim **direct-resume or dropout support** for a harness
  that satisfies the Tier B contract
- Roger should only claim **in-harness Roger command support** for a harness
  that actually exposes the relevant Tier C affordances

## Agent-session / worker boundary acceptance

Roger now has two distinct machine-facing surfaces:

- `rr --robot` for operator-facing commands in machine-readable form
- `rr agent` for in-session review-worker calls bound to an active task

Minimum acceptance expectations for the worker boundary:

- `rr agent` requires valid session/run/task binding plus task nonce
- one task may execute as the default single-turn report flow or as an explicit
  configured multi-turn program
- prompt-turn history remains auditable per turn
- terminal worker results materialize findings only after Roger validation
- out-of-scope memory/tool requests fail closed with explicit denial

## `0.1.0` Browser And Bridge Matrix

| Surface | `0.1.0` status | Notes |
|---------|----------------|-------|
| Native Messaging | Primary bridge | Required for serious companion-tier behavior |
| Chrome | Supported | Same extension source base |
| Brave | Supported | Same extension source base |
| Edge | Supported and first-class | Must not be treated as "probably similar enough to Chrome" |

## Release Artifact Baseline

For `0.1.0`, Roger should plan to produce:

- `rr` companion/runtime binary for supported targets
- Native Messaging host manifest assets
- browser-installable extension artifacts
- checksums for release artifacts
- local install/uninstall instructions for companion + extension
- Roger-owned pack/install automation that generates these artifacts

Recommended minimum target matrix:

| Artifact class | Required targets |
|----------------|------------------|
| Core Rust binaries | macOS `arm64`, macOS `x86_64`, Windows `x86_64`, Linux `x86_64`, Linux `arm64` |
| Extension package | Chrome, Brave, Edge from one source base |
| Bridge install docs | macOS, Windows, Linux |

If Linux browser integration proves materially weaker at first, Roger should be
truthful about it rather than silently dropping Linux from the documented matrix
late.

Windows `arm64` remains an explicit follow-on release lane rather than part of
the current truthful first shipped subset. Do not describe it as blessed
support until the build, verification, installer, and update path all exist for
that target.

### `0.1.0` artifact classes

Roger should treat release outputs as four separate artifact classes with clear
ownership and support claims:

| Artifact class | Required for blessed `0.1.0` local release | Target platforms | Notes |
|----------------|---------------------------------------------|------------------|-------|
| Core companion archive | Yes | macOS `arm64`, macOS `x86_64`, Windows `x86_64`, Windows `arm64`, Linux `x86_64`, Linux `arm64` | Versioned archive containing the `rr` binary and minimal local runtime assets. The unified `release` workflow currently ships a truthful first subset (`macOS arm64/x86_64`, `Windows x86_64`, `Linux x86_64/arm64`) and records `Windows arm64` as explicitly excluded in the aggregate manifest until that lane is wired. |
| Bridge registration bundle | Yes where Roger claims browser-launch support on that OS | macOS, Windows, Linux | Native Messaging manifest templates plus Roger-owned install/uninstall helpers for the registered `rr` host runtime |
| Browser extension sideload package | Optional release lane, but required before Roger claims Chrome/Brave/Edge launch as shipped product behavior | One source base targeting Chrome, Brave, Edge | Keep browser-store publication out of the `0.1.0` critical path; publish installable package assets and docs first |
| Release metadata bundle | Yes | All published releases | `SHA256SUMS`, install/update docs, release notes, and asset manifest describing what was built and what support tier it carries |

Artifact-class rules:

- the core companion archive is the only artifact class that must always be
  present for a blessed local-first Roger release
- bridge registration bundles are separate from the binary archive because the
  OS registration steps and Native Messaging manifests are part of the product
  contract, not an afterthought in README prose
- browser-extension packaging is a separate optional release lane so Roger can
  ship an honest local product before browser publication workflow is complete
- if an extension or bridge artifact is omitted for a target, Roger must remove
  or narrow the corresponding support claim instead of implying parity

### Checksum and signing expectations

Roger should distinguish between integrity requirements that are mandatory now
and signing maturity that may arrive in stages.

| Artifact class | `0.1.0` minimum integrity expectation | `0.1.0` signing expectation |
|----------------|----------------------------------------|-----------------------------|
| Core companion archive | Publish in `SHA256SUMS` and verify in CI before publication | Preferred for blessed stable releases on macOS and Windows; if signing is not yet available for a target, call that out explicitly in release notes rather than silently shipping unsigned as if nothing changed |
| Bridge registration bundle | Publish in `SHA256SUMS` and keep manifest/install helpers versioned with the binary release | Same expectation as the companion archive when bundled as a platform installer; plain manifest/helper assets may remain checksum-only in early `0.1.0` releases |
| Browser extension sideload package | Publish in `SHA256SUMS` and verify the packaged extension matches the source revision | Browser-store signing is not a `0.1.0` gate; sideload artifacts may ship checksum-only as long as Roger does not claim store distribution |
| Release metadata bundle | Generate from CI and publish alongside assets | No extra signing requirement beyond whatever signs the release/tag provenance |

Rules:

- every blessed stable release must publish a checksum manifest covering all
  attached assets
- checksum generation and verification belong to CI/release automation, not to
  local maintainer habit
- signing is a release-quality signal, not a hidden optional nicety; if a
  target ships unsigned, the release notes must say so plainly

### CI and release job ownership

Roger should keep release automation inspectable without exploding the GitHub
Actions sidebar. The current shape is one operator-facing `release` workflow
with parallel jobs for build, packaging, verification, and publication.

| Job | Ownership | Responsibilities | Outputs |
|-----|-----------|------------------|---------|
| `ci-verify` | Continuous integration on every PR and release candidate | Lint, unit/integration suites, packaging smoke, artifact naming checks, checksum-manifest shape validation | Validation only; no published assets |
| `release` `fixture-rehearsal` | PR guard inside unified release workflow | Run deterministic release fixture scripts and contract checks when release plumbing changes | Validation only; no published assets |
| `release` `build-core` | Release pipeline | Build versioned Rust binaries for the supported OS/arch matrix, smoke the staged `rr` artifact surface, and stage raw archives | Per-target core companion archives |
| `release` `package-bridge` | Release pipeline | Generate Native Messaging manifests, platform registration helpers, and bridge install/uninstall bundles for supported OS targets | Per-OS bridge registration bundles |
| `release` `package-extension` | Release pipeline | Produce browser-installable extension packages from the shared source base and stamp them with the release version/source revision | Extension sideload packages for Chrome/Brave/Edge |
| `release` `verify-release-assets` | Release pipeline | Recompute checksums, verify archive contents, confirm release manifest completeness, and enforce publish gates | Verified `SHA256SUMS` and release asset manifest |
| `release` `windows-install-update-rehearsal` | Release pipeline | Rehearse release-hosted PowerShell install on a GitHub-hosted Windows runner and run installed `rr update --dry-run --robot` against the same release contract | `windows-install-update-rehearsal` artifact containing run summary + robot outputs |
| `release` `publish-release` | Release pipeline with explicit maintainer approval | Attach approved artifacts to the versioned release and publish notes from the same workflow run | Published GitHub release and notes |

Ownership rules:

- `release` keeps one top-level workflow while preserving job-level ownership
- `build-core` owns compilation, but not publication
- the intended gate is for `build-core` to fail closed if the staged `rr`
  artifact cannot satisfy the minimal packaged-binary smoke contract (`rr --help`,
  `rr robot-docs`, `rr update --dry-run --robot`) for that target, but the
  current workflow does not yet execute that packaged-binary smoke in
  `build-core`; release truth must therefore come from explicit downstream proof
  rather than assuming this gate already exists
- `package-bridge` owns Native Messaging registration assets and host-runtime
  packaging truth, but not browser-extension packaging
- `package-extension` remains its own job so the browser lane can
  advance or pause without muddying the core local-product release lane
- `publish-release` should publish only the artifact classes that actually
  passed verification; it must not imply extension availability if the extension
  job was skipped or failed for that tag
- manual release work should be limited to explicit approval and smoke checks,
  not ad hoc asset assembly

Unified `release` operator contract for `0.1.0`:

1. Run the unified `release` workflow for the intended tag.
2. Use `workflow_dispatch` with `publish_mode=draft` for rehearsal, or
   `publish_mode=publish` for stable CalVer tags only.
3. Approval is explicit via the `release-publish-approval` environment gate.
4. Release is an operator decision that should be made only after the current
   bead frontier, support wording, validation evidence, and smoke prerequisites
   are acceptable for the intended claim set.
5. `publish-release` must fail closed unless:
   - verify manifest schema is `roger.release-verify-assets.v1`
   - `publish_gate.publish_allowed == true`
   - the same workflow run produced successful build/package/verify jobs for the
     artifact classes being published
   - captured provenance URLs resolve to the canonical GitHub Actions run URL
     for that unified release run
   - approved tag ref and release metadata stay consistent across verified
     manifests
   - optional-lane parity holds with verify data from the same run; no silent
     widening or downgrade is allowed
6. Release notes are generated from the verified manifest and must include:
   support posture, narrowed claims, checksum/signing references, and release
   run provenance.
7. Stable publish (`publish_mode=publish`) requires explicit operator smoke
   acknowledgement and the checklist in
   `docs/release-publish-operator-smoke.md`.
8. `release-publish-plan` artifacts should retain:
   - generated release plan + notes
   - verified manifest/checksums/signing notes from the same run
9. Stable installer-readiness is not complete until live post-publish checks pass
   against the canonical repo:
   - `GET https://api.github.com/repos/<owner>/<repo>/releases/latest` returns
     `200` and resolves to the expected stable tag.
   - `bash scripts/release/rr-install.sh --repo <owner>/<repo> --dry-run`
     exits `0` and resolves install metadata + target archive URLs for that tag.
   - closeout records the absolute UTC probe timestamp and stable release tag
     used for the live check.
   - CI-sensitive closeout evidence for release/publish-labelled beads must
     include these live-proof fields through
     `scripts/swarm/check_ci_closeout_evidence.sh`:
     `--latest-proof-utc <YYYY-MM-DDTHH:MM:SSZ>`,
     `--latest-proof-tag <stable-tag>`, and
     `--installer-dry-run-outcome success`.

### Release request contract

Roger does not require one sacred trigger mechanism for release. A release
request may come from:

- explicit operator intent to cut a draft rehearsal or a stable release
- a bead or release-tracking note that says `release at this point`
- a support-claim narrowing change that needs a fresh tagged artifact to become
  truthful

Minimum release-request content:

- intended mode: `draft rehearsal` or `publish`
- target tag or channel
- intended claim set, including any explicit narrowed exclusions
- bead frontier expected to be included, or an explicit waiver list
- required smoke surfaces for this claim set

Agent obligations when a release is requested:

1. Resolve the current bead frontier and record any waived gaps explicitly.
2. Collect the freshest `unit`, `integration`, and `e2e` evidence required for
   the intended claim set.
3. Narrow release wording to what the current evidence actually supports.
4. Run the unified `release` workflow in draft mode first unless the operator
   explicitly asks to skip rehearsal.
5. Review the verified manifest, smoke evidence, and installer/readiness checks
   from that same run before proposing publish.
6. Leave behind the verified manifest, support posture, and any narrowed claims
   as the release closeout record.

Rules:

- a bead may request release-readiness, but it must not imply automatic publish
- publish remains an explicit operator approval decision
- if evidence is incomplete, agents should prepare the truthful narrowed draft
  release rather than widening claims or silently skipping prerequisites

### Publication posture

For `0.1.0`, Roger should publish release artifacts in this order:

1. Core companion archives plus release metadata bundle
2. Bridge registration bundles for supported OS targets
3. Browser extension sideload packages when that lane is ready for the tagged release

Browser-store submission is still a non-goal for `0.1.0`. The release contract
is satisfied by Roger-owned published assets plus truthful install/update
instructions.

## Install And Update Contract

The local Roger product needs one install lane and one update lane that stay
aligned with the artifact classes above.

### Base local-product lane

The blessed one-line flow installs only the local CLI/TUI companion surface:

- Unix-like shells: `curl -fsSL https://.../rr-install.sh | sh`
- PowerShell: `irm https://.../rr-install.ps1 | iex`

Behavior rules:

- the installer resolves the latest stable release by default and may accept an
  explicit pinned version or allowed channel such as `stable` or `rc`
- the `0.1.0` product-line alias currently belongs only to the installer
  surface documented in the update contract; explicit CalVer pins remain
  `YYYY.MM.DD[-rc.N]`
- host OS and CPU detection map to the published core companion archive matrix
  and must fail clearly for unsupported targets rather than guessing
- installer checksum/metadata behavior must follow the narrower current-truth
  rules in [`UPDATE_RELEASE_AND_TESTED_UPGRADE_CONTRACT.md`](UPDATE_RELEASE_AND_TESTED_UPGRADE_CONTRACT.md),
  including any surface-specific fallback or parity caveat
- WSL is an explicit narrowed cohort: only Linux-side Unix installer usage
  inside WSL is in scope for install guidance in `0.1.x`; Windows-host
  PowerShell installs remain a distinct Windows cohort and do not imply WSL
  parity
- install success yields a usable local `rr` binary without requiring the
  browser extension, Native Messaging registration, or store publication
- platform-specific install paths and PATH guidance are Roger-owned release-doc
  work, not hidden Homebrew, winget, or npm assumptions

Current repo entrypoints for this lane:

- Unix-like installer: `scripts/release/rr-install.sh`
- PowerShell installer: `scripts/release/rr-install.ps1`

Smoke validation for this contract:

- `bash scripts/release/test_rr_install.sh`
  - verifies fresh install success from a synthetic release payload
  - verifies fail-closed behavior when release metadata is missing
  - verifies fail-closed behavior when release metadata is ambiguous for target
- `bash scripts/release/test_update_upgrade_rehearsal.sh --output-dir <artifact-dir>`
  - builds representative stable releases `N` and `N+1` with distinct embedded
    release metadata
  - installs `N` through the official installer, proves same-version no-op on
    the installed old binary, applies `rr update --yes --robot`, and then
    proves same-version no-op on the updated binary
  - records explicit cohort posture in `update-upgrade-rehearsal-manifest.json`
    where `cohort_contract.wsl_unix_shell.status=covered_by_release_wsl_lane`;
    this rehearsal points to the dedicated WSL lane and does not widen WSL
    claims by itself
  - serves as the representative `INV-UPDATE-004` proof for the bounded stable
    direct-binary update lane; representative RC apply remains separately
    narrowed
- post-publish live stable smoke (manual release lane):
  - `curl -fsSL https://api.github.com/repos/cdilga/roger-reviewer/releases/latest`
  - `bash scripts/release/rr-install.sh --repo cdilga/roger-reviewer --dry-run`
  - fresh isolated install from the live Unix installer followed by
    `rr update --dry-run --robot` from that installed binary
  - record UTC timestamp + resolved stable tag in release closeout evidence
- release workflow Windows-host rehearsal lane:
  - `release` `windows-install-update-rehearsal` runs on `windows-2022`
  - installs from release-hosted `rr-install.ps1` and executes installed
    `rr update --dry-run --robot` against the same artifact contract
  - retains `windows-install-update-rehearsal-summary.json` plus robot/stdout
    artifacts for release evidence
- if the lane blocks or is missing for a release run, Windows install/update
  claims must be narrowed explicitly in release notes and closeout
- release workflow WSL-host rehearsal lane:
  - `release` `wsl-install-update-rehearsal` runs on `windows-2022` and
    executes `rr-install.sh` + installed `rr update --dry-run --robot` inside
    WSL against the same artifact contract
  - retains `wsl-install-update-rehearsal-summary.json` plus robot/stdout
    artifacts for release evidence
  - WSL support claims may widen only when retained lane summary evidence
    reports `status=pass`; blocked/missing evidence keeps claims narrowed
- dedicated local WSL rehearsal command (preflight aid):
  - `bash scripts/release/test_update_upgrade_rehearsal_wsl.sh --output-dir <artifact-dir>`
  - useful for rehearsal before publish, but release widening claims require the
    retained `wsl-install-update-rehearsal` artifact from the unified release
    run

### Update lane

`0.1.0` implementation status:

- `rr update` is the Roger-owned updater in `packages/cli` and performs
  in-place binary replacement against published CalVer release metadata
- default apply behavior is confirmation-gated on an interactive TTY
- current source exposes `--yes` / `-y` as confirmation-bypass flags for
  non-interactive apply, but release-hosted support claims must follow the
  actual published binary surface until a stable release carries those flags
- `--dry-run` and `--robot` remain non-mutating metadata/preflight paths
- local/unpublished builds are blocked and require explicit reinstall from a
  published CalVer release before update can run

Behavior rules:

- truthful current support is stable-by-default; RC users must opt in with
  `--channel rc` until current-channel stickiness is implemented and proven
- an explicit pinned target version is allowed
- the updater path reuses the same host detection and install metadata +
  manifest + checksum verification rules as install and fails closed on missing
  metadata, metadata/manifest drift, checksum mismatch, or ambiguous target
  resolution
- the representative stable direct-binary upgrade proof is the deterministic
  rehearsal `bash scripts/release/test_update_upgrade_rehearsal.sh --output-dir <artifact-dir>`;
  representative RC apply rehearsal remains a separate follow-on proof lane
  rather than implied parity
- WSL (`rr-install.sh` inside a WSL distro) remains an explicit narrowed cohort
  in `0.1.x`; install and reinstall guidance is allowed, but no in-place WSL
  update support claim should be made unless retained
  `wsl-install-update-rehearsal` release-lane evidence reports `status=pass`
- apply path uses an atomic rename/backup strategy with rollback restore when
  replacement fails after backup
- installs created from local/unpublished artifacts should not be upgraded as
  if they were blessed release installs; Roger requires an explicit reinstall
  from a published CalVer release in that case
- migration-capable updates are intentionally deferred in `0.1.x`; update
  responses must report migration policy as binary-only
  (`migration.status=deferred_in_0_1_x`) and fail closed if a future release
  requires state/schema migration before apply
- every unified release run must rehearse at least one representative
  prior-schema Roger store upgrade before artifact generation; a failing
  migration rehearsal blocks release generation rather than publishing a build
  that cannot open supported local state

Migration contract baseline for the `rr-1xhg` lane:

- compatibility envelope and fail-closed boundaries are defined in
  [`PLAN_FOR_SCHEMA_MIGRATIONS_AND_UPDATE_COMPATIBILITY.md`](PLAN_FOR_SCHEMA_MIGRATIONS_AND_UPDATE_COMPATIBILITY.md)
- once migration-capable update support is implemented, `rr update --dry-run`
  must report migration posture as one of:
  - `no_migration_needed`
  - `auto_safe_migration_after_update`
  - `migration_requires_explicit_operator_gate`
  - `migration_unsupported`
- apply must block before binary replacement when preflight reports
  `migration_requires_explicit_operator_gate` or `migration_unsupported`
- first-run store open may auto-run only Class A/B migrations when envelope and
  policy checks allow `auto_safe`; Class C/D paths remain fail-closed until an
  explicit operator-gated lane is shipped

### Separate optional extension lane

- bridge registration bundles and extension sideload packages remain an
  optional guided setup lane after core install, driven by
  `rr extension setup` and `rr extension doctor`; normal guided setup should
  provision bridge registration directly so first-time browser onboarding does
  not require a manual low-level bridge command
- Roger may expose explicit low-level commands such as `rr bridge install`, but
  those commands are repair/development workflows outside the normal onboarding
  contract
- release notes must not imply browser-launch support on a target unless the
  matching bridge registration bundle and extension package were actually
  shipped for that release
- browser-launch support also requires one host-runtime smoke that proves the
  registered `rr` binary can complete a Native Messaging request/response round
  trip; install/doctor-only validation is not enough
- extension packaging lane smoke command:
  `bash scripts/release/test_package_extension_bundle.sh`
  (proves zip artifact + extension-bundle manifest + bridge/pack robot outputs)

## Prescriptive E2E Catalog

Roger should not start with many slow end-to-end tests. It should start with a
small set that covers the real failure boundaries. This catalog is prescriptive:
it names the six major journeys Roger intentionally budgets, even though they
will be implemented incrementally rather than all at once.

Budget posture:

- budget-approved major journey slots: `E2E-01` through `E2E-06`
- executable today: `E2E-01`
- `E2E-02` through `E2E-06` are approved scenario slots for planning and guard
  purposes, but they do not count as functional coverage until executable
  suites land and run
- Roger intentionally keeps these as six distinct major proofs rather than one
  sprawling omnibus E2E so failures stay diagnostic and the implemented journeys
  can run in parallel when needed
- persona scenario ids such as `PJ-03A` or `PJ-05D` should be used as the
  human-readable story anchors when defining or defending one of these E2Es

### E2E-01: Core review happy path

Required shape:

- launch from CLI
- create or resume a real provider-backed review session
- capture a valid structured findings pack
- normalize findings into Roger state
- review/approve a local outbound draft
- post through a GitHub adapter test double
- persist posted-action audit state

Purpose:

- prove the full Roger loop works without the browser
- primary persona anchors: `PJ-03A` and `PJ-05A`
- invariant anchors: `INV-HARNESS-002`, `INV-POST-001`, `INV-POST-004`
- executable proof today: `e2e_core_review_happy_path`
- cheaper suites still own malformed-findings, invalidation, and post-recovery
  truth through `int_harness_opencode_resume`, `int_github_outbound_audit`, and
  `int_github_posting_safety_recovery`

Outbound approval visibility mapping (non-E2E owner for `INV-POST-004`):

- `cargo test -p roger-cli --test session_aware_cli_smoke status_and_findings_surface_outbound_approval_states_truthfully -- --nocapture`
- `cargo test -p roger-app-core --test tui_shell_smoke queue_and_inspector_keep_outbound_states_and_posting_elevation_visible -- --nocapture`

Rules:

- this is the first implemented member of Roger's six-slot heavyweight E2E
  catalog for `0.1.x`
- any seventh automated E2E needs explicit justification that lower-level unit
  or integration coverage cannot defend the same product promise more cheaply
- Roger should track the blessed automated E2E count in a small Roger-owned
  budget file or manifest and emit an agent-facing warning when that count rises
- that warning should explicitly ask whether the author can defend the new
  behavior with a smaller test or whether they are taking the lazy route to
  another expensive E2E
- after the warning-only phase, CI should be allowed to fail additions that lack
  a recorded justification
- this E2E stays lean and does not own Roger's broader memory or browser
  continuity story

### E2E-02: Cross-surface review continuity with recall

Status:

- implemented on the deterministic extension-loaded Chromium lane

Required shape:

- launch from a supported browser or extension-originated bridge path on a
  concrete PR target
- persist Roger-to-provider linkage for the review session
- drop out or detach while provider work is still in flight or before triage is
  complete
- resume the same review from another terminal
- enter the TUI, inspect the returned findings, and open prior review context
- triage at least three findings with distinct outcomes:
  `accepted`, `needs-follow-up`, and `ignored`
- materialize or update a local outbound draft batch from that triage
- persist continuity state and prove the resumed terminal sees the same review
  and draft state

Required memory assertions:

- recall runs from the live review context, not from an unrelated global search
- `retrieval_mode` is truthful: `hybrid` when semantic is available, or
  `lexical_only` when it is not
- repo scope remains the default and no `project_overlay` or `org_policy`
  content appears unless explicitly enabled for that review
- provenance buckets remain visible on returned items, including
  `repo_memory`, `project_overlay`, `org_policy`, and candidate/tentative items
- candidate memory remains visibly tentative rather than silently behaving like
  promoted memory
- degraded lexical-only fallback is surfaced honestly and remains usable after
  dropout or resume

Purpose:

- defend Roger's cross-surface continuity claim across extension or bridge
  entry, provider continuity, TUI triage, memory-aware recall, and local draft
  persistence
- persona anchors: `PJ-02A`, `PJ-02D`, `PJ-04A`, and `PJ-04B`
- invariant anchors: `INV-SESSION-002`, `INV-CONTEXT-001`,
  `INV-SEARCH-003`, `INV-SEARCH-004`
- executable proof today: none
- until the executable suite lands, the current cheaper owners are
  `int_cli_session_aware`, `accept_opencode_resume`, and
  `int_search_prior_review_lookup`
- missing executable proof owner: `rr-6iah.1`

Execution posture:

- preferred in `operator stability` or `release-candidate`, using a real
  supported provider path when the environment is available
- GitHub mutation remains doubled and locally approval-gated even in this E2E

### E2E-03: TUI-first review with memory-assisted triage

Status:

- budget-approved scenario slot; not yet implemented

Required shape:

- start from Roger's local CLI or TUI entry on a specific PR or open-session
  target
- create or resume a provider-backed review session
- browse findings, history, and relevant raw/provider outputs from the TUI
- use recall to compare at least one current finding with prior review evidence
  or policy memory
- triage at least three findings with distinct outcomes:
  `accepted`, `needs-follow-up`, and `ignored`
- materialize or refine a local outbound draft or suggestion candidate without
  direct GitHub posting
- suspend and resume the same review cleanly, including from a second terminal
  when that path is supported

Required memory assertions:

- the live review exposes truthful retrieval mode and scope information
- recalled items preserve provenance buckets and source identity
- broader overlays stay fenced unless explicitly enabled
- candidate-versus-promoted memory state stays explicit in the TUI
- lexical-only degraded behavior remains honest and still supports triage

Purpose:

- defend Roger's TUI-first operating model when review triage depends on prior
  review memory rather than only the current findings pack
- persona anchors: `PJ-03A`, `PJ-03C`, and `PJ-04A`
- invariant anchors: `INV-TUI-001`, `INV-TUI-002`, `INV-SEARCH-003`,
  `INV-SEARCH-004`
- executable proof today: none
- until the executable suite lands, the current cheaper owners are
  `int_search_prior_review_lookup` and `int_cli_session_aware`
- missing executable proof owner: `rr-6iah.2`

Execution posture:

- earn this slot only if the TUI-first journey still exposes a real
  multi-surface gap after `integration` coverage is strong
- keep non-interactive `--robot` equivalents in `integration` by default unless
  a later justification proves a full E2E is necessary

### E2E-04: Refresh and draft reconciliation after new commits

Status:

- budget-approved scenario slot; not yet implemented

Required shape:

- start from an active review that already has findings, triage state, and at
  least one local outbound draft
- introduce new commits or a rebase on the same review target
- run refresh through Roger's normal surface
- reconcile findings into at least `new`, `carried_forward`, and `resolved` or
  `stale` outcomes with preserved lineage
- prove draft state is either revalidated or marked for reconfirmation before
  post
- preserve audit history and return the operator to a truthful triage or drafts
  state

Purpose:

- defend Roger's refresh contract as a real product journey rather than as a
  pile of narrower reconciliation helpers
- persona anchors: `PJ-02D`, `PJ-04D`, and `PJ-05B`
- invariant anchors: `INV-POST-002`, `INV-POST-003`, `INV-HARNESS-003`
- executable proof today: none
- until the executable suite lands, the current cheaper owners are
  `prop_refresh_identity_lifecycle`, `int_github_posting_safety_recovery`, and
  `int_github_outbound_audit`
- missing executable proof owner: `rr-6iah.3`

Execution posture:

- preferred in `operator stability` or `nightly` once implemented
- most fingerprinting, invalidation, and anchor-remap rules still belong in
  `unit`, `prop_*`, and `integration`

### E2E-05: Browser setup and first PR-page launch

Status:

- implemented executable heavyweight E2E

Required shape:

- start from a machine that does not yet have Roger's browser companion path
  fully configured
- run `rr extension setup` through the product-facing command surface
- complete the one required manual browser load step while Roger discovers or
  self-registers the extension identity without manual typing
- register the installed `rr` binary as the Native Messaging host
- run `rr extension doctor` and prove the result is truthful
- launch from a supported GitHub PR page and land in the correct local Roger
  session

Purpose:

- defend Roger's first-use browser contract across setup, registration,
  companion truth, and first PR-page launch
- persona anchors: `PJ-01A`, `PJ-01B`, and `PJ-01C`
- invariant anchors: `INV-BRIDGE-001`, `INV-BRIDGE-002`,
  `INV-SESSION-001`
- executable proof today: `e2e_browser_setup_first_launch`
  (`packages/cli/tests/e2e_browser_setup_first_launch.rs`)
- deterministic setup/doctor + launch proof is now owned by the E2E lane above;
  branded-browser smoke owners remain
  `smoke_browser_launch_chrome`, `smoke_browser_launch_brave`,
  `smoke_browser_launch_edge` for browser-specific stability evidence
- executable proof owner bead: `rr-6iah.4.2`

Execution posture:

- preferred in `operator stability` or `release-candidate` on the supported
  browser matrix
- low-level bridge envelope, doctor, and theme/readability proof should remain
  in `integration` or smoke suites where possible

### E2E-06: Bare-harness dropout and return continuity

Status:

- budget-approved scenario slot; not yet implemented

Required shape:

- start a local Roger review on a supported provider path
- intentionally drop out to the underlying supported harness while preserving
  Roger control context
- inspect or continue the task in the bare harness without mutating approval or
  posting state directly
- return through `rr return` or the supported harness-native equivalent
- land back in the same Roger session with truthful continuity, findings, and
  task context intact

Purpose:

- defend Roger's OpenCode-first fallback story and the promise that the bare
  harness path remains a real way out and back
- persona anchors: `PJ-03C`, `PJ-04A`, and `PJ-04B`
- invariant anchors: `INV-SESSION-002`, `INV-CONTEXT-001`
- executable proof today: none
- until the executable suite lands, the current cheaper owners are
  `accept_opencode_dropout_return`, `accept_opencode_resume`,
  `smoke_opencode_continuity`, and `int_storage_opencode_dropout_return`
- missing executable proof owner: `rr-6iah.5`

Execution posture:

- preferred on the strongest supported direct-resume path first
- bounded providers may assert a truthful reduced form later rather than
  claiming parity too early

### High-value automated boundary paths

These should usually stay as integration-family suites or smoke tests rather
than becoming separate heavyweight E2Es.

#### Bridge-INT-01: Browser bridge happy path

Required shape:

- browser-originated PR launch
- bridge handoff through the chosen primary bridge
- local Roger target resolution
- truthful bridge response

Purpose:

- prove the companion/install story is real, not just the domain model

#### SMOKE-BRIDGE-CHROME-01: Chrome PR-page launch smoke

Required shape:

- launch a GitHub PR from Chrome through the serious bridge path
- capture launch-intent and bridge response transcripts
- verify Native Messaging happy path when registration is present
- verify truthful launch-only fallback guidance when Native Messaging is absent

Purpose:

- keep Chrome launch support explicit without adding a second heavyweight E2E

#### INT-CLI-ROBOT-01: Non-interactive review continuity

Required shape:

- run the bounded `--robot` review, resume, findings, or status path without
  the TUI
- verify stable machine-readable envelopes and fail-closed mutation behavior
- verify that any surfaced search or recall fields remain truthful about mode,
  scope, and degraded state

Purpose:

- keep automation and scripting support real without spending a heavyweight E2E
  slot on a path that should usually be defendable as `integration`

#### SMOKE-BRIDGE-BRAVE-01: Brave PR-page launch smoke

Required shape:

- launch a GitHub PR from Brave through the serious bridge path
- capture launch-intent and bridge response transcripts
- verify Native Messaging happy path when registration is present
- verify truthful launch-only fallback guidance when Native Messaging is absent

Purpose:

- keep Brave launch support explicit without adding a second heavyweight E2E

#### SMOKE-BRIDGE-EDGE-01: Edge PR-page launch smoke

Required shape:

- launch a GitHub PR from Edge through the serious bridge path
- capture launch-intent and bridge response transcripts
- verify Native Messaging happy path when registration is present
- verify launch-only honesty (no fake local status claims)
- verify truthful launch-only fallback guidance when Native Messaging is absent

Purpose:

- keep Edge launch support explicit without adding a second heavyweight E2E

Rules:

- these suites stay in targeted integration plus smoke coverage by default
- it must not be promoted into the heavyweight E2E lane unless the
  `0.1.x` E2E budget contract is explicitly changed
- fixture ownership for these scenarios is
  `fixture-bridge-transcripts` plus `fixture-bridge-launch-only-no-status`
- a Chrome-specific run is required when bridge host registration, launch
  envelopes, extension packaging, or Chrome launch support claims change
- a Brave-specific run is required when bridge host registration, launch
  envelopes, extension packaging, or Brave launch support claims change
- an Edge-specific run is required when bridge host registration, launch
  envelopes, extension packaging, or Edge support claims change
- shared-source coverage is sufficient only for docs-only or style-only changes
  with no launch-surface behavior deltas

#### Harness-INT-01: OpenCode dropout and return

Required shape:

- start a Roger review
- drop out intentionally to bare OpenCode
- continue with Roger control context
- return with `rr return` or equivalent

Purpose:

- prove fallback is operational, not marketing

#### SMOKE-OPENCODE-01: OpenCode direct continuity smoke

Required shape:

- start or resume a Roger review through the real OpenCode binary path
- prove locator reopen when available or truthful `ResumeBundle` reseed when the
  stored locator is stale
- intentionally drop to bare OpenCode without bypassing Roger posting gates
- return through `rr return` and retain the same Roger continuity context
- preserve session-locator evidence, reseed evidence when used, and the
  resulting return or status envelope for closeout

Purpose:

- keep Roger's first-class OpenCode continuity claim attached to a named
  real-provider smoke lane
- separate first-class OpenCode continuity proof from deterministic acceptance
  and integration coverage

Rules:

- `SMOKE-OPENCODE-01` is required when OpenCode support wording changes, when
  locator reopen or `rr return` behavior changes, when provider launch capture
  changes materially, or when Roger re-claims first-class OpenCode continuity
  after a breakage
- otherwise `accept_opencode_*`, `int_cli_opencode_transactional_*`, and the
  last representative `SMOKE-OPENCODE-01` artifact are sufficient
- Codex, Gemini, Claude Code, and feature-gated Copilot stay out of per-provider
  real-provider smoke until Roger widens them beyond bounded, non-first-class
  support wording

Codex, Gemini, Claude Code, and feature-gated Copilot should not each get their
own heavyweight automated E2E initially. They should keep bounded
provider-acceptance coverage until a later claim widens into a first-class
public support promise.

## Provider Acceptance Suites

### OpenCode acceptance

- locator-based reopen works when available
- ResumeBundle reseed works when reopen fails
- raw output and structured findings both persist
- bare-harness dropout and `rr return` work

### Bounded provider acceptance (`codex`, `claude`, `gemini`, feature-gated `copilot`)

- Roger can start a bounded-provider-backed review through the live CLI surface
- structured findings and/or raw output persist according to that provider's
  truthful current claim
- ResumeBundle reseed path works truthfully
- unsupported deeper capabilities fail clearly rather than pretending parity
- Codex, Gemini, and Claude Code remain bounded Tier A only
- Copilot remains feature-gated bounded Tier B: verified start,
  locator/session-id reopen, `rr return`, and honest `ResumeBundle` reseed
  fallback, but still no default public live claim
- no bounded, non-first-class provider gets a dedicated real-provider smoke lane
  in `0.1.0`; widen to smoke only in the same change that widens the public
  support claim

## Fixture Repos

Roger should maintain a small fixture set instead of inventing ad hoc test
corpora late.

Minimum fixture families:

- `fixture-small-review`: compact repo with one intentionally findable issue
- `fixture-monorepo`: multi-package repo for path/scope/config behavior
- `fixture-same-pr-multi-instance`: supports same-PR routing and worktree tests
- `fixture-malformed-findings`: provider outputs for partial/raw-only/repair
  paths
- `fixture-resumebundle-continuity`: reopen, reseed, and dropout continuity
  cases for supported harnesses
- `fixture-github-draft-payloads`: local outbound drafts, approval hashes, and
  posted-action payload snapshots
- `fixture-bridge-transcripts`: Native Messaging and browser launch-intent
  transcripts for launch-only, no-status, and bounded readback cases
- `fixture-migration-artifact-integrity`: migration, artifact-budget, and
  cold-store integrity cases
- `fixture-memory-scope`: repo/project/org overlay retrieval and abstention

These fixtures should back both integration tests and manual validation.
The shared fixture corpus and its harness entrypoints should be established
up front before provider acceptance, smoke suites, or heavier validation work
fans out.

## Manual Release Smoke Matrix

Manual release smoke is still required even with good CI.

Minimum manual smoke set:

- macOS `arm64`: OpenCode primary path (`SMOKE-OPENCODE-01`), browser launch,
  Native Messaging host registration
- Windows `x86_64`: Edge launch path, Native Messaging host registration,
  browser-extension install flow
- Linux `x86_64`: CLI/TUI path plus bridge/install truthfulness for the shipped
  support level

Supported-browser release rule:

- run `SMOKE-BRIDGE-CHROME-01`, `SMOKE-BRIDGE-BRAVE-01`, and
  `SMOKE-BRIDGE-EDGE-01` for release candidates whenever bridge host
  registration, launch payload handling, extension packaging, or browser
  support wording changed since the last passing smoke artifacts
- otherwise, previously passing browser smoke artifacts plus green shared-source
  bridge integration coverage are sufficient

First-class provider release rule:

- run `SMOKE-OPENCODE-01` for release candidates whenever OpenCode support
  wording, locator reopen or reseed semantics, dropout or return control flow,
  or provider launch capture changed since the last passing smoke artifact
- otherwise, green `accept_opencode_*` and `int_cli_opencode_transactional_*`
  coverage plus the last representative `SMOKE-OPENCODE-01` artifact are
  sufficient
- bounded-provider release proof remains `accept_codex_*`,
  `accept_gemini_*`, `accept_claude_*`, and `accept_copilot_*` plus
  provider-surface truth coverage; do not invent per-provider real-provider
  smoke until the support wording widens into a first-class public claim

## What Must Be Stubbed

To keep tests valuable and stable:

- GitHub mutation should use a Roger-owned adapter double in most automated
  tests
- structured findings repair paths should use canned provider outputs
- search/index corruption tests should use seeded local fixtures, not live model
  downloads

## What Must Stay Real Somewhere

- at least one real provider-backed review path
- at least one real browser-to-local bridge path
- one real resume/dropout path for OpenCode

## Explicit `0.1.0` Non-Goals

- exhaustive CI coverage for every browser/OS/provider combination
- Tier B parity for Codex, Gemini, or Claude Code before the implementation earns it
- browser-store publication as a product gate
- Gemini parity with OpenCode on native reopen semantics
