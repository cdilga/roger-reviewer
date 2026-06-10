#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="${ROOT_DIR}/scripts/swarm/broadcast_testing_lane_orders.sh"

assert_contains() {
  local needle="$1"
  local haystack_file="$2"
  if ! grep -Fq -- "$needle" "$haystack_file"; then
    echo "expected to find '$needle' in $haystack_file" >&2
    exit 1
  fi
}

run_default_browser_owner_case() {
  local workdir="$1"
  local fakebin="${workdir}/json-bin"
  local send_log="${workdir}/json-send.log"
  local prompt_file="${workdir}/testing-prompt.md"
  local stdout_log="${workdir}/json-stdout.log"
  mkdir -p "$fakebin" "${workdir}/tmp"

  cat >"${fakebin}/ntm" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  status)
    cat <<'JSON'
{
  "session": "rr-testing-json",
  "panes": [
    {"index": 1, "title": "codex one", "type": "codex", "command": "codex"},
    {"index": 2, "title": "codex two", "type": "codex", "command": "codex"},
    {"index": 3, "title": "gemini one", "type": "gemini", "command": "gemini"},
    {"index": 4, "title": "codex three", "type": "codex", "command": "codex"}
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
  *)
    echo "unexpected tmux invocation: $*" >&2
    exit 2
    ;;
esac
EOF
  chmod +x "${fakebin}/tmux"

  cat >"$prompt_file" <<'EOF'
Testing lane base prompt
EOF

  PATH="${fakebin}:$PATH" \
  TMPDIR="${workdir}/tmp" \
  FAKE_SEND_LOG="$send_log" \
  bash "$SCRIPT" --session rr-testing-json --prompt-file "$prompt_file" --delay-seconds 0 >"$stdout_log"

  assert_contains "Assigned browser-owner to rr-testing-json pane 1" "$stdout_log"
  assert_contains "Assigned cli-smoke to rr-testing-json pane 2" "$stdout_log"
  assert_contains "Assigned docker-integration to rr-testing-json pane 3" "$stdout_log"
  assert_contains "Assigned recovery-and-contention to rr-testing-json pane 4" "$stdout_log"

  assert_contains "Lane assignment: \`browser-owner\`." "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-1.txt"
  assert_contains "RR_EXTENSION_PROFILE_ROOT" "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-1.txt"
  assert_contains "Watch Agent Mail for browser-task requests" "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-1.txt"
  assert_contains "browser-harness" "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-1.txt"
  assert_contains "browser-harness-edge.sh" "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-1.txt"
  assert_contains "helpers.py" "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-1.txt"
  assert_contains "prepare_browser_test_env.sh --browser edge" "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-1.txt"
  assert_contains "launch_preloaded_browser.sh --browser edge" "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-1.txt"
  assert_contains "Required flow-selection workflow for this pane:" "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-1.txt"
  assert_contains "docs/TESTING.md" "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-1.txt"
  assert_contains "Target: ..." "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-1.txt"
  assert_contains "docs/AUTOMATED_E2E_BUDGET.json" "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-1.txt"
  assert_contains "E2E-05" "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-1.txt"
  assert_contains "PJ-01A" "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-1.txt"
  assert_contains "F02.1" "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-1.txt"
  assert_contains "Lane assignment: \`cli-smoke\`." "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-2.txt"
  assert_contains "send a concrete Agent Mail request" "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-2.txt"
  assert_contains "E2E-01" "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-2.txt"
  assert_contains "PJ-03A + F01" "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-2.txt"
  assert_contains "Lane assignment: \`docker-integration\`." "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-3.txt"
  assert_contains "E2E-04" "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-3.txt"
  assert_contains "run_stitched_full_surface_e2e.sh" "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-3.txt"
  assert_contains "Lane assignment: \`recovery-and-contention\`." "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-4.txt"
  assert_contains "E2E-06" "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-4.txt"
  assert_contains "F17" "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-json/pane-4.txt"
}

run_explicit_pane_override_case() {
  local workdir="$1"
  local fakebin="${workdir}/override-bin"
  local send_log="${workdir}/override-send.log"
  local prompt_file="${workdir}/override-prompt.md"
  local stdout_log="${workdir}/override-stdout.log"
  mkdir -p "$fakebin" "${workdir}/tmp"

  cat >"${fakebin}/ntm" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  status)
    cat <<'JSON'
{
  "session": "rr-testing-override",
  "panes": [
    {"index": 0, "title": "codex zero", "type": "codex", "command": "codex"},
    {"index": 2, "title": "codex two", "type": "codex", "command": "codex"}
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
  *)
    echo "unexpected tmux invocation: $*" >&2
    exit 2
    ;;
esac
EOF
  chmod +x "${fakebin}/tmux"

  cat >"$prompt_file" <<'EOF'
Override prompt
EOF

  PATH="${fakebin}:$PATH" \
  TMPDIR="${workdir}/tmp" \
  FAKE_SEND_LOG="$send_log" \
  bash "$SCRIPT" --session rr-testing-override --prompt-file "$prompt_file" --browser-owner pane:2 --delay-seconds 0 >"$stdout_log"

  assert_contains "Assigned cli-smoke to rr-testing-override pane 0" "$stdout_log"
  assert_contains "Assigned browser-owner to rr-testing-override pane 2" "$stdout_log"
  assert_contains "Lane assignment: \`browser-owner\`." "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-override/pane-2.txt"
  assert_contains "E2E-05" "${workdir}/tmp/roger-reviewer-testing-prompts/rr-testing-override/pane-2.txt"
}

workdir="$(mktemp -d)"
trap 'rm -rf "${workdir}"' EXIT

run_default_browser_owner_case "$workdir"
run_explicit_pane_override_case "$workdir"
