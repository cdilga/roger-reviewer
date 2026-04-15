#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
SCRIPT="${ROOT_DIR}/scripts/swarm/run_assign_watch.sh"

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

run_check_mode_case() {
  local workdir="$1"
  local fakebin="${workdir}/check-bin"
  local stdout_log="${workdir}/check-stdout.log"
  local assign_log="${workdir}/check-assign.log"
  mkdir -p "$fakebin"

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

  cat >"${fakebin}/ntm" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  status)
    cat <<JSON
{"working_directory":"${TEST_PROJECT_ROOT}","agent_counts":{"codex":1,"total":1}}
JSON
    ;;
  activity)
    cat <<'JSON'
{"success":true,"agents":[{"pane":0,"agent_type":"codex","state":"WAITING"}],"summary":{"WAITING":1}}
JSON
    ;;
  assign)
    printf '%s\n' "$*" >>"${FAKE_ASSIGN_LOG}"
    exit 99
    ;;
  *)
    echo "unexpected ntm invocation: $*" >&2
    exit 2
    ;;
esac
EOF
  chmod +x "${fakebin}/ntm"

  TEST_PROJECT_ROOT="$ROOT_DIR" \
  FAKE_ASSIGN_LOG="$assign_log" \
  PATH="${fakebin}:$PATH" \
  "$SCRIPT" --session roger-reviewer --check >"$stdout_log"

  assert_contains "Session scope check passed for roger-reviewer" "$stdout_log"
  assert_contains "\"working_directory\":\"${ROOT_DIR}\"" "$stdout_log"
  assert_contains "\"state\":\"WAITING\"" "$stdout_log"
  if [[ -e "$assign_log" ]]; then
    echo "check mode should not invoke ntm assign" >&2
    exit 1
  fi
}

run_scope_mismatch_case() {
  local workdir="$1"
  local fakebin="${workdir}/mismatch-bin"
  local stderr_log="${workdir}/mismatch-stderr.log"
  local assign_log="${workdir}/mismatch-assign.log"
  mkdir -p "$fakebin"

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

  cat >"${fakebin}/ntm" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  status)
    cat <<'JSON'
{"working_directory":"/tmp/not-roger","agent_counts":{"codex":1,"total":1}}
JSON
    ;;
  assign)
    printf '%s\n' "$*" >>"${FAKE_ASSIGN_LOG}"
    exit 99
    ;;
  *)
    echo "unexpected ntm invocation: $*" >&2
    exit 2
    ;;
esac
EOF
  chmod +x "${fakebin}/ntm"

  if TEST_PROJECT_ROOT="$ROOT_DIR" \
    FAKE_ASSIGN_LOG="$assign_log" \
    PATH="${fakebin}:$PATH" \
    "$SCRIPT" --session roger-reviewer > /dev/null 2>"$stderr_log"; then
    echo "scope mismatch should fail closed" >&2
    exit 1
  fi

  assert_contains "Refusing to run assign-watch for session 'roger-reviewer'" "$stderr_log"
  if [[ -e "$assign_log" ]]; then
    echo "scope mismatch should fail before ntm assign" >&2
    exit 1
  fi
}

run_reject_dry_run_case() {
  local workdir="$1"
  local fakebin="${workdir}/dry-run-bin"
  local stderr_log="${workdir}/dry-run-stderr.log"
  local assign_log="${workdir}/dry-run-assign.log"
  mkdir -p "$fakebin"

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

  cat >"${fakebin}/ntm" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  status)
    cat <<JSON
{"working_directory":"${TEST_PROJECT_ROOT}","agent_counts":{"codex":1,"total":1}}
JSON
    ;;
  assign)
    printf '%s\n' "$*" >>"${FAKE_ASSIGN_LOG}"
    exit 99
    ;;
  *)
    echo "unexpected ntm invocation: $*" >&2
    exit 2
    ;;
esac
EOF
  chmod +x "${fakebin}/ntm"

  if TEST_PROJECT_ROOT="$ROOT_DIR" \
    FAKE_ASSIGN_LOG="$assign_log" \
    PATH="${fakebin}:$PATH" \
    "$SCRIPT" --session roger-reviewer -- --dry-run > /dev/null 2>"$stderr_log"; then
    echo "wrapper should reject --dry-run" >&2
    exit 1
  fi

  assert_contains "Refusing to forward '--dry-run' to 'ntm assign'" "$stderr_log"
  if [[ -e "$assign_log" ]]; then
    echo "--dry-run should be rejected before ntm assign" >&2
    exit 1
  fi
}

run_exec_case() {
  local workdir="$1"
  local fakebin="${workdir}/exec-bin"
  local stdout_log="${workdir}/exec-stdout.log"
  local assign_log="${workdir}/exec-assign.log"
  mkdir -p "$fakebin"

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

  cat >"${fakebin}/ntm" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  status)
    cat <<JSON
{"working_directory":"${TEST_PROJECT_ROOT}","agent_counts":{"codex":2,"total":2}}
JSON
    ;;
  assign)
    printf '%s\n' "$*" >>"${FAKE_ASSIGN_LOG}"
    ;;
  *)
    echo "unexpected ntm invocation: $*" >&2
    exit 2
    ;;
esac
EOF
  chmod +x "${fakebin}/ntm"

  TEST_PROJECT_ROOT="$ROOT_DIR" \
  FAKE_ASSIGN_LOG="$assign_log" \
  PATH="${fakebin}:$PATH" \
  "$SCRIPT" --session roger-reviewer --stop-when-done -- --cod-only --limit 4 >"$stdout_log"

  assert_contains "Starting continuous assignment loop:" "$stdout_log"
  assert_contains "assign roger-reviewer --watch --auto --strategy dependency --watch-interval 10s --delay 2s --stop-when-done --cod-only --limit 4" "$assign_log"
  assert_not_contains "--dry-run" "$assign_log"
}

workdir="$(mktemp -d)"
trap 'rm -rf "${workdir}"' EXIT

run_check_mode_case "$workdir"
run_scope_mismatch_case "$workdir"
run_reject_dry_run_case "$workdir"
run_exec_case "$workdir"
