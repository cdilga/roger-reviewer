#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/rr-ci-closeout-test.XXXXXX")"
trap 'rm -rf "$WORKDIR"' EXIT

FAKE_BIN="${WORKDIR}/bin"
mkdir -p "$FAKE_BIN"

cat >"${FAKE_BIN}/br" <<'BR'
#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" != "show" ]]; then
  echo "unsupported fake br command: ${1:-}" >&2
  exit 2
fi

bead="${2:-}"
case "$bead" in
  rr-sensitive)
    cat <<'JSON'
[
  {
    "id": "rr-sensitive",
    "labels": ["ci", "release"]
  }
]
JSON
    ;;
  rr-docs)
    cat <<'JSON'
[
  {
    "id": "rr-docs",
    "labels": ["docs"]
  }
]
JSON
    ;;
  *)
    echo '[]'
    ;;
esac
BR
chmod +x "${FAKE_BIN}/br"

SCRIPT_UNDER_TEST="${WORKDIR}/check_ci_closeout_evidence.sh"
cp "${SCRIPT_DIR}/check_ci_closeout_evidence.sh" "$SCRIPT_UNDER_TEST"
chmod +x "$SCRIPT_UNDER_TEST"

export PATH="${FAKE_BIN}:$PATH"

printf 'Scenario 1: CI-sensitive bead without remote evidence fails...\n'
set +e
out="$("$SCRIPT_UNDER_TEST" --bead rr-sensitive 2>&1)"
status=$?
set -e
[[ "$status" -ne 0 ]]
grep -q "CI-sensitive bead requires remote run evidence" <<<"$out"

printf 'Scenario 2: CI-sensitive bead with valid remote evidence passes...\n'
"$SCRIPT_UNDER_TEST" \
  --bead rr-sensitive \
  --run-url "https://github.com/cdilga/roger-reviewer/actions/runs/23821543593" \
  --outcome success >/dev/null

printf 'Scenario 3: Non-CI-sensitive bead with local-only reason passes...\n'
"$SCRIPT_UNDER_TEST" \
  --bead rr-docs \
  --local-only-reason "docs-only contract bead with no remote CI promise" >/dev/null

printf 'Scenario 4: Non-CI-sensitive bead with no evidence fails...\n'
set +e
out="$("$SCRIPT_UNDER_TEST" --bead rr-docs 2>&1)"
status=$?
set -e
[[ "$status" -ne 0 ]]
grep -q "must provide either remote evidence" <<<"$out"

printf 'All check_ci_closeout_evidence scenarios passed.\n'
