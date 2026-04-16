# Update Release And Tested Upgrade Contract

This document freezes Roger's `0.1.x` update contract at the boundary between:

- install/update semantics exposed to operators
- GitHub Releases asset mechanics
- the proof needed before Roger claims a tested upgrade path

It complements, but does not replace:

- [`PLAN_FOR_ROGER_REVIEWER.md`](PLAN_FOR_ROGER_REVIEWER.md)
- [`RELEASE_AND_TEST_MATRIX.md`](RELEASE_AND_TEST_MATRIX.md)
- [`RELEASE_CALVER_VERSIONING_CONTRACT.md`](RELEASE_CALVER_VERSIONING_CONTRACT.md)
- [`STORE_MIGRATION_COMPATIBILITY_AND_OPERATOR_CONTRACT.md`](STORE_MIGRATION_COMPATIBILITY_AND_OPERATOR_CONTRACT.md)

The migration contract remains the authority for schema compatibility envelopes,
migration classes, and first-run migration fail-closed rules. This document owns
the broader update lane: release discovery, asset requirements, install-layout
expectations, updater mechanics, and upgrade-path proof.

## Scope And Authority

When install/update behavior, release notes, README language, or release
automation disagree, this contract narrows the support claim to the truthful
minimum.

For `0.1.x`, this contract has higher authority than scattered prose in:

- [`PLAN_FOR_ROGER_REVIEWER.md`](PLAN_FOR_ROGER_REVIEWER.md) install/update sections
- [`RELEASE_AND_TEST_MATRIX.md`](RELEASE_AND_TEST_MATRIX.md) install/update prose
- [`README.md`](../README.md) install/update examples

If code and this contract disagree, either:

1. fix the code and keep the stronger contract, or
2. narrow the contract and public docs before claiming support

Do not widen support from installer existence alone.

## Current Repo Truth

As of `2026-04-14`, the repo has these real facts:

- `rr update` exists in `packages/cli` and performs published-release metadata
  validation, checksum validation, confirmation-gated apply, and
  rename-with-rollback replacement for supported in-place installs.
- the Unix installer `scripts/release/rr-install.sh` exists and has automated
  synthetic-release tests.
- the PowerShell installer `scripts/release/rr-install.ps1` now follows the
  same checksum-manifest contract as the Unix installer for canonical
  `SHA256SUMS` releases and legacy published tags whose metadata still names a
  per-release checksum file, but Windows proof remains manual in this
  workspace.
- release automation already verifies core assets, installer-script presence,
  optional-lane claim drift, and publish-gate inputs before publication.
- schema/data migration-capable updates remain deferred/fail-closed in `0.1.x`.

Those facts are necessary, but they are not yet a complete tested-upgrade
contract.

## Gaps Identified In The Current Repo

These are the concrete gaps this contract closes or makes explicit.

### 1. Authority fragmentation

Update semantics are currently split across the canonical plan, release/test
matrix, migration contract, README, installer scripts, updater code, and
release scripts. That makes it too easy to over-claim support from one surface
that another surface does not actually defend.

### 2. Channel-default drift

The plan prose says `rr update` should stay on the current installed release
channel by default. The live CLI parser currently defaults `--channel` to
`stable`, regardless of the installed binary's embedded release channel.

Until current-channel stickiness is implemented and tested, Roger must not
claim it.

### 3. Target-support drift

The Unix installer auto-detects and tests Linux `arm64` / `aarch64`, but
`rr update` does not auto-detect that target today. Roger therefore cannot
truthfully claim the same in-place update target matrix as the installer lane.

### 4. Installer parity drift

The checksum-manifest lane now aligns across the Unix and PowerShell installers
for both canonical `SHA256SUMS` releases and the legacy published-tag shape
where install metadata still names `<artifact>-checksums.txt`. Other installer
parity gaps remain: the Unix installer supports the `--version 0.1.0` stable
alias and has retained automated synthetic-feed tests, while the PowerShell
lane still relies on manual Windows-host proof in this workspace.

