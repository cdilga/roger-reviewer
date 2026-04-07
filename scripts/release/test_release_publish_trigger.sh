#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TRIGGER_SCRIPT="${ROOT_DIR}/scripts/release/release_publish_trigger.sh"

workdir="$(mktemp -d)"
trap 'rm -rf "${workdir}"' EXIT

fixture_dir="${workdir}/fixtures"
bin_dir="${workdir}/bin"
workflow_log="${workdir}/workflow.log"
mkdir -p "${fixture_dir}" "${bin_dir}"

cat >"${bin_dir}/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

fixtures="${GH_FIXTURE_DIR:?}"
workflow_log="${GH_WORKFLOW_LOG:?}"

if [[ "${1:-}" == "api" ]]; then
  endpoint="${2:-}"
  case "${endpoint}" in
    repos/*/actions/runs/*)
      run_id="${endpoint##*/}"
      cat "${fixtures}/run-${run_id}.json"
      exit 0
      ;;
    repos/*/actions/workflows/*/runs*)
      workflow_and_query="${endpoint#*actions/workflows/}"
      workflow_file="${workflow_and_query%%/runs*}"
      safe_name="${workflow_file//\//__}"
      payload_path="${fixtures}/list-${safe_name}.json"
      if [[ -f "${payload_path}" ]]; then
        cat "${payload_path}"
      else
        printf '{"workflow_runs":[]}\n'
      fi
      exit 0
      ;;
  esac
fi

if [[ "${1:-}" == "workflow" && "${2:-}" == "run" ]]; then
  printf '%s\n' "$*" >>"${workflow_log}"
  exit 0
fi

echo "unsupported gh invocation: $*" >&2
exit 2
EOF
chmod +x "${bin_dir}/gh"

write_run_payload() {
  local run_id="$1"
  local workflow_path="$2"
  local event="$3"
  cat >"${fixture_dir}/run-${run_id}.json" <<EOF
{
  "path": "${workflow_path}",
  "name": "fixture-${run_id}",
  "status": "completed",
  "conclusion": "success",
  "event": "${event}",
  "html_url": "https://github.com/example/repo/actions/runs/${run_id}"
}
EOF
}

write_run_payload "101" ".github/workflows/release-build-core.yml" "workflow_dispatch"
write_run_payload "202" ".github/workflows/release-verify-assets.yml" "push"
write_run_payload "501" ".github/workflows/release-build-core.yml" "push"
write_run_payload "601" ".github/workflows/release-verify-assets.yml" "workflow_dispatch"

cat >"${fixture_dir}/list-.github__workflows__release-build-core.yml.json" <<'EOF'
{
  "workflow_runs": [
    {
      "id": 501,
      "head_branch": "2026.04.07",
      "status": "completed",
      "conclusion": "success",
      "event": "push"
    }
  ]
}
EOF

cat >"${fixture_dir}/list-.github__workflows__release-verify-assets.yml.json" <<'EOF'
{
  "workflow_runs": [
    {
      "id": 601,
      "head_branch": "2026.04.07",
      "status": "completed",
      "conclusion": "success",
      "event": "workflow_dispatch"
    }
  ]
}
EOF

export PATH="${bin_dir}:${PATH}"
export GH_FIXTURE_DIR="${fixture_dir}"
export GH_WORKFLOW_LOG="${workflow_log}"

explicit_output="$(bash "${TRIGGER_SCRIPT}" \
  --repo example/repo \
  --core-run-id 101 \
  --verify-run-id 202 \
  --publish-mode draft \
  --dry-run)"

grep -q "core_run_id=101" <<<"${explicit_output}"
grep -q "verify_run_id=202" <<<"${explicit_output}"
grep -q "publish_mode=draft" <<<"${explicit_output}"
grep -q "gh workflow run release-publish.yml" <<<"${explicit_output}"
if grep -q "operator_smoke_ack=true" <<<"${explicit_output}"; then
  echo "draft run should not set operator_smoke_ack=true" >&2
  exit 1
fi

if [[ -f "${workflow_log}" ]] && [[ -s "${workflow_log}" ]]; then
  echo "dry-run should not dispatch workflow" >&2
  exit 1
fi

discovery_output="$(bash "${TRIGGER_SCRIPT}" \
  --repo example/repo \
  --tag 2026.04.07 \
  --publish-mode draft \
  --dry-run)"

grep -q "core_run_id=501" <<<"${discovery_output}"
grep -q "verify_run_id=601" <<<"${discovery_output}"
grep -q "tag=2026.04.07" <<<"${discovery_output}"

if bash "${TRIGGER_SCRIPT}" \
  --repo example/repo \
  --core-run-id 101 \
  --verify-run-id 202 \
  --publish-mode publish \
  --dry-run; then
  echo "publish mode must require --operator-smoke-ack" >&2
  exit 1
fi

bash "${TRIGGER_SCRIPT}" \
  --repo example/repo \
  --core-run-id 101 \
  --verify-run-id 202 \
  --publish-mode publish \
  --operator-smoke-ack >/dev/null

grep -q "publish_mode=publish" "${workflow_log}"
grep -q "operator_smoke_ack=true" "${workflow_log}"

echo "test_release_publish_trigger: PASS"
