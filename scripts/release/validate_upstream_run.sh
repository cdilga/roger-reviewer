#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/release/validate_upstream_run.sh \
    --payload <actions-run.json> \
    --expected-workflow-path <workflow-path> \
    --label <label>
EOF
}

payload_path=""
expected_workflow_path=""
label="run"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --payload)
      payload_path="${2:-}"
      shift 2
      ;;
    --expected-workflow-path)
      expected_workflow_path="${2:-}"
      shift 2
      ;;
    --label)
      label="${2:-}"
      shift 2
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

if [[ -z "${payload_path}" || -z "${expected_workflow_path}" ]]; then
  echo "error: --payload and --expected-workflow-path are required" >&2
  usage >&2
  exit 2
fi
if [[ ! -f "${payload_path}" ]]; then
  echo "error: payload file not found: ${payload_path}" >&2
  exit 2
fi

workflow_path="$(jq -r '.path // empty' "${payload_path}")"
workflow_name="$(jq -r '.name // empty' "${payload_path}")"
run_status="$(jq -r '.status // empty' "${payload_path}")"
run_conclusion="$(jq -r '.conclusion // empty' "${payload_path}")"
run_event="$(jq -r '.event // empty' "${payload_path}")"
run_url="$(jq -r '.html_url // empty' "${payload_path}")"

if [[ "${workflow_path}" != "${expected_workflow_path}" ]]; then
  echo "error: ${label} run is not ${expected_workflow_path}" >&2
  echo "observed path=${workflow_path:-<missing>} name=${workflow_name:-<missing>}" >&2
  exit 1
fi

if [[ "${run_status}" != "completed" || "${run_conclusion}" != "success" ]]; then
  echo "error: ${label} run must be completed and successful" >&2
  echo "observed status=${run_status:-<missing>} conclusion=${run_conclusion:-<missing>}" >&2
  exit 1
fi

case "${run_event}" in
  workflow_dispatch|push) ;;
  *)
    echo "error: ${label} run must come from push/workflow_dispatch (got ${run_event:-<missing>})" >&2
    exit 1
    ;;
esac

if [[ -z "${run_url}" || "${run_url}" == "null" ]]; then
  echo "error: ${label} run is missing html_url" >&2
  exit 1
fi

printf '%s\n' "${run_url}"