### 5. Install-layout semantics are under-specified

The updater only supports a direct regular-file install layout whose executable
name matches the release binary (`rr` or `rr.exe`). Symlinked installs, renamed
copies, and wrapper-based layouts fail closed today, but this boundary is not
yet made first-class in public update semantics.

### 6. Repair guidance drift

This gap is now closed in source: `rr update` emits release-hosted reinstall
commands instead of repo-relative `scripts/release/rr-install.*` paths. The
remaining work is retention and replay discipline, not command-shape truth.

### 7. Tested-upgrade-path gap

The repo has installer tests and updater-unit coverage, but it does not yet
have one explicit required rehearsal that proves this whole sequence:

1. install a published-like old release through the official installer surface
2. run the installed old `rr`
3. preflight and apply `rr update`
4. verify the updated binary is usable afterward

Until that rehearsal exists and is retained as release evidence, Roger should
describe its update lane as validated in pieces, not as a comprehensively
rehearsed upgrade path.

### 8. Release-gate checksum contradiction

This gap is now closed in source: install metadata, release verification, and
published asset naming all converge on `SHA256SUMS` as the canonical checksum
manifest. The installer and updater keep an explicit legacy fallback path so
already-published tags that still declare `<artifact>-checksums.txt` remain
consumable until the stable line is refreshed.

### 9. Published-artifact versus repo-doc drift

The current repo source and support docs describe update semantics that the
latest published stable binary does not yet expose. In particular, the latest
published `v2026.04.08` artifact:

- does not accept `rr update --yes` / `-y`
- does not list update commands in `rr robot-docs guide --robot`

Until a published release actually carries those semantics, Roger must not let
release-hosted install/update docs imply that the installed binary already does.

### 10. Release-smoke coverage gap

The documented publish smoke currently checks `releases/latest` resolution and a
live Unix installer dry-run, but it does not prove that a freshly installed
published binary can run `rr update --dry-run --robot` against the same release.

That omission is why the current checksum-contract break can survive publish.

## What Happens If A User Runs `rr update` Right Now

This section is intentionally concrete. It describes the live `0.1.x` behavior
observed on `2026-04-14`, not the intended end state.

### Walkthrough: published stable install on macOS `arm64`

Assume a normal operator has:

- a published Roger binary installed at a direct path such as
  `/Users/<user>/.local/bin/rr`
- a normal stable-install history
- no explicit `--version`, `--channel`, or `--target`

Observed live sequence:

1. `rr update --dry-run --robot` starts from the installed binary.
2. the binary is allowed past the provenance gate because it is a published
   release install, not a local/unpublished build.
3. `rr update` resolves the target channel as `stable`.
4. it resolves the target release from GitHub `releases/latest`; at the time of
   writing, that is `v2026.04.08`.
5. it fetches `release-install-metadata-2026.04.08.json` successfully.
6. it reads `checksums_name` from that metadata. Older published bundles may
   still say `roger-reviewer-<version>-checksums.txt`, while current source
   now emits `SHA256SUMS`.
7. it attempts to fetch the declared checksums file first.
8. if the declared asset is absent and the canonical `SHA256SUMS` exists, the
   updater falls back to `SHA256SUMS` and continues validation.
9. if neither manifest exists, `rr update` blocks immediately with:
   - `schema_id=rr.robot.update.v1`
   - `outcome=blocked`
   - `reason_code=checksums_missing`

This happens before:

- the updater can conclude “already up to date”
- confirmation logic matters
- install-layout checks matter
- apply/rollback logic matters

That means the first hard failure in the current stable updater path is the
release checksum contract, not the binary replacement step.

### Walkthrough: fresh install of the current latest release

There is a more severe consequence: a user can install the latest stable release
successfully via the Unix installer and still get a broken self-update path
immediately afterward.

Observed live sequence:

