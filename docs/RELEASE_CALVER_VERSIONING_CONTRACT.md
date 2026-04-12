# Release CalVer Versioning Contract (`0.1.x`)

This document defines Roger's canonical release-version authority for `0.1.x`.
It supersedes any older duplicate CalVer contract drafts; do not keep parallel
release-version authority docs.

## Scope and Authority

- canonical derivation path: `scripts/release/derive_calver_version.py`
- canonical deterministic validation command: `./scripts/release/test_derive_calver_version.sh`
- canonical CI dry-run path: PR fixture checks inside `.github/workflows/release.yml`

Any workflow that names release tags, release titles, archive names, checksums, or
manifest files must consume the JSON envelope emitted by the derivation script.

Extension packaging follows the same release identity, but must translate that
identity into browser-safe manifest fields:

- tagged release builds stamp a numeric `manifest.version` derived from CalVer
- tagged release builds stamp a matching human-readable `version_name`
- local untagged builds keep a compatibility-safe numeric manifest version and
  use `version_name` for `-dev` / local provenance postfixes

## Canonical CalVer Tag Format

Roger uses date-based CalVer for release identity:

- stable tag: `vYYYY.MM.DD`
- release-candidate tag: `vYYYY.MM.DD-rc.N` where `N >= 1`
- nightly channel: derived from non-tag refs as `YYYY.MM.DD-nightly.<shortsha>`

`YYYY.MM.DD` must be zero-padded and parse as a valid UTC date.

## Channel Semantics

- `stable`
  - source ref must be a stable tag (`refs/tags/vYYYY.MM.DD`)
  - `prerelease=false`, `promotable=true`
- `rc`
  - source ref must be an RC tag (`refs/tags/vYYYY.MM.DD-rc.N`)
  - `prerelease=true`, `promotable=true`
- `nightly`
  - source ref is any non-tag ref (for example `refs/heads/main`)
  - version is derived from `--today` (or UTC now) plus commit short SHA
  - `prerelease=true`, `promotable=false`

## Provenance Rules

The derivation output includes a `provenance` object with:

- `source_ref`
- `source_sha`
- `source_short_sha`
- `date_basis`
- `version_source` (`tag` or `derived-ref`)

Fail-closed rules:

- malformed tag refs are rejected
- missing ref or SHA is rejected
- invalid date basis is rejected

## Cargo Workspace Version Interaction

Cargo workspace semver (`[workspace.package].version` in `Cargo.toml`) remains
`0.1.0` in this slice and is not mutated by CalVer derivation. The script emits
that semver as `workspace_version` for compatibility/audit context while using
CalVer as the release identity for tags and artifact naming.

## Release Metadata and Artifact Naming

The derivation script emits these metadata fields from the same source tuple
`(ref, sha, date_basis, workspace_version)`:

- `channel`, `version`, `tag`, `release_name`, `prerelease`, `promotable`
- `artifact_stem`
- `artifacts.cli_archive`
- `artifacts.bridge_archive`
- `artifacts.extension_archive`
- `artifacts.checksums`
- `artifacts.manifest`

This prevents drift where tags, release names, and archive names are generated
from different logic.

## Why `rr-001.2` / `rr-001.3` Closure Was Not Enough

`rr-001.2` and `rr-001.3` resolved ownership and installer/update semantics, but
left version authority undefined. Without this contract, there was no single
machine-truthful source for:

- release tag validity and channel mapping
- pre-release vs stable labeling
- deterministic archive/checksum/manifest naming
- release metadata derivation in CI without hand-edited version strings

This contract closes that gap by defining one canonical derivation path.
