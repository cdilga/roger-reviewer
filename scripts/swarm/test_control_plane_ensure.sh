#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/rr-control-plane-ensure-test.XXXXXX")"
trap 'rm -rf "$WORKDIR"' EXIT

FAKE_BIN="${WORKDIR}/bin"
FAKE_SWARM="${WORKDIR}/swarm"
STATE_DIR="${WORKDIR}/state"
mkdir -p "$FAKE_BIN" "$FAKE_SWARM" "$STATE_DIR"

SESSIONS_FILE="${STATE_DIR}/sessions.txt"
CAPTURE_DIR="${STATE_DIR}/captures"
UP_INVOCATIONS="${STATE_DIR}/control_plane_up.log"
mkdir -p "$CAPTURE_DIR"

cat >"${FAKE_BIN}/tmux" <<'TMUX'
#!/usr/bin/env bash
set -euo pipefail

SESSIONS_FILE="${TEST_SESSIONS_FILE:?}"
CAPTURE_DIR="${TEST_CAPTURE_DIR:?}"

cmd="$1"
shift || true

case "$cmd" in
  has-session)
    [[ "${1:-}" == "-t" ]] || exit 1
    session="${2:-}"
    grep -Fxq "$session" "$SESSIONS_FILE"
    ;;
  capture-pane)
    target=""
    while [[ $# -gt 0 ]]; do
      case "$1" in
        -t)
          target="$2"
          shift 2
          ;;
        *)
          shift
          ;;
      esac
    done
    session="${target%%:*}"
    file="${CAPTURE_DIR}/${session}.txt"
    if [[ -f "$file" ]]; then
      cat "$file"
    fi
    ;;
  *)
    echo "unsupported fake tmux command: $cmd" >&2
    exit 2
    ;;
esac
TMUX
chmod +x "${FAKE_BIN}/tmux"

cat >"${FAKE_SWARM}/control_plane_up.sh" <<'UP'
#!/usr/bin/env bash
set -euo pipefail

SESSIONS_FILE="${TEST_SESSIONS_FILE:?}"
LOG_FILE="${TEST_UP_LOG_FILE:?}"

SESSION=""
MODE="assign"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --session)
      SESSION="$2"
      shift 2
      ;;
    --mode)
      MODE="$2"
      shift 2
      ;;
    --no-ft)
      shift
      ;;
    *)
      shift
      ;;
  esac
done

if [[ -z "$SESSION" ]]; then
  echo "missing --session" >&2
  exit 1
fi

{
  echo "session=$SESSION mode=$MODE"
} >>"$LOG_FILE"

touch "$SESSIONS_FILE"
grep -Fxq "$SESSION" "$SESSIONS_FILE" || echo "$SESSION" >>"$SESSIONS_FILE"
grep -Fxq "${SESSION}-controller" "$SESSIONS_FILE" || echo "${SESSION}-controller" >>"$SESSIONS_FILE"
grep -Fxq "${SESSION}-health" "$SESSIONS_FILE" || echo "${SESSION}-health" >>"$SESSIONS_FILE"
if [[ "$MODE" == "assign" ]]; then
  grep -Fxq "${SESSION}-control-plane" "$SESSIONS_FILE" || echo "${SESSION}-control-plane" >>"$SESSIONS_FILE"
fi
UP
chmod +x "${FAKE_SWARM}/control_plane_up.sh"

cp "${SCRIPT_DIR}/control_plane_ensure.sh" "${FAKE_SWARM}/control_plane_ensure.sh"
chmod +x "${FAKE_SWARM}/control_plane_ensure.sh"

export PATH="${FAKE_BIN}:$PATH"
export TEST_SESSIONS_FILE="$SESSIONS_FILE"
export TEST_CAPTURE_DIR="$CAPTURE_DIR"
export TEST_UP_LOG_FILE="$UP_INVOCATIONS"

touch "$SESSIONS_FILE"
echo "demo" >"$SESSIONS_FILE"

printf 'Scenario 1: check reports missing sessions...\n'
set +e
out="$("${FAKE_SWARM}/control_plane_ensure.sh" --session demo --mode assign --check --json)"
status=$?
set -e
[[ "$status" -ne 0 ]]
[[ "$(jq -r '.status' <<<"$out")" == "missing" ]]
[[ "$(jq -r '.missing_sessions | length' <<<"$out")" -ge 1 ]]

printf 'Scenario 2: ensure starts required sessions...\n'
out="$("${FAKE_SWARM}/control_plane_ensure.sh" --session demo --mode assign --json)"
[[ "$(jq -r '.status' <<<"$out")" == "started" ]]
grep -Fxq "demo-control-plane" "$SESSIONS_FILE"
grep -Fxq "demo-controller" "$SESSIONS_FILE"
grep -Fxq "demo-health" "$SESSIONS_FILE"
grep -q "session=demo mode=assign" "$UP_INVOCATIONS"

printf 'Scenario 3: controller reclaim signal is detected...\n'
cat >"${CAPTURE_DIR}/demo-controller.txt" <<'EOF'
2026-03-31T12:00:00+00:00 nudged pane 3 (continue)
EOF
out="$("${FAKE_SWARM}/control_plane_ensure.sh" --session demo --mode assign --check --json)"
[[ "$(jq -r '.status' <<<"$out")" == "healthy" ]]
[[ "$(jq -r '.controller_signal' <<<"$out")" == "reclaiming" ]]

printf 'Scenario 4: explicit exhausted signal is detected...\n'
cat >"${CAPTURE_DIR}/demo-controller.txt" <<'EOF'
queue is genuinely exhausted; send explicit exhausted-queue report and hold
EOF
out="$("${FAKE_SWARM}/control_plane_ensure.sh" --session demo --mode assign --check --json)"
[[ "$(jq -r '.status' <<<"$out")" == "healthy" ]]
[[ "$(jq -r '.controller_signal' <<<"$out")" == "exhausted" ]]

printf 'All control_plane_ensure scenarios passed.\n'