1. `curl -fsSL https://github.com/cdilga/roger-reviewer/releases/latest/download/rr-install.sh | bash -s -- --install-dir <tmp>`
   succeeds.
2. the installer succeeds because Roger now treats `SHA256SUMS` as the
   canonical checksum manifest and retains a legacy fallback for older published
   metadata.
3. the freshly installed `<tmp>/rr update --dry-run --robot` should now
   complete a same-version no-op or cross-version dry-run without a
   `checksums_missing` contract break, provided the release still ships either
   the declared checksums file or `SHA256SUMS`.

### Why the Unix installer works while `rr update` fails

Current live split:

- `rr update` now tries the metadata-declared checksums asset first, then falls
  back to `SHA256SUMS` when that canonical manifest is present.
- the Unix installer and PowerShell installer follow the same checksum-manifest
  rule.

That removes the prior three-way split in checksum behavior. Remaining parity
gaps are documented elsewhere in this contract.

## End-To-End Flow And Blocker Ledger

This ledger follows the update path from the first release input through the
live operator command surface. Each blocker is concrete and currently observed,
not theoretical.

| Flow step | Current mechanism | Observed blocker note |
| --- | --- | --- |
| 1. Derive release identity | `derive_calver_version.py` creates the tag/version/artifact stem tuple consumed by the release workflow | No blocker found here in current tracing. |
| 2. Build core archives | `release.yml` `build-core` compiles and packages per-target `rr` archives | Blocker: the workflow does not actually run the packaged-binary smoke that [`RELEASE_AND_TEST_MATRIX.md`](RELEASE_AND_TEST_MATRIX.md) says `build-core` should defend (`rr --help`, `rr robot-docs`, `rr update --dry-run --robot`). |
| 3. Aggregate install metadata | `build_install_metadata_bundle.py` writes the installer/updater routing bundle | Blocker: the bundle hardcodes `checksums_name = <artifact-stem>-checksums.txt`, creating a checksum identity that later publish steps do not preserve. |
| 4. Verify release assets | `verify_release_assets.py` checks bundle/core-manifest agreement and emits verification outputs | Blocker: the verifier enforces the per-release checksum filename from step 3, but then writes `SHA256SUMS` as the only verified checksum artifact, so the verify lane itself preserves both sides of the contradiction. |
| 5. Build publish plan and notes | `publish_release.py`, `build_release_notes.sh`, and `release.yml` publish from `upstream/verify-report/SHA256SUMS` | Blocker: the published release notes and asset set standardize on `SHA256SUMS`, not the metadata-declared checksums file the updater later requires. |
| 6. Post-publish stable smoke | [`release-publish-operator-smoke.md`](release-publish-operator-smoke.md) defines the required manual smoke | Blocker: the smoke does not run a fresh installed binary through `rr update --dry-run --robot`, so self-update breakage is not part of the publish gate today. |
| 7. Fresh Unix install from live release | `rr-install.sh` resolves latest/pinned metadata and installs successfully on Unix | Current source aligns canonical checksum naming on `SHA256SUMS` and retains legacy fallback for older published tags; live stable proof still has to be retained per release. |
| 8. Fresh Windows install from live release | `rr-install.ps1` resolves the same metadata-driven asset set | Current source now shares the checksum-manifest fallback logic with Unix, but Windows-host proof remains manual until a stable `pwsh` lane is available here. |
| 9. In-place updater dry-run | installed `rr` reads install metadata, manifests, and checksums before no-op/apply decisions | Current source now falls back from legacy metadata-declared checksum names to `SHA256SUMS`; live release proof still has to be retained per stable tag. |
| 10. Non-interactive apply surface | current source exposes `--yes/-y` and robot docs for update discovery | Blocker: the latest published stable binary does not expose those flags or discovery entries yet, so repo/public docs must not treat current source semantics as already shipped release semantics. |
| 11. Install/update repair guidance | blocked update envelopes should point users at a real reinstall path | Current source emits release-hosted reinstall commands; the remaining obligation is retaining those blocked-envelope proofs when the support claim is widened. |

