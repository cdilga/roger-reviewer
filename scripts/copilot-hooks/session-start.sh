#!/bin/sh
set -eu

input_json="$(cat || true)"
artifact_path="${RR_COPILOT_SESSION_START_ARTIFACT:-}"
audit_dir="${RR_COPILOT_HOOK_AUDIT_DIR:-}"
worktree_root="${RR_COPILOT_WORKTREE_ROOT:-$(pwd)}"
attempt_nonce="${RR_COPILOT_ATTEMPT_ID:-}"
policy_digest="${RR_COPILOT_POLICY_PROFILE_DIGEST:-}"
copilot_home="${COPILOT_HOME:-${HOME:-}/.copilot}"
state_root="${copilot_home}/session-state"

if [ -n "$audit_dir" ]; then
  mkdir -p "$audit_dir"
  printf '%s\n' "$input_json" >> "${audit_dir}/session-start.jsonl"
fi

[ -n "$artifact_path" ] || exit 0

extract_string_field() {
  printf '%s' "$input_json" | sed -n "s/.*\"$1\"[[:space:]]*:[[:space:]]*\"\\([^\"]*\\)\".*/\\1/p" | head -n 1
}

session_id="$(extract_string_field sessionId)"
if [ -d "$state_root" ]; then
  attempts=20
  if [ -z "$session_id" ]; then
    while [ "$attempts" -gt 0 ]; do
      session_dir="$(ls -td "$state_root"/*/ 2>/dev/null | head -n 1 || true)"
      if [ -n "$session_dir" ]; then
        session_id="$(basename "$session_dir")"
        break
      fi
      attempts=$((attempts - 1))
      sleep 0.25
    done
  fi
fi

json_escape() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

mkdir -p "$(dirname "$artifact_path")"
printf '{"hook":"session-start","payload":{"provider":"copilot","session_id":"%s","worktree_root":"%s","launch_profile_id":"profile-open-pr","attempt_nonce":"%s","policy_digest":"%s"}}\n' \
  "$(json_escape "$session_id")" \
  "$(json_escape "$worktree_root")" \
  "$(json_escape "$attempt_nonce")" \
  "$(json_escape "$policy_digest")" \
  > "$artifact_path"
