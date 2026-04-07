#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
INGEST_SCRIPT="${ROOT_DIR}/scripts/swarm/ingest_failed_actions_runs.py"
DEFAULT_CONFIG="${ROOT_DIR}/.github/ci-failure-intake.json"
DEFAULT_STATE="${ROOT_DIR}/.beads/ci-failure-intake-state.json"
DEFAULT_BR_BIN="${ROOT_DIR}/scripts/swarm/br_pinned.sh"

POLL_SECONDS="${CI_FAILURE_POLL_SECONDS:-300}"
CONFIG_PATH="${CI_FAILURE_INTAKE_CONFIG:-${DEFAULT_CONFIG}}"
STATE_FILE="${CI_FAILURE_INTAKE_STATE:-${DEFAULT_STATE}}"
BR_BIN="${CI_FAILURE_BR_BIN:-${DEFAULT_BR_BIN}}"
REPO_SLUG="${CI_FAILURE_REPO:-}"
PER_PAGE=30
ONCE=0
DRY_RUN=0

usage() {
  cat <<'EOF'
Usage:
  watch_ci_failures.sh [options]

Options:
  --repo OWNER/REPO        Repository slug. Defaults to origin remote parsing.
  --config PATH            Intake config JSON. Default: .github/ci-failure-intake.json
  --state-file PATH        State file for dedupe across polls. Default: .beads/ci-failure-intake-state.json
  --poll-seconds N         Poll interval in seconds. Default: 300
  --per-page N             Number of failed runs to fetch each poll. Default: 30
  --once                   Run one ingestion pass and exit
  --dry-run                Do not mutate beads or state
  --br-binary PATH         br wrapper/binary path. Default: scripts/swarm/br_pinned.sh
  -h, --help               Show this help
EOF
}

derive_repo_slug() {
  local remote
  remote="$(git -C "${ROOT_DIR}" remote get-url origin 2>/dev/null || true)"
  if [[ -z "${remote}" ]]; then
    echo "ERROR: could not derive repository slug from origin remote" >&2
    exit 1
  fi

  if [[ "${remote}" =~ github\.com[:/]([^/]+)/([^/.]+)(\.git)?$ ]]; then
    printf '%s/%s\n' "${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}"
    return 0
  fi

  echo "ERROR: unsupported origin remote for GitHub slug derivation: ${remote}" >&2
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo)
      REPO_SLUG="${2:-}"
      shift 2
      ;;
    --config)
      CONFIG_PATH="${2:-}"
      shift 2
      ;;
    --state-file)
      STATE_FILE="${2:-}"
      shift 2
      ;;
    --poll-seconds)
      POLL_SECONDS="${2:-}"
      shift 2
      ;;
    --per-page)
      PER_PAGE="${2:-}"
      shift 2
      ;;
    --once)
      ONCE=1
      shift
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --br-binary)
      BR_BIN="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "${REPO_SLUG}" ]]; then
  REPO_SLUG="$(derive_repo_slug)"
fi

run_once() {
  local output
  if ! output="$(python3 "${INGEST_SCRIPT}" \
    --repo "${REPO_SLUG}" \
    --project-root "${ROOT_DIR}" \
    --config "${CONFIG_PATH}" \
    --state-file "${STATE_FILE}" \
    --per-page "${PER_PAGE}" \
    --br-binary "${BR_BIN}" \
    $([[ "${DRY_RUN}" == "1" ]] && printf '%s' '--dry-run'))"; then
    echo "watch_ci_failures: ingestion failed at $(date -Iseconds)" >&2
    return 1
  fi

  printf '%s\n' "${output}"
  return 0
}

if [[ "${ONCE}" == "1" ]]; then
  run_once
  exit $?
fi

while true; do
  run_once || true
  sleep "${POLL_SECONDS}"
done
