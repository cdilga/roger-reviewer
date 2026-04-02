#!/usr/bin/env bash
set -euo pipefail

workdir="$(mktemp -d)"
trap 'rm -rf "${workdir}"' EXIT

validator="scripts/release/validate_upstream_run.sh"

write_payload() {
  local path="$1"
  local workflow_path="$2"
  local status="$3"
  local conclusion="$4"
  local event="$5"
  local url="$6"

  cat >"${path}" <<EOF
{
  "path": "${workflow_path}",
  "name": "workflow-name-fixture",
  "status": "${status}",
  "conclusion": "${conclusion}",
  "event": "${event}",
  "html_url": "${url}"
}
EOF
}

pass_payload="${workdir}/pass.json"
write_payload \
  "${pass_payload}" \
  ".github/workflows/release-verify-assets.yml" \
  "completed" \
  "success" \
  "workflow_dispatch" \
  "https://github.com/example/repo/actions/runs/123"

run_url="$(bash "${validator}" \
  --payload "${pass_payload}" \
  --expected-workflow-path ".github/workflows/release-verify-assets.yml" \
  --label verify)"
[[ "${run_url}" == "https://github.com/example/repo/actions/runs/123" ]]

fail_path="${workdir}/bad-path.json"
write_payload \
  "${fail_path}" \
  ".github/workflows/validation-pr.yml" \
  "completed" \
  "success" \
  "workflow_dispatch" \
  "https://github.com/example/repo/actions/runs/124"
if bash "${validator}" \
  --payload "${fail_path}" \
  --expected-workflow-path ".github/workflows/release-verify-assets.yml" \
  --label verify; then
  echo "expected workflow-path mismatch failure" >&2
  exit 1
fi

fail_status="${workdir}/bad-status.json"
write_payload \
  "${fail_status}" \
  ".github/workflows/release-verify-assets.yml" \
  "in_progress" \
  "success" \
  "workflow_dispatch" \
  "https://github.com/example/repo/actions/runs/125"
if bash "${validator}" \
  --payload "${fail_status}" \
  --expected-workflow-path ".github/workflows/release-verify-assets.yml" \
  --label verify; then
  echo "expected status gate failure" >&2
  exit 1
fi

fail_event="${workdir}/bad-event.json"
write_payload \
  "${fail_event}" \
  ".github/workflows/release-verify-assets.yml" \
  "completed" \
  "success" \
  "pull_request" \
  "https://github.com/example/repo/actions/runs/126"
if bash "${validator}" \
  --payload "${fail_event}" \
  --expected-workflow-path ".github/workflows/release-verify-assets.yml" \
  --label verify; then
  echo "expected event policy failure" >&2
  exit 1
fi

fail_url="${workdir}/bad-url.json"
write_payload \
  "${fail_url}" \
  ".github/workflows/release-verify-assets.yml" \
  "completed" \
  "success" \
  "push" \
  ""
if bash "${validator}" \
  --payload "${fail_url}" \
  --expected-workflow-path ".github/workflows/release-verify-assets.yml" \
  --label verify; then
  echo "expected missing-url failure" >&2
  exit 1
fi

echo "test_validate_upstream_run: PASS"
