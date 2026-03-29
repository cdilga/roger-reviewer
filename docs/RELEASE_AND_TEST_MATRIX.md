# Release And Test Matrix

This document turns the current plan into a common-sense support and validation
matrix for Roger `0.1.0`.

It is intentionally opinionated. The goal is to choose a small number of
high-value release targets and tests rather than pretending every combination is
equally important.

The canonical plan remains
[`PLAN_FOR_ROGER_REVIEWER.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/PLAN_FOR_ROGER_REVIEWER.md).

## Engineering Posture

Rules:

- prefer one blessed path over a wide shallow support claim
- automate the highest-risk workflow first, not the easiest one
- treat OS/browser/provider packaging as product work, not release cleanup
- treat Native Messaging as the serious `0.1.0` bridge and custom URL only as
  a thin launch/recovery path
- keep the support matrix explicit so unsupported combinations are truthful
- use stubs and fixtures where they increase determinism, but keep at least one
  real boundary test for each major external surface

## `0.1.0` Provider Matrix

| Provider | `0.1.0` status | Minimum expectation |
|----------|----------------|---------------------|
| OpenCode | Primary | Real locator-based resume, Roger ledger integration, bare-harness dropout, `rr return` |
| Gemini harness | Secondary, bounded | Roger-owned session/run ledger, prompt intake, structured/raw output capture, ResumeBundle reseed path |
| Codex | Not in `0.1.0` | Contract-shaping only |
| Claude | Not in `0.1.0` | Contract-shaping only |
| Pi-Agent | Not in `0.1.0` | Contract-shaping only |

Gemini support in `0.1.0` should stay common-sense:

- Roger owns the continuity model
- Gemini support does not require transcript-isomorphic resume parity with
  OpenCode
- if Gemini lacks a stable reopen path, Roger should still support truthful
  reseed/resume through `ResumeBundle`

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
| Custom URL | Convenience path | Keep for thin launch and recovery, not as the only serious bridge |
| Chrome | Supported | Same extension source base |
| Brave | Supported | Same extension source base |
| Edge | Supported and first-class | Must not be treated as "probably similar enough to Chrome" |

Custom URL may remain useful for thin launch and recovery, but it does not by
itself satisfy Roger's companion-tier bridge claims.

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
| Core Rust binaries | macOS `arm64`, macOS `x86_64`, Windows `x86_64`, Windows `arm64`, Linux `x86_64` |
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
| Core companion archive | Yes | macOS `arm64`, macOS `x86_64`, Windows `x86_64`, Windows `arm64`, Linux `x86_64` | Versioned archive containing the `rr` binary and minimal local runtime assets |
| Bridge registration bundle | Yes where Roger claims browser-launch support on that OS | macOS, Windows, Linux | Native Messaging manifest templates plus Roger-owned install/uninstall helpers and any custom-URL registration assets |
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

Roger should split release automation by responsibility so build, packaging, and
publication remain inspectable.

| Job | Ownership | Responsibilities | Outputs |
|-----|-----------|------------------|---------|
| `ci-verify` | Continuous integration on every PR and release candidate | Lint, unit/integration suites, packaging smoke, artifact naming checks, checksum-manifest shape validation | Validation only; no published assets |
| `release-build-core` | Release pipeline | Build versioned Rust binaries for the supported OS/arch matrix and stage raw archives | Per-target core companion archives |
| `release-package-bridge` | Release pipeline | Generate Native Messaging manifests, platform registration helpers, and bridge install/uninstall bundles for supported OS targets | Per-OS bridge registration bundles |
| `release-package-extension` | Separate optional release lane | Produce browser-installable extension packages from the shared source base and stamp them with the release version/source revision | Extension sideload packages for Chrome/Brave/Edge |
| `release-verify-assets` | Release pipeline | Recompute checksums, verify archive contents, confirm release manifest completeness, and enforce signing policy gates that are active for the target | Verified `SHA256SUMS` and release asset manifest |
| `release-publish` | Release pipeline with explicit maintainer approval | Attach approved artifacts to the versioned release, publish notes, and mark which artifact lanes are shipped for that tag | Published GitHub release and notes |

Ownership rules:

- `release-build-core` owns compilation, but not publication
- `release-package-bridge` owns Native Messaging and custom-URL registration
  assets, but not browser-extension packaging
- `release-package-extension` is intentionally separate so the browser lane can
  advance or pause without muddying the core local-product release lane
- `release-publish` should publish only the artifact classes that actually
  passed verification; it must not imply extension availability if the extension
  lane was skipped for that tag
- manual release work should be limited to explicit approval and smoke checks,
  not ad hoc asset assembly

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
- host OS and CPU detection map to the published core companion archive matrix
  and must fail clearly for unsupported targets rather than guessing
- the installer downloads the chosen core companion archive plus the release
  metadata bundle, verifies the artifact against published `SHA256SUMS`, and
  aborts on any mismatch
- install success yields a usable local `rr` binary without requiring the
  browser extension, Native Messaging registration, or store publication
- platform-specific install paths and PATH guidance are Roger-owned release-doc
  work, not hidden Homebrew, winget, or npm assumptions

### Update lane

After installation, the blessed one-line update flow is Roger-owned:

- `rr self-update`

Behavior rules:

- default behavior stays on the current release channel and upgrades only to a
  newer published version in that lane
- an explicit pinned target version is allowed
- the updater reuses the same host detection and checksum-verification rules as
  install and fails closed on checksum mismatch, missing metadata, or ambiguous
  provenance
- installs created from local/unpublished artifacts should not be upgraded as
  if they were blessed release installs; Roger should require an explicit
  reinstall from a published release in that case

### Separate optional extension lane

- bridge registration bundles and extension sideload packages remain a separate
  post-install workflow
- Roger may expose explicit follow-up commands such as `rr bridge install`, but
  those commands are outside the base one-line local-product contract
- release notes must not imply browser-launch support on a target unless the
  matching bridge registration bundle and extension package were actually
  shipped for that release

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
  unit, parameterized, or integration coverage cannot defend the same product
  promise more cheaply
- Roger should track the blessed automated E2E count in a small Roger-owned
  budget file or manifest and emit an agent-facing warning when that count rises
- that warning should explicitly ask whether the author can defend the new
  behavior with a smaller test or whether they are taking the lazy route to
  another expensive E2E
- after the warning-only phase, CI should be allowed to fail additions that lack
  a recorded justification

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

#### Harness-INT-01: OpenCode dropout and return

Required shape:

- start a Roger review
- drop out intentionally to bare OpenCode
- continue with Roger control context
- return with `rr return` or equivalent

Purpose:

- prove fallback is operational, not marketing

Gemini should not get its own heavyweight E2E initially. It should get a
bounded adapter acceptance suite plus one smoke path proving prompt intake,
structured/raw capture, and ResumeBundle-driven reseed.

## Provider Acceptance Suites

### OpenCode acceptance

- locator-based reopen works when available
- ResumeBundle reseed works when reopen fails
- raw output and structured findings both persist
- bare-harness dropout and `rr return` work

### Gemini acceptance

- Roger can start a Gemini-backed review through the adapter
- structured findings and raw output both persist
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
- `fixture-memory-scope`: repo/project/org overlay retrieval and abstention

These fixtures should back both integration tests and manual validation.

## Manual Release Smoke Matrix

Manual release smoke is still required even with good CI.

Minimum manual smoke set:

- macOS `arm64`: OpenCode primary path, browser launch, Native Messaging host
  registration
- Windows `x86_64`: Edge launch path, Native Messaging host registration,
  browser-extension install flow
- Linux `x86_64`: CLI/TUI path plus bridge/install truthfulness for the shipped
  support level

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
- Codex, Claude, or Pi-Agent provider support
- browser-store publication as a product gate
- Gemini parity with OpenCode on native reopen semantics
