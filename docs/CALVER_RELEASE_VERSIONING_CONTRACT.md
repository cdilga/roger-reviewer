# CalVer Release Versioning Contract (`rr-xr6.1`)

## Purpose

This contract defines a single authoritative version-derivation path for Roger
release tags, channels, and artifact metadata. It removes manual version edits
from release workflows and gives one machine-truthful source for:

- release tag identity
- channel semantics (`stable`, `rc`, `nightly`)
- artifact naming prefix
- release metadata fields used by later packaging and self-update lanes

## Canonical CalVer Forms

All release-facing versions derive from UTC date + channel rules:

- Stable tag: `vYYYY.MM.DD`
- RC tag: `vYYYY.MM.DD-rc.N` where `N >= 1`
- Nightly synthetic tag: `vYYYY.MM.DD-nightly.RUN_NUMBER`

`YYYY.MM.DD` is always UTC and must match the supplied `date_utc` input in the
derivation command.

## Channel Semantics

- `stable`
  - source ref: `refs/tags/vYYYY.MM.DD`
  - prerelease flag: `false`
- `rc`
  - source ref: `refs/tags/vYYYY.MM.DD-rc.N`
  - prerelease flag: `true`
- `nightly`
  - source ref: `refs/heads/main` (or `master` for compatibility)
  - version suffix: `-nightly.RUN_NUMBER`
  - prerelease flag: `true`

Any other ref shape fails closed.

## Provenance and Rerun Rules

Every derivation emits immutable provenance:

`sha.<short_sha>.run.<run_number>.attempt.<run_attempt>`

Rules:

- reruns (`run_attempt` changes) do **not** alter the canonical channel/tag
  decision logic
- provenance changes on rerun and is intended for metadata/audit lineage
- release tags remain the authority for stable/rc identity; branch refs cannot
  impersonate them

## Single-Source Derived Fields

The command `roger-validation derive-calver` is the canonical derivation source.
From one input it emits:

- `channel`
- `canonical_version`
- `artifact_version`
- `tag`
- `release_name`
- `artifact_prefix`
- `release_prerelease`
- `provenance`

Artifact names and release metadata must consume these derived fields instead of
recomputing channel/tag logic independently.

## Cargo Workspace Version Interaction

Roger keeps Cargo workspace package versioning (`version.workspace`) separate
from release CalVer identity in `0.1.x`:

- Cargo workspace version remains SemVer (`0.1.0` line)
- release tags/artifact versions use this CalVer contract
- self-update and release metadata lookups use CalVer outputs, not Cargo crate
  version strings

This avoids hand-editing Cargo versions for every release tag and keeps the
release identity path deterministic.

## Deterministic Validation Contract

Primary test command:

```bash
cargo test -p roger-validation calver_derivation
```

CLI derivation smoke (synthetic inputs):

```bash
cargo run -p roger-validation -- derive-calver \
  --git-ref refs/tags/v2026.03.31 \
  --sha 0123456789abcdef0123456789abcdef01234567 \
  --run-number 77 \
  --run-attempt 1 \
  --date-utc 2026-03-31
```

CI dry-run coverage for synthetic refs is defined in:

- PR fixture checks inside `.github/workflows/release.yml`

## Why `rr-001.2` / `rr-001.3` Were Not Enough

`rr-001.2` and `rr-001.3` established release ownership boundaries and
install/update expectations, but they did not define:

- one concrete tag format
- channel derivation rules
- rerun provenance behavior
- single-source version outputs for artifacts and release metadata

So ownership existed, but version authority did not. This contract closes that
gap.