## Canonical Update Surface

Roger exposes three distinct operator-facing actions. They must not be blurred
together.

### 1. Install

Install means bootstrap `rr` onto a machine that does not already have a Roger
release install in the target directory.

Canonical public entrypoints:

- Unix-like: release-hosted `rr-install.sh`
- Windows: release-hosted `rr-install.ps1`

Install resolves a release target, downloads the release assets, verifies the
metadata/manifests/checksums contract, extracts the archive, and copies the
binary into the chosen install directory.

### 2. Reinstall

Reinstall means replacing an existing binary by re-running the installer lane.
It is a valid recovery path even when `rr update` is blocked.

Reinstall is not the same thing as the in-place updater and does not inherit
the updater's rollback guarantee unless that behavior is explicitly added and
tested for the installer lane.

### 3. In-Place Update

In-place update means running `rr update` from an already installed Roger
release binary.

For `0.1.x`, this is the only Roger-owned path that may claim:

- confirmation-gated mutation
- install-layout inspection
- rename-with-backup replacement
- immediate rollback restore on replacement failure

## Channel, Version, And Release Discovery Semantics

Release identity remains defined by
[`RELEASE_CALVER_VERSIONING_CONTRACT.md`](RELEASE_CALVER_VERSIONING_CONTRACT.md).
This contract narrows how that identity is consumed by install/update flows.

### Stable/latest

- GitHub `releases/latest` is the canonical stable-resolution surface.
- release-hosted `latest/download/rr-install.*` URLs are the canonical stable
  installer entrypoints.
- stable/latest is only a truthful promise when post-publish proof confirms the
  `releases/latest` endpoint resolves to the intended stable tag.

### RC lane

- RC discovery must resolve from the GitHub Releases feed using prerelease
  entries only.
- Roger must not move a user onto RC bits implicitly.
- RC installs/updates require explicit operator choice through `--channel rc`
  or an explicit pinned RC version.

### Pinned versions

- the canonical pinned format is `YYYY.MM.DD[-rc.N]`
- installers may accept `vYYYY.MM.DD[-rc.N]` input only as normalization
- `rr update` support claims apply only to explicit CalVer pins; it must not
  claim the `0.1.0` alias unless that alias is intentionally added and tested

### `0.1.0` alias

- the `0.1.0` alias is a product-line alias for installers only
- it resolves to the latest stable published CalVer tag
- it must remain stable-only
- Roger must not document this alias as cross-shell or cross-command behavior
  unless every official installer and any advertised updater path implement it

### Default channel for `rr update`

Truthful current support claim:

- until current-channel stickiness is implemented and tested, `rr update`
  defaults to the `stable` lane unless the operator passes `--channel rc`

If Roger later lands channel stickiness, this contract should be updated at the
same time as the parser/help text, tests, and public docs.

## GitHub Releases Asset Contract

Every published tag that Roger expects installers or the updater to consume must
contain one coherent release asset set.

### Required base assets

For the core local-product lane, the GitHub release must contain:

- target-specific core archives for the claimed supported targets
- `release-core-manifest-<version>.json`
- `release-install-metadata-<version>.json`
- `SHA256SUMS`
- `rr-install.sh`
- `rr-install.ps1`
- signing notes / provenance notes owned by the release lane
- release notes generated from the verified publish plan

Optional bridge/extension artifacts are separate lanes. Their presence is
required only when the release claims those optional surfaces as shipped.

### Asset-consistency rules

- the install metadata bundle is the canonical installer/updater routing object
- the core manifest must agree with the install metadata target entry for
  archive name, checksum, payload directory, and binary name
- `SHA256SUMS` is the canonical published checksum manifest for release proof
- installer entrypoints, release notes, and publish-plan artifacts must all
  point at the same tag/version tuple
- optional-lane support claims must match the actual uploaded optional assets

### Fallback policy

