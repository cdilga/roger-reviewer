#!/usr/bin/env bash
set -euo pipefail

# DEPRECATED ORPHAN — DO NOT USE.
#
# This script used to dispatch a fan of split release workflows
# (release-publish.yml, release-build-core.yml, release-verify-assets.yml,
# release-package-bridge.yml, release-package-extension.yml). Those workflows
# no longer exist: the release pipeline is now the single unified `release.yml`
# workflow, which runs build/package/verify/rehearsal jobs and the gated
# publish step in one run.
#
# The single sanctioned publish entrypoint is documented in
# docs/release-publish-operator-smoke.md (`gh workflow run release.yml`).
# Use that operator smoke checklist; it is the only supported path.

cat >&2 <<'EOF'
release_publish_trigger.sh is deprecated and dispatches nothing.

The split release-publish.yml / release-build-core.yml / release-verify-assets.yml
workflows it targeted were removed. Use the unified release workflow instead:

  gh workflow run release.yml -f ref=refs/tags/<tag> -f publish_mode=draft

Follow the sanctioned operator checklist before publishing:

  docs/release-publish-operator-smoke.md
EOF
exit 2
