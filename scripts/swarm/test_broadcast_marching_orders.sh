#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="${ROOT_DIR}/scripts/swarm/broadcast_marching_orders.sh"

assert_contains() {
  local needle="$1"
  local haystack_file="$2"
  if ! grep -Fq -- "$needle" "$haystack_file"; then
    echo "expected to find '$needle' in $haystack_file" >&2
    exit 1
  fi
}

assert_not_contains() {
  local needle="$1"
  local haystack_file="$2"
  if grep -Fq -- "$needle" "$haystack_file"; then
    echo "did not expect to find '$needle' in $haystack_file" >&2
    exit 1
  fi
}

run_json_detection_case() {
  local workdir="$1"
  local fakebin="${workdir}/json-bin"
  local send_log="${workdir}/json-send.log"
  local tmux_log="${workdir}/json-tmux.log"
  local prompt_file="${workdir}/json-prompt.md"
  local stdout_log="${workdir}/json-stdout.log"
  mkdir -p "$fakebin" "${workdir}/tmp"

  cat >"${fakebin}/ntm" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  status)
    cat <<'JSON'
{
  "session": "rr-v51u-json",
  "panes": [
    {"index": 0, "title": "operator shell", "type": "user", "command": "zsh"},
    {"index": 1, "title": "plain shell title", "type": "codex", "command": "codex-aarch64-a"},
    {"index": 2, "title": "another plain title", "type": "gemini", "command": "gemini"}
  ]
}
JSON
    ;;
  send)
    printf '%s\n' "$*" >>"${FAKE_SEND_LOG}"
    ;;
  *)
    echo "unexpected ntm invocation: $*" >&2
    exit 2
    ;;
esac
EOF
  chmod +x "${fakebin}/ntm"

  cat >"${fakebin}/tmux" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  has-session)
    exit 0
    ;;
  list-panes)
    printf 'unexpected tmux list-panes fallback\n' >>"${FAKE_TMUX_LOG}"
    exit 99
    ;;
  *)
    echo "unexpected tmux invocation: $*" >&2
    exit 2
    ;;
esac
EOF
  chmod +x "${fakebin}/tmux"

  cat >"$prompt_file" <<'EOF'
JSON detection prompt
EOF

  PATH="${fakebin}:$PATH" \
  TMPDIR="${workdir}/tmp" \
  FAKE_SEND_LOG="$send_log" \
  FAKE_TMUX_LOG="$tmux_log" \
  "$SCRIPT" --session rr-v51u-json --prompt-file "$prompt_file" --delay-seconds 0 >"$stdout_log"

  assert_contains "send rr-v51u-json --pane=1" "$send_log"
  assert_contains "send rr-v51u-json --pane=2" "$send_log"
  assert_not_contains "--pane=0" "$send_log"
  assert_contains "pane 1 (codex: plain shell title)" "$stdout_log"
  assert_contains "pane 2 (gemini: another plain title)" "$stdout_log"
  if [[ -f "$tmux_log" ]]; then
    echo "tmux fallback should not have been used when ntm JSON succeeded" >&2
    exit 1
  fi
}

run_tmux_fallback_case() {
  local workdir="$1"
  local fakebin="${workdir}/tmux-bin"
  local send_log="${workdir}/tmux-send.log"
  local prompt_file="${workdir}/tmux-prompt.md"
  local stdout_log="${workdir}/tmux-stdout.log"
  mkdir -p "$fakebin" "${workdir}/tmp"

  cat >"${fakebin}/ntm" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  status)
    exit 1
    ;;
  send)
    printf '%s\n' "$*" >>"${FAKE_SEND_LOG}"
    ;;
  *)
    echo "unexpected ntm invocation: $*" >&2
    exit 2
    ;;
esac
EOF
  chmod +x "${fakebin}/ntm"

  cat >"${fakebin}/tmux" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  has-session)
    exit 0
    ;;
  list-panes)
    cat <<'PANES'
0|operator shell|zsh
1|generic pane title|codex-aarch64-a
2|rr__cc_legacy|zsh
PANES
    ;;
  *)
    echo "unexpected tmux invocation: $*" >&2
    exit 2
    ;;
esac
EOF
  chmod +x "${fakebin}/tmux"

  cat >"$prompt_file" <<'EOF'
tmux fallback prompt
EOF

  PATH="${fakebin}:$PATH" \
  TMPDIR="${workdir}/tmp" \
  FAKE_SEND_LOG="$send_log" \
  "$SCRIPT" --session rr-v51u-tmux --prompt-file "$prompt_file" --delay-seconds 0 >"$stdout_log"

  assert_contains "send rr-v51u-tmux --pane=1" "$send_log"
  assert_contains "send rr-v51u-tmux --pane=2" "$send_log"
  assert_not_contains "--pane=0" "$send_log"
  assert_contains "pane 1 (codex: generic pane title)" "$stdout_log"
  assert_contains "pane 2 (claude: rr__cc_legacy)" "$stdout_log"
}

run_empty_ntm_rows_then_tmux_fallback_case() {
  local workdir="$1"
  local fakebin="${workdir}/empty-json-bin"
  local send_log="${workdir}/empty-json-send.log"
  local prompt_file="${workdir}/empty-json-prompt.md"
  local stdout_log="${workdir}/empty-json-stdout.log"
  mkdir -p "$fakebin" "${workdir}/tmp"

  cat >"${fakebin}/ntm" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  status)
    cat <<'JSON'
{
  "session": "rr-v51u-empty-json",
  "panes": [
    {"index": 0, "title": "retitled codex pane", "type": "user", "command": "codex-aarch64-a"},
    {"index": 1, "title": "operator shell", "type": "user", "command": "zsh"}
  ]
}
JSON
    ;;
  send)
    printf '%s\n' "$*" >>"${FAKE_SEND_LOG}"
    ;;
  *)
    echo "unexpected ntm invocation: $*" >&2
    exit 2
    ;;
esac
EOF
  chmod +x "${fakebin}/ntm"

  cat >"${fakebin}/tmux" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  has-session)
    exit 0
    ;;
  list-panes)
    cat <<'PANES'
0|retitled codex pane|codex-aarch64-a
1|operator shell|zsh
PANES
    ;;
  *)
    echo "unexpected tmux invocation: $*" >&2
    exit 2
    ;;
esac
EOF
  chmod +x "${fakebin}/tmux"

  cat >"$prompt_file" <<'EOF'
empty json fallback prompt
EOF

  PATH="${fakebin}:$PATH" \
  TMPDIR="${workdir}/tmp" \
  FAKE_SEND_LOG="$send_log" \
  "$SCRIPT" --session rr-v51u-empty-json --prompt-file "$prompt_file" --delay-seconds 0 >"$stdout_log"

  assert_contains "send rr-v51u-empty-json --pane=0" "$send_log"
  assert_not_contains "--pane=1" "$send_log"
  assert_contains "pane 0 (codex: retitled codex pane)" "$stdout_log"
}

workdir="$(mktemp -d)"
trap 'rm -rf "${workdir}"' EXIT

run_json_detection_case "$workdir"
run_tmux_fallback_case "$workdir"
run_empty_ntm_rows_then_tmux_fallback_case "$workdir"