Roger currently has one legacy checksum-fallback behavior in the Unix installer.
That is not enough to claim a general fallback policy.

Until installers and updater are aligned, the support contract is:

- `SHA256SUMS` is the only published checksum file Roger requires operators to
  reason about
- any backward-compatibility fallback beyond that is implementation detail, not
  public contract

## Supported Target Matrix Must Be Surface-Specific

Roger must distinguish:

1. base install target support
2. in-place update target support

Those are related, but not identical.

### Base install support

Roger may claim a base install target only when:

- the release lane publishes the target archive
- release verification includes it in the verified asset manifest
- the official installer for that OS family can resolve and install it

### In-place update support

Roger may claim in-place update support for a target only when:

- `rr update` can resolve that target without guesswork or unsupported manual
  overrides, or the docs explicitly say `--target` is required
- the updater path validates and applies the release archive for that target
- at least one rehearsal covers that target or the target family contract

Truthful current narrowing:

- do not claim Linux `arm64` in-place update parity with the installer lane
  until `rr update` supports and validates it directly

## User, Environment, And History Matrix

Roger does not have one generic update user. The contract must name the major
cohorts explicitly.

| User cohort | Current outcome if they run `rr update` | First hard issue | Truthful recovery today |
| --- | --- | --- | --- |
| Published stable install, direct binary, macOS `arm64` or Linux/macOS `x86_64` | Blocked today on current stable line | release checksum contract mismatch (`checksums_missing`) | release must publish the metadata-declared checksums asset, or updater must align to `SHA256SUMS`; reinstall via release-hosted installer is the only practical user path |
| Fresh Unix installer user on latest stable | Install succeeds, update check still blocked | Unix installer fallback hides release mismatch; updater does not | same as above; the release is installable but not updater-self-consistent |
| Windows user using `rr-install.ps1` on latest stable | Likely blocked at install | PowerShell installer has no demonstrated fallback for the missing metadata-declared checksums file | use a corrected published installer/release asset set; do not assume Windows latest-install parity with Unix latest-install |
| Linux `arm64` / `aarch64` published install user | Blocked by default even after checksum issue is fixed | updater host auto-detect does not cover Linux `aarch64` | pass `--target aarch64-unknown-linux-gnu` explicitly, or use reinstall until updater parity lands |
| RC user who expects “stay on my channel” | Misrouted unless they remember `--channel rc` | updater defaults to `stable`, not installed-channel stickiness | pass `--channel rc` or pin an explicit RC version |
| User on local/unpublished binary or source-checkout build | Hard-blocked immediately | no embedded published-release metadata | reinstall from a published release; source-checkout users are on the developer path, not the updater path |
| User with a symlinked `rr`, renamed binary, wrapper, or package-manager shim | Hard-blocked after preflight gets that far | unsupported install layout | replace with a direct `rr` / `rr.exe` release-binary install, then rerun update |
| User with a malformed or incompatible local store | Blocked if store preflight is reached | migration/store-schema probe fails or reports unsupported posture | repair/export/remove local state first; updater must not mutate through an unknown migration boundary |
| User without local `curl` / `tar` prerequisites | Blocked with misleading fetch/extraction style errors | updater/install runtime prerequisites are implicit in code, not first-class diagnostics | install prerequisites first; contract and docs need explicit prerequisite language |
| User already on the current release version | Should get an up-to-date no-op, but may not | broken release metadata can block before same-version detection completes | release asset contract must be healthy before even no-op update checks are trustworthy |

## Update Truth-Maintenance Rules

The update surface is release-critical and must follow the same proof posture as
the rest of Roger's truth-maintenance system.

Rules:

- every update support claim or documented user cohort must map to one or more
  invariant ids in [`VALIDATION_INVARIANT_MATRIX.md`](VALIDATION_INVARIANT_MATRIX.md)
- the cohort list in this document is part of the support surface, not just an
  explanatory appendix
- if a platform, shell family, install history, or recovery path is omitted
  from the cohort/proof matrix below, Roger must not imply that it is supported
