# Release Publish Operator Smoke (rr-xr6.5)

Purpose: provide the minimum truthful manual smoke checklist that must be completed before running the unified `release` workflow with `publish_mode=publish`.

## Preconditions

1. The unified `release` workflow completed its build, packaging, and verify jobs successfully for the intended tag.
2. The `release-verify-assets-report` artifact reports `publish_gate.publish_allowed=true`.
3. The same workflow run produced the optional bridge/extension artifacts you intend to ship.

## Required Checks

1. Verify run identity in GitHub Actions UI:
- the run is the unified `release` workflow for the intended tag
- run conclusion is `success`
- run event is `push` or `workflow_dispatch`
- the internal jobs `build-core`, `package-bridge`, `package-extension`, and
  `verify-release-assets` all succeeded for the run you are using

2. Verify release artifacts in the `release-verify-assets-report` artifact:
- `release-asset-manifest.json` schema is `roger.release-verify-assets.v1`
- `publish_gate.publish_allowed` is `true`
- release tag/version/channel are the intended values
- `SHA256SUMS` and signing notes are present
- optional lane claims in verify data match the artifacts the unified release
  run actually produced (no silent optional-lane downgrade)

3. Dry-run publication path as draft first (recommended):
```bash
gh workflow run release.yml \
  -f ref=refs/tags/<stable-or-rc-tag> \
  -f publish_mode=draft
```

4. Inspect draft output:
- generated release notes include support posture + narrowed claims + checksum references
- attached assets match verified manifest + checksums
- run provenance is recorded in plan/notes

5. Stable publish confirmation:
- only after steps 1-4 pass, run with `publish_mode=publish`
- publish mode must pass `--operator-smoke-ack`
- example:
```bash
gh workflow run release.yml \
  -f ref=refs/tags/<stable-or-rc-tag> \
  -f publish_mode=publish \
  -f operator_smoke_ack=true
```

6. Post-publish live install/update proof (required for stable readiness):
- `curl -fsSL https://api.github.com/repos/cdilga/roger-reviewer/releases/latest`
  must resolve (HTTP 200) and return the expected stable tag.
- `bash scripts/release/rr-install.sh --repo cdilga/roger-reviewer --dry-run`
  must exit 0 against the live release feed.
- a fresh isolated install from the live release-hosted Unix installer must
  succeed in a temp directory, and the installed binary must then pass
  `rr update --dry-run --robot` against that same live release without a
  release-asset inconsistency block such as `checksums_missing`
- record these exact CI-evidence fields in closeout:
  - `--latest-proof-utc <YYYY-MM-DDTHH:MM:SSZ>`
  - `--latest-proof-tag <stable-tag>`
  - `--installer-dry-run-outcome success`
  - `--fresh-install-update-dry-run-outcome <complete|blocked|...>`
  - `--fresh-install-update-dry-run-reason-code <reason-or-none>`

## Evidence to retain

- URL for the unified `release` workflow run used for publish
- `release-publish-plan` artifact from the publish run
- final release URL
- live installer/update proof outputs (`releases/latest` response summary + installer dry-run output + fresh-install update dry-run output)
- explicit CI evidence fields for the closeout guard:
  `latest_proof_utc`, `latest_proof_tag`, `installer_dry_run_outcome`,
  `fresh_install_update_dry_run_outcome`, `fresh_install_update_dry_run_reason_code`
