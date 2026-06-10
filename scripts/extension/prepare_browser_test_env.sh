#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LAUNCH_SCRIPT="${ROOT_DIR}/scripts/extension/launch_preloaded_browser.sh"

usage() {
  cat <<'EOF'
Usage:
  scripts/extension/prepare_browser_test_env.sh \
    --browser <edge|chrome|brave> \
    [--profile-root <path>] \
    [--package-dir <path>] \
    [--start-url <url>] \
    [--browser-binary <path-or-command>] \
    [--artifact-root <path>] \
    [--reset-profile] \
    [--skip-setup] \
    [--skip-doctor] \
    [--skip-launch] \
    [--require-doctor-pass] \
    [--dry-run]

Notes:
  - This is the higher-level swarm/browser-owner entrypoint. It runs Roger's
    extension setup/doctor workflow against the same dedicated profile root, then
    relaunches the browser through the preloaded-extension helper with clean
    profile-process shutdown.
  - Use `--reset-profile` for the first bootstrap or when Edge profile state has
    gone stale.
  - `rr extension doctor` is informative by default; use `--require-doctor-pass`
    when you want the helper to fail closed before browser launch.
EOF
}

browser=""
profile_root=""
package_dir=""
start_url=""
browser_binary_override=""
artifact_root=""
reset_profile=0
skip_setup=0
skip_doctor=0
skip_launch=0
require_doctor_pass=0
dry_run=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --browser)
      browser="${2:-}"
      shift 2
      ;;
    --profile-root)
      profile_root="${2:-}"
      shift 2
      ;;
    --package-dir)
      package_dir="${2:-}"
      shift 2
      ;;
    --start-url)
      start_url="${2:-}"
      shift 2
      ;;
    --browser-binary)
      browser_binary_override="${2:-}"
      shift 2
      ;;
    --artifact-root)
      artifact_root="${2:-}"
      shift 2
      ;;
    --reset-profile)
      reset_profile=1
      shift
      ;;
    --skip-setup)
      skip_setup=1
      shift
      ;;
    --skip-doctor)
      skip_doctor=1
      shift
      ;;
    --skip-launch)
      skip_launch=1
      shift
      ;;
    --require-doctor-pass)
      require_doctor_pass=1
      shift
      ;;
    --dry-run)
      dry_run=1
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

if [[ -z "$browser" ]]; then
  echo "error: --browser is required" >&2
  usage >&2
  exit 2
fi

case "$browser" in
  edge|chrome|brave)
    ;;
  *)
    echo "error: unsupported browser '$browser'" >&2
    exit 2
    ;;
esac

if ! command -v rr >/dev/null 2>&1; then
  echo "error: rr is required on PATH for browser environment preparation" >&2
  exit 1
fi

profile_root="${profile_root:-${RR_EXTENSION_PROFILE_ROOT:-${HOME}/.roger/bridge/browser-profiles/${browser}}}"
artifact_root="${artifact_root:-${profile_root}/roger-bootstrap-artifacts/$(date -u +%Y%m%dT%H%M%SZ)}"

if [[ "$dry_run" -eq 1 ]]; then
  cat <<EOF
browser=${browser}
profile_root=${profile_root}
artifact_root=${artifact_root}
reset_profile=${reset_profile}
skip_setup=${skip_setup}
skip_doctor=${skip_doctor}
skip_launch=${skip_launch}
require_doctor_pass=${require_doctor_pass}
launch_script=${LAUNCH_SCRIPT}
EOF
  exit 0
fi

mkdir -p "$artifact_root"
export RR_EXTENSION_PROFILE_ROOT="$profile_root"

setup_json="${artifact_root}/rr_extension_setup.json"
doctor_json="${artifact_root}/rr_extension_doctor.json"
doctor_exit=0

if [[ "$skip_setup" -eq 0 ]]; then
  rr extension setup --browser "$browser" --robot >"$setup_json"
fi

if [[ "$skip_doctor" -eq 0 ]]; then
  if ! rr extension doctor --browser "$browser" --robot >"$doctor_json"; then
    doctor_exit=$?
    if [[ "$require_doctor_pass" -eq 1 ]]; then
      echo "error: rr extension doctor failed for ${browser}; see ${doctor_json}" >&2
      exit "$doctor_exit"
    fi
  fi
fi

if [[ "$skip_launch" -eq 0 ]]; then
  launch_cmd=(
    "$LAUNCH_SCRIPT"
    --browser "$browser"
    --profile-root "$profile_root"
    --close-existing
    --dismiss-edge-prompts
  )
  if [[ "$reset_profile" -eq 1 ]]; then
    launch_cmd+=(--reset-profile)
  fi
  if [[ -n "$package_dir" ]]; then
    launch_cmd+=(--package-dir "$package_dir")
  fi
  if [[ -n "$start_url" ]]; then
    launch_cmd+=(--start-url "$start_url")
  fi
  if [[ -n "$browser_binary_override" ]]; then
    launch_cmd+=(--browser-binary "$browser_binary_override")
  fi
  "${launch_cmd[@]}"
fi

cat <<EOF
Prepared browser test environment.

browser=${browser}
profile_root=${profile_root}
artifact_root=${artifact_root}
setup_json=${setup_json}
doctor_json=${doctor_json}
doctor_exit=${doctor_exit}
launch_script=${LAUNCH_SCRIPT}

If Edge still reports a stale or forbidden bridge state, fully quit the
dedicated-profile browser process and rerun this helper with --reset-profile.
EOF
