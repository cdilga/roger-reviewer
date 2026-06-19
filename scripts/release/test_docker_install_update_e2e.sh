#!/usr/bin/env bash
# End-to-end smoke against REAL published GitHub releases, inside a clean Docker
# container: install a pinned older version via the one-liner, then `rr update`
# to a newer published version and assert the binary actually upgraded.
#
# This is a network + real-release test (not a hermetic unit test): it pulls the
# live installer and release assets from github.com. Run it manually after
# publishing a new release to prove install + self-update work for real.
#
# Usage:
#   scripts/release/test_docker_install_update_e2e.sh \
#     --from 2026.06.17 --to 2026.06.20 [--platform linux/arm64|linux/amd64] [--image ubuntu:24.04]
set -euo pipefail

from_version=""
to_version=""
platform=""        # empty => Docker default (host arch)
image="ubuntu:24.04"
repo="cdilga/roger-reviewer"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --from) from_version="${2:-}"; shift 2 ;;
    --to) to_version="${2:-}"; shift 2 ;;
    --platform) platform="${2:-}"; shift 2 ;;
    --image) image="${2:-}"; shift 2 ;;
    --repo) repo="${2:-}"; shift 2 ;;
    -h|--help)
      grep '^#' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *) echo "error: unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [[ -z "$from_version" || -z "$to_version" ]]; then
  echo "error: --from and --to are required" >&2
  exit 2
fi

command -v docker >/dev/null 2>&1 || { echo "error: docker not found" >&2; exit 2; }
docker info >/dev/null 2>&1 || { echo "error: docker daemon not running" >&2; exit 2; }

platform_args=()
[[ -n "$platform" ]] && platform_args=(--platform "$platform")

echo "==> Docker install+update E2E"
echo "    image:     ${image}"
echo "    platform:  ${platform:-<host default>}"
echo "    from:      ${from_version}  ->  to (latest): ${to_version}"
echo

# In-container script. Quoted heredoc => the outer shell does NOT touch it; the
# FROM/TO/REPO values arrive via `docker run -e`. `rr` reports its embedded
# version through `rr update --dry-run --robot` (.data.current_version).
read -r -d '' container_script <<'CONTAINER' || true
set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq >/dev/null
apt-get install -y -qq curl ca-certificates python3 >/dev/null
export PATH="$HOME/.local/bin:$PATH"

echo "--- install pinned ${FROM_VERSION} via the published one-liner ---"
curl -fsSL "https://github.com/${REPO}/releases/download/v${FROM_VERSION}/rr-install.sh" \
  | bash -s -- --version "${FROM_VERSION}"

current() {
  # the published binary reports its embedded version under
  # data.current_release.version
  rr update --dry-run --robot 2>/dev/null \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["data"]["current_release"]["version"])'
}

before="$(current)"
echo "--- installed version: ${before} ---"
[ "${before}" = "${FROM_VERSION}" ] || { echo "FAIL: expected ${FROM_VERSION}, got ${before}"; exit 1; }

echo "--- rr --help works on installed binary ---"
rr --help >/dev/null || { echo "FAIL: rr --help failed"; exit 1; }

echo "--- rr update --yes --robot (apply self-update to latest) ---"
rr update --yes --robot 2>&1 | python3 -c 'import json,sys
d=json.load(sys.stdin); print("update outcome:", d.get("outcome"), "exit_code:", d.get("exit_code"))' \
  || { echo "FAIL: update did not return a robot envelope"; exit 1; }

after="$(current)"
echo "--- version after update: ${after} ---"
[ "${after}" = "${TO_VERSION}" ] || { echo "FAIL: expected upgrade to ${TO_VERSION}, got ${after}"; exit 1; }

echo "--- updated binary still works (rr --help + doctor) ---"
rr --help >/dev/null || { echo "FAIL: rr --help failed after update"; exit 1; }
rr doctor --robot >/dev/null 2>&1 || true   # doctor may warn; just ensure it runs

echo "PASS: install ${FROM_VERSION} -> update -> ${after}"
CONTAINER

set +e
docker run --rm "${platform_args[@]}" \
  -e FROM_VERSION="$from_version" \
  -e TO_VERSION="$to_version" \
  -e REPO="$repo" \
  "$image" bash -c "$container_script"
status=$?
set -e

echo
if [[ $status -eq 0 ]]; then
  echo "test_docker_install_update_e2e: PASS (${platform:-host})"
else
  echo "test_docker_install_update_e2e: FAIL (${platform:-host}) exit=${status}"
fi
exit $status
