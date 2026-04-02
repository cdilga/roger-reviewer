# Release Publish Operator Smoke (rr-xr6.5)

Purpose: provide the minimum truthful manual smoke checklist that must be completed before running `release-publish` with `publish_mode=publish`.

## Preconditions

1. `release-build-core` run completed successfully for the intended tag.
2. `release-verify-assets` run completed successfully and reports `publish_gate.publish_allowed=true`.
3. If optional lanes are claimed as shipped, corresponding `release-package-bridge` and/or `release-package-extension` runs completed successfully.
4. You have the run IDs for all lanes being referenced.

## Required Checks

1. Verify run identity in GitHub Actions UI:
- run IDs map to expected workflows (`release-build-core`, `release-verify-assets`, optional package lanes)
- run conclusion is `success`
- run event is `push` or `workflow_dispatch`
- if upstream verify data claims bridge/extension lanes as built, provide
  matching `bridge_run_id` / `extension_run_id` (and keep those runs successful)

2. Verify release artifacts in the `release-verify-assets-report` artifact:
- `release-asset-manifest.json` schema is `roger.release-verify-assets.v1`
- `publish_gate.publish_allowed` is `true`
- release tag/version/channel are the intended values
- `SHA256SUMS` and signing notes are present
- optional lane claims in upstream verify data match the lanes you provide to
  `release-publish` (no silent optional-lane downgrade)

3. Dry-run publication path as draft first (recommended):
```bash
# Example: draft rehearsal
# (replace IDs with real upstream run IDs)
gh workflow run release-publish.yml \
  -f core_run_id=<core_run_id> \
  -f verify_run_id=<verify_run_id> \
  -f publish_mode=draft
```

4. Inspect draft output:
- generated release notes include support posture + narrowed claims + checksum references
- attached assets match verified manifest + checksums
- run provenance is recorded in plan/notes

5. Stable publish confirmation:
- only after steps 1-4 pass, run with `publish_mode=publish`
- set `operator_smoke_ack=true`

6. Post-publish live installer proof (required for stable readiness):
- `curl -fsSL https://api.github.com/repos/<owner>/<repo>/releases/latest` must
  resolve (HTTP 200) and return the expected stable tag.
- `bash scripts/release/rr-install.sh --repo <owner>/<repo> --dry-run` must
  exit 0 against the live release feed.
- record absolute UTC timestamp and resolved stable tag in closeout notes.

## Evidence to retain

- URLs for upstream run IDs used by release-publish
- `release-publish-plan` artifact from the publish run
- final release URL
- live installer proof outputs (`releases/latest` response summary + dry-run output)
