# Release And Test Matrix

This document turns the current plan into a common-sense support and validation
matrix for Roger `0.1.0`.

It is intentionally opinionated. The goal is to choose a small number of
high-value release targets and tests rather than pretending every combination is
equally important.

The canonical plan remains
[`PLAN_FOR_ROGER_REVIEWER.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/PLAN_FOR_ROGER_REVIEWER.md).

The implementation-facing harness contract lives in
[`TEST_HARNESS_GUIDELINES.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/TEST_HARNESS_GUIDELINES.md).
The automated E2E budget file lives in
[`AUTOMATED_E2E_BUDGET.json`](/Users/cdilga/Documents/dev/roger-reviewer/docs/AUTOMATED_E2E_BUDGET.json).

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
| OpenCode | Primary | Real locator-based resume, Roger ledger integration, bare-harness dropout, `rr return` |
| Codex | Secondary, bounded | Exposed via `rr review --provider codex`; truthful Tier A reseed/raw-capture path, no locator reopen or `rr return` claim |
| Claude | Secondary, bounded | Exposed via `rr review --provider claude`; truthful Tier A reseed/raw-capture path, no locator reopen or `rr return` claim |
| Gemini harness | Secondary, bounded | Exposed via `rr review --provider gemini`; truthful Tier A reseed/raw-capture path, no locator reopen or `rr return` claim |
| GitHub Copilot CLI | Active implementation scope, not yet live | Do not claim support until verified launch, policy, and continuity coverage are real |
| Pi-Agent | Not in `0.1.0` | Contract-shaping only |

Bounded-provider coverage in `0.1.0` should stay common-sense:

- Roger owns the continuity model
- Codex, Claude, and Gemini do not require transcript-isomorphic resume parity
  with OpenCode to earn truthful Tier A claims
- if a bounded provider lacks a stable reopen path, Roger should still support
  truthful reseed/resume through `ResumeBundle` without widening the claim

Provider claim rule:

- Roger should only claim **bounded support** for a harness that satisfies the
  Tier A contract
- Roger should only claim **direct-resume or dropout support** for a harness
  that satisfies the Tier B contract
- Roger should only claim **in-harness Roger command support** for a harness
  that actually exposes the relevant Tier C affordances

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
| Core Rust binaries | macOS `arm64`, macOS `x86_64`, Windows `x86_64`, Windows `arm64`, Linux `x86_64`, Linux `arm64` |
| Extension package | Chrome, Brave, Edge from one source base |
| Bridge install docs | macOS, Windows, Linux |

If Linux browser integration proves materially weaker at first, Roger should be
truthful about it rather than silently dropping Linux from the documented matrix
late.

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
| `release` `build-core` | Release pipeline | Build versioned Rust binaries for the supported OS/arch matrix and stage raw archives | Per-target core companion archives |
| `release` `package-bridge` | Release pipeline | Generate Native Messaging manifests, platform registration helpers, and bridge install/uninstall bundles for supported OS targets | Per-OS bridge registration bundles |
| `release` `package-extension` | Release pipeline | Produce browser-installable extension packages from the shared source base and stamp them with the release version/source revision | Extension sideload packages for Chrome/Brave/Edge |
| `release` `verify-release-assets` | Release pipeline | Recompute checksums, verify archive contents, confirm release manifest completeness, and enforce publish gates | Verified `SHA256SUMS` and release asset manifest |
| `release` `publish-release` | Release pipeline with explicit maintainer approval | Attach approved artifacts to the versioned release and publish notes from the same workflow run | Published GitHub release and notes |

Ownership rules:

- `release` keeps one top-level workflow while preserving job-level ownership
- `build-core` owns compilation, but not publication
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
- `--version 0.1.0` is a product-line alias that resolves to the latest stable
  published CalVer tag; explicit CalVer pins remain `YYYY.MM.DD[-rc.N]`
- host OS and CPU detection map to the published core companion archive matrix
  and must fail clearly for unsupported targets rather than guessing
- the installer downloads the chosen core companion archive plus the release
  metadata bundle, verifies the artifact against published `SHA256SUMS`, and
  aborts on any mismatch
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
- post-publish live stable smoke (manual release lane):
  - `curl -fsSL https://api.github.com/repos/cdilga/roger-reviewer/releases/latest`
  - `bash scripts/release/rr-install.sh --repo cdilga/roger-reviewer --dry-run`
  - record UTC timestamp + resolved stable tag in release closeout evidence
- PowerShell installer validation is currently a manual smoke on a Windows host
  until a stable `pwsh` lane is available in this workspace

### Update lane

`0.1.0` implementation status:

- `rr update` is the Roger-owned updater in `packages/cli` and performs
  in-place binary replacement against published CalVer release metadata
- default apply behavior is confirmation-gated on an interactive TTY
- `--yes` / `-y` bypasses only the confirmation prompt for non-interactive
  apply; artifact/provenance/safety checks still run
- `--dry-run` and `--robot` remain non-mutating metadata/preflight paths; in
  `--robot` mode, apply is blocked unless `--yes` / `-y` is provided
- local/unpublished builds are blocked and require explicit reinstall from a
  published CalVer release before update can run

Behavior rules:

- default update behavior stays on the selected published channel (`stable` or
  `rc`) and upgrades only to a newer published version in that lane
- an explicit pinned target version is allowed
- the updater path reuses the same host detection and install metadata +
  manifest + checksum verification rules as install and fails closed on missing
  metadata, metadata/manifest drift, checksum mismatch, or ambiguous target
  resolution
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

## Blessed Automated Paths

Roger should not start with many slow end-to-end tests. It should start with a
small set that covers the real failure boundaries.

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

Rules:

- this is the one blessed automated end-to-end test for `0.1.x`
- any additional automated E2E needs explicit justification that lower-level
  unit or integration coverage cannot defend the same product
  promise more cheaply
- Roger should track the blessed automated E2E count in a small Roger-owned
  budget file or manifest and emit an agent-facing warning when that count rises
- that warning should explicitly ask whether the author can defend the new
  behavior with a smaller test or whether they are taking the lazy route to
  another expensive E2E
- after the warning-only phase, CI should be allowed to fail additions that lack
  a recorded justification
- if a future E2E defends a memory-assisted journey, it must assert truthful
  retrieval mode, explicit scope buckets, preserved provenance, and degraded
  lexical-only fallback where semantic retrieval is unavailable

### High-value automated boundary paths

These should usually stay as integration, acceptance, or smoke tests rather
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

- these suites stay in targeted smoke/acceptance coverage by default
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

Codex, Claude, and Gemini should not each get their own heavyweight automated
E2E initially. They should get bounded provider-acceptance coverage plus smoke
paths commensurate with the claim Roger is actually making.

## Provider Acceptance Suites

### OpenCode acceptance

- locator-based reopen works when available
- ResumeBundle reseed works when reopen fails
- raw output and structured findings both persist
- bare-harness dropout and `rr return` work

### Bounded provider acceptance (`codex`, `claude`, `gemini`)

- Roger can start a bounded-provider-backed review through the live CLI surface
- structured findings and/or raw output persist according to that provider's
  truthful current claim
- ResumeBundle reseed path works truthfully
- unsupported deeper capabilities fail clearly rather than pretending parity

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

- macOS `arm64`: OpenCode primary path, browser launch, Native Messaging host
  registration
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
- Tier B parity for Codex, Claude, or Gemini before the implementation earns it
- browser-store publication as a product gate
- Gemini parity with OpenCode on native reopen semantics
