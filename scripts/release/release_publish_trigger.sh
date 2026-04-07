#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/release/release_publish_trigger.sh \
    --repo <owner/repo> \
    [--tag <release-tag>] \
    [--core-run-id <id>] \
    [--verify-run-id <id>] \
    [--bridge-run-id <id>] \
    [--extension-run-id <id>] \
    [--publish-mode draft|publish] \
    [--operator-smoke-ack] \
    [--dry-run]

Notes:
  - If --core-run-id/--verify-run-id are omitted, --tag is required and the
    script discovers the latest successful run IDs for that tag.
  - publish mode requires --operator-smoke-ack.
EOF
}

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VALIDATE_UPSTREAM_RUN="${ROOT_DIR}/scripts/release/validate_upstream_run.sh"

repo=""
tag=""
core_run_id=""
verify_run_id=""
bridge_run_id=""
extension_run_id=""
publish_mode="draft"
operator_smoke_ack=false
dry_run=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo)
      repo="${2:-}"
      shift 2
      ;;
    --tag)
      tag="${2:-}"
      shift 2
      ;;
    --core-run-id)
      core_run_id="${2:-}"
      shift 2
      ;;
    --verify-run-id)
      verify_run_id="${2:-}"
      shift 2
      ;;
    --bridge-run-id)
      bridge_run_id="${2:-}"
      shift 2
      ;;
    --extension-run-id)
      extension_run_id="${2:-}"
      shift 2
      ;;
    --publish-mode)
      publish_mode="${2:-}"
      shift 2
      ;;
    --operator-smoke-ack)
      operator_smoke_ack=true
      shift
      ;;
    --dry-run)
      dry_run=true
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "${repo}" ]]; then
  echo "error: --repo is required" >&2
  usage >&2
  exit 2
fi

if [[ "${publish_mode}" != "draft" && "${publish_mode}" != "publish" ]]; then
  echo "error: --publish-mode must be one of: draft, publish" >&2
  exit 2
fi

if [[ "${publish_mode}" == "publish" && "${operator_smoke_ack}" != "true" ]]; then
  echo "error: publish mode requires --operator-smoke-ack" >&2
  exit 2
fi

if [[ "${tag}" == refs/tags/* ]]; then
  tag="${tag#refs/tags/}"
fi

tmpdir="$(mktemp -d)"
trap 'rm -rf "${tmpdir}"' EXIT

discover_run_id_for_tag() {
  local workflow_file="$1"
  local label="$2"

  if [[ -z "${tag}" ]]; then
    echo "error: --tag is required when ${label}_run_id is omitted" >&2
    exit 2
  fi

  local list_payload="${tmpdir}/runs-${label}.json"
  gh api "repos/${repo}/actions/workflows/${workflow_file}/runs?status=completed&per_page=100" >"${list_payload}"

  local discovered
  discovered="$(jq -r --arg tag "${tag}" '
    .workflow_runs
    | map(select(
        (.head_branch // "") == $tag
        and (.status // "") == "completed"
        and (.conclusion // "") == "success"
        and (((.event // "") == "push") or ((.event // "") == "workflow_dispatch"))
      ))
    | .[0].id // empty
  ' "${list_payload}")"

  if [[ -z "${discovered}" ]]; then
    echo "error: failed to discover a successful ${label} run for tag ${tag}" >&2
    exit 1
  fi
  printf '%s\n' "${discovered}"
}

validate_run() {
  local run_id="$1"
  local expected_workflow_path="$2"
  local label="$3"

  local payload_path="${tmpdir}/run-${label}.json"
  gh api "repos/${repo}/actions/runs/${run_id}" >"${payload_path}"

  bash "${VALIDATE_UPSTREAM_RUN}" \
    --payload "${payload_path}" \
    --expected-workflow-path "${expected_workflow_path}" \
    --label "${label}"
}

if [[ -z "${core_run_id}" ]]; then
  core_run_id="$(discover_run_id_for_tag ".github/workflows/release-build-core.yml" "core")"
fi
if [[ -z "${verify_run_id}" ]]; then
  verify_run_id="$(discover_run_id_for_tag ".github/workflows/release-verify-assets.yml" "verify")"
fi

core_run_url="$(validate_run "${core_run_id}" ".github/workflows/release-build-core.yml" "core")"
verify_run_url="$(validate_run "${verify_run_id}" ".github/workflows/release-verify-assets.yml" "verify")"

bridge_run_url=""
if [[ -n "${bridge_run_id}" ]]; then
  bridge_run_url="$(validate_run "${bridge_run_id}" ".github/workflows/release-package-bridge.yml" "bridge")"
fi

extension_run_url=""
if [[ -n "${extension_run_id}" ]]; then
  extension_run_url="$(validate_run "${extension_run_id}" ".github/workflows/release-package-extension.yml" "extension")"
fi

cmd=(
  gh workflow run release-publish.yml
  --repo "${repo}"
  -f "core_run_id=${core_run_id}"
  -f "verify_run_id=${verify_run_id}"
  -f "publish_mode=${publish_mode}"
)

if [[ -n "${bridge_run_id}" ]]; then
  cmd+=(-f "bridge_run_id=${bridge_run_id}")
fi
if [[ -n "${extension_run_id}" ]]; then
  cmd+=(-f "extension_run_id=${extension_run_id}")
fi
if [[ "${publish_mode}" == "publish" ]]; then
  cmd+=(-f "operator_smoke_ack=true")
fi

echo "release-publish trigger summary:"
echo "  repo=${repo}"
echo "  publish_mode=${publish_mode}"
echo "  core_run_id=${core_run_id}"
echo "  verify_run_id=${verify_run_id}"
if [[ -n "${bridge_run_id}" ]]; then
  echo "  bridge_run_id=${bridge_run_id}"
fi
if [[ -n "${extension_run_id}" ]]; then
  echo "  extension_run_id=${extension_run_id}"
fi
if [[ -n "${tag}" ]]; then
  echo "  tag=${tag}"
fi
echo "  core_run_url=${core_run_url}"
echo "  verify_run_url=${verify_run_url}"
if [[ -n "${bridge_run_url}" ]]; then
  echo "  bridge_run_url=${bridge_run_url}"
fi
if [[ -n "${extension_run_url}" ]]; then
  echo "  extension_run_url=${extension_run_url}"
fi

printf '  command='
printf '%q ' "${cmd[@]}"
printf '\n'

if [[ "${dry_run}" == "true" ]]; then
  echo "dry-run: release-publish was not dispatched"
  exit 0
fi

"${cmd[@]}"
echo "release-publish dispatch submitted"