- if a cohort has only partial proof, the release wording must narrow that
  cohort explicitly rather than inheriting parity from a different surface
- release closeout for install/update claims must retain machine-derivable
  evidence, not only prose notes

## Cohort Proof Matrix

This matrix is the update-lane equivalent of the proof ladder in
[`TESTING.md`](TESTING.md). It is the minimum truth-maintenance map Roger needs
before widening install/update claims.

| Cohort / surface slice | Promise being claimed | Invariant ids | Primary proof lane | Minimum evidence required before widening claim | Current posture |
| --- | --- | --- | --- | --- | --- |
| Unix latest stable install | release-hosted Unix installer can resolve and dry-run the latest stable release truthfully | `INV-UPDATE-001`, `INV-UPDATE-003` | `integration` + `smoke` | `bash scripts/release/test_rr_install.sh`, live `releases/latest` probe, live Unix installer dry-run output | partially defended |
| Windows latest stable install | release-hosted PowerShell installer can resolve and dry-run the latest stable release truthfully | `INV-UPDATE-001`, `INV-UPDATE-003` | `integration` + `smoke` | PowerShell installer parity tests or retained Windows-host dry-run evidence against the live release | under-defended |
| Published stable direct-binary update | installed stable release can preflight and apply a newer stable release without hidden target/provenance drift | `INV-UPDATE-002`, `INV-UPDATE-003`, `INV-UPDATE-004` | `integration` + `smoke` | updater dry-run envelope, `bash scripts/release/test_update_upgrade_rehearsal.sh --output-dir <artifact-dir>`, pre/post version evidence, blocked-reason snapshots for fail-closed variants | representative synthetic rehearsal defended; live per-release smoke still required |
| Published RC direct-binary update | installed RC release stays on the intended prerelease lane when the operator asks for RC behavior | `INV-UPDATE-002`, `INV-UPDATE-004` | `unit` + `integration` | channel-history fixtures, RC-target dry-run envelope, RC-upgrade rehearsal or explicit narrowed claim | explicit RC dry-run truth is defended; representative RC apply remains narrowed |
| Pinned install/update flows | explicit pinned versions behave deterministically and do not inherit accidental latest semantics | `INV-UPDATE-001`, `INV-UPDATE-002`, `INV-UPDATE-003` | `integration` | pinned synthetic release fixtures, installer dry-runs, updater dry-run envelopes for pinned targets | partially defended |
| Same-version no-op check | user already on the target release gets a truthful no-op result instead of a misleading remote-asset failure | `INV-UPDATE-001`, `INV-UPDATE-002` | `integration` | same-version updater rehearsal with healthy release assets and explicit no-op envelope evidence | defended by `update_release_contract_smoke` and the synthetic upgrade rehearsal |
| Linux `aarch64` published install/update | Linux `aarch64` support is truthful and surface-specific for install versus in-place update | `INV-UPDATE-002`, `INV-UPDATE-003` | `integration` + `smoke` | Linux `aarch64` installer proof, updater target-resolution proof, explicit `--target` posture or parity proof | install supported; updater parity not yet truthful |
| Local/unpublished or source-checkout users | updater fails closed and points those users at the correct release-backed recovery path | `INV-UPDATE-002`, `INV-UPDATE-005` | `integration` | blocked updater envelope with `local_or_unpublished_build`, release-backed reinstall guidance evidence | defended by current source-backed integration coverage |
| Symlinked, renamed, wrapper, or shim installs | unsupported layouts fail closed without partial mutation and with bounded recovery guidance | `INV-UPDATE-002`, `INV-UPDATE-005` | `unit` + `integration` | layout fixtures, blocked envelopes, reinstall guidance evidence | renamed/layout block remains defended; symlink/shim variants still stay bounded |
| Missing prerequisites, offline release drift, or malformed release assets | updater/install surfaces fail clearly and preserve a diagnosable repair path | `INV-UPDATE-001`, `INV-UPDATE-003`, `INV-UPDATE-005` | `integration` + `smoke` | malformed release-bundle fixtures, missing-tool fixtures, blocked envelopes, release verify artifacts | partially defended |
| Malformed or incompatible local store at update time | update preflight does not mutate across unknown migration boundaries and emits bounded recovery guidance | `INV-STORE-001`, `INV-UPDATE-002`, `INV-UPDATE-005` | `integration` | migration/store-schema probe fixtures, blocked envelopes, recovery-path evidence | partially defended |

If any row above lacks current evidence, the corresponding user-facing claim
must remain narrowed.

## Install Layout Contract

Roger must make the updater's install-layout boundary explicit.

### Blessed in-place-update layout

Supported updater layout is:

- a direct regular file
- named exactly `rr` on Unix-like systems or `rr.exe` on Windows
- located in a writable install directory
- executed directly rather than through a symlink hop at the binary path

### Unsupported updater layouts

These must fail closed for `rr update` unless Roger explicitly adds support:

- symlinked executable paths
- renamed release binaries
- wrapper scripts that exec a different binary path
- package-manager-managed shims that are not the release binary itself

When the layout is unsupported:

- `rr update` must block without mutation
- Roger must recommend reinstall rather than pretending in-place update support

## Apply And Rollback Semantics

### Installer lane

Current truthful installer guarantee:

- resolve target release
- fetch metadata, manifest, checksums, and archive
- verify checksum agreement
- extract archive
- copy binary into install directory

Current non-guarantee:

- installer replacement is not yet a rollback-capable atomic update contract

Roger must not describe installer reruns as having the same rollback semantics
as `rr update`.

### `rr update` lane

For published-release installs only, `rr update` must:

- fail closed for local/unpublished binaries
- perform non-mutating metadata and migration preflight in `--dry-run`
- require explicit confirmation before apply
- treat `--yes` / `-y` as confirmation bypass only
- block apply when migration preflight reports `apply_allowed=false`
- stage the candidate binary before replacement
- rename the current binary to a backup path
- attempt immediate rollback restore if replacement fails after backup begins

### Runtime prerequisites

The update/install contract depends on a small local tool surface:

- Unix installer: `bash`, `curl`, `python3`, `tar`
- PowerShell installer: `tar` plus standard Windows PowerShell web/file cmdlets
- updater: `curl` and `tar` on the host environment used by the installed `rr`

Roger must document these prerequisites and fail clearly when they are absent.

## Repair And Reinstall Guidance Contract

Repair guidance emitted by `rr update` must be usable from an installed-binary
context, not only from a repo checkout.

Required rule:

- recommended reinstall guidance must use release-backed commands or URLs
  that work without local repo scripts

Non-truthful guidance examples:

- `bash scripts/release/rr-install.sh ...`
- `powershell -File scripts/release/rr-install.ps1 ...`

unless the command is explicitly labeled as a source-checkout developer path.

## Tested Upgrade Path Contract

Roger needs one explicit proof story for updates. Piecewise validation is not
enough by itself.

## Immediate Build-Now Issues

If Roger were to make the update lane truly supportable right now, these are the
first issues that would have to be fixed in order:

1. make the release metadata, updater, installers, and published release assets
   agree on one checksum artifact contract
2. decide whether `rr update` is truthfully stable-by-default or
   current-channel-sticky, then align parser behavior, help text, docs, and
   tests
3. align the updater target-detection matrix with the installer/release target
   matrix, especially Linux `aarch64`
4. bring PowerShell installer semantics and proof closer to the Unix installer
   lane, or narrow Windows claims further
5. replace repo-relative reinstall guidance in updater responses with
   release-backed commands a normal installed user can actually run
6. make runtime prerequisites explicit and diagnostic instead of letting missing
   `curl` / `tar` look like generic release/network drift
7. keep the published-to-published synthetic upgrade rehearsal retained and
   wired into release evidence so the upgrade path stays defended as a journey
   rather than only as disjoint pieces
8. wire publish smoke so a freshly installed published binary must pass
   `rr update --dry-run --robot` against the just-published release
9. stop repo/public docs from assuming current-source update flags and robot-doc
   entries are already present in the latest published release artifact
10. either add the packaged-binary smoke Roger claims for `build-core`, or narrow
    the release docs so the gate is described truthfully

### Required automated proof layers

The update lane must retain these automated checks:

1. release asset verification
   - canonical entrypoint: `scripts/release/verify_release_assets.py`
   - proves asset presence, checksum manifest generation, and optional-lane
     claim consistency

2. publish-plan verification
   - canonical entrypoint: `scripts/release/publish_release.py`
   - proves release notes, publish-gate rules, and installer URL generation

3. Unix installer synthetic-feed tests
   - canonical entrypoint: `bash scripts/release/test_rr_install.sh`
   - must keep covering latest-resolution, explicit pins, ambiguous-target
     failure, and target-detection truth

4. updater-unit / updater-integration tests
   - canonical surface: `packages/cli/src/lib.rs`
   - must keep covering local-build block, confirmation matrix, migration
     preflight states, successful replace, rollback restore, and unsupported
     install layout
5. deterministic published-to-published upgrade rehearsal
   - canonical entrypoint:
     `bash scripts/release/test_update_upgrade_rehearsal.sh --output-dir <artifact-dir>`
   - proves install `N`, same-version no-op on the installed old binary, apply
     to `N+1`, and same-version no-op on the updated binary with preserved
     rehearsal artifacts

### Representative automated upgrade proof now present

Roger now carries a dedicated synthetic published-to-published upgrade rehearsal:

1. build release `N` and release `N+1` binaries with distinct embedded release metadata
2. install `N` via the official installer into an isolated directory
3. execute the installed `rr` from that directory and prove same-version no-op
4. repoint the synthetic release feed to `N+1`
5. run `rr update --yes --robot`
6. assert the updated binary remains usable and now reports a same-version no-op against `N+1`

Canonical entrypoint:

- `bash scripts/release/test_update_upgrade_rehearsal.sh --output-dir <artifact-dir>`

This is the representative `INV-UPDATE-004` proof for the bounded stable
direct-binary update lane. It does not, by itself, widen Windows-host or RC
apply claims; those remain separate cohorts with narrower current posture.

### Required manual stable-release proof

Before stable publication claims are widened, release closeout must retain:

1. `releases/latest` proof for the intended stable tag
2. live Unix installer dry-run proof against that latest release
3. PowerShell installer dry-run proof on a Windows host, or an explicit narrowed
   note that Windows installer proof remains manual and unretained
4. one representative prior-stable-to-current-stable upgrade rehearsal in an
   isolated directory on at least one primary target, using the official
   installer plus installed `rr update`

If step 4 is not done, Roger may still publish, but the release notes and
support posture must not imply a comprehensively tested upgrade path.

## Current Follow-On Gaps To Close

These are the concrete follow-ons implied by this contract.

1. Align `rr update` channel-default semantics with either:
   - truthful stable-by-default docs, or
   - real current-channel stickiness plus tests

2. Align the in-place updater target matrix with the installer matrix, or narrow
   docs so Linux `arm64` is install-only until updater parity lands.

3. Bring PowerShell installer semantics to parity with the Unix installer, or
   narrow the Windows installer contract so alias/fallback differences are
   explicit.

4. Replace repo-relative reinstall guidance emitted by `rr update` with
   release-backed commands.

5. Keep the synthetic published-to-published upgrade rehearsal retained in the
   release validation story and its output manifest wired into release evidence.

6. Decide whether checksum fallback is a real cross-surface contract or a Unix
   installer implementation detail, then align installers/updater/docs.

7. Document the updater runtime prerequisites in user-facing install/update
   docs rather than leaving them implicit in code.

Until those follow-ons land, the narrower truth in this contract wins.
