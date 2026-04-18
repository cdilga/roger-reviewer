#!/bin/sh
set -eu

roger_hook_read_input() {
  ROGER_HOOK_INPUT="$(cat)"
  ROGER_HOOK_INPUT_COMPACT="$(printf '%s' "$ROGER_HOOK_INPUT" | tr '\r\n' '  ')"
  export ROGER_HOOK_INPUT
  export ROGER_HOOK_INPUT_COMPACT
}

roger_hook_extract_string() {
  key="$1"
  printf '%s' "$ROGER_HOOK_INPUT_COMPACT" \
    | sed -n "s/.*\"$key\"[[:space:]]*:[[:space:]]*\"\\([^\"]*\\)\".*/\\1/p" \
    | head -n 1
}

roger_hook_first_string() {
  for key in "$@"; do
    value="$(roger_hook_extract_string "$key" || true)"
    if [ -n "${value:-}" ]; then
      printf '%s' "$value"
      return 0
    fi
  done
  return 1
}

roger_hook_json_escape() {
  printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'
}

roger_hook_artifact_dir() {
  if [ -n "${RR_COPILOT_SESSION_START_ARTIFACT:-}" ]; then
    dirname "$RR_COPILOT_SESSION_START_ARTIFACT"
  else
    printf '%s' "${RR_COPILOT_ARTIFACT_DIR:-$(pwd)/.roger/provider/copilot}"
  fi
}

roger_hook_ensure_artifact_dir() {
  dir="$(roger_hook_artifact_dir)"
  mkdir -p "$dir"
  printf '%s' "$dir"
}

roger_hook_append_json_line() {
  relative_path="$1"
  json_line="$2"
  dir="$(roger_hook_ensure_artifact_dir)"
  printf '%s\n' "$json_line" >> "$dir/$relative_path"
}

roger_hook_session_id() {
  roger_hook_first_string sessionId session_id || true
}

roger_hook_cwd() {
  value="$(roger_hook_first_string cwd || true)"
  if [ -n "${value:-}" ]; then
    printf '%s' "$value"
  else
    printf '%s' "${RR_COPILOT_WORKTREE_ROOT:-$(pwd)}"
  fi
}

roger_hook_source() {
  roger_hook_first_string source || true
}

roger_hook_prompt() {
  roger_hook_first_string initialPrompt initial_prompt prompt || true
}

roger_hook_tool_name() {
  roger_hook_first_string toolName tool_name || true
}

roger_hook_transcript_path() {
  roger_hook_first_string transcriptPath transcript_path || true
}

roger_hook_reason() {
  roger_hook_first_string reason stopReason stop_reason || true
}

roger_hook_command_text() {
  printf '%s' "$ROGER_HOOK_INPUT_COMPACT" \
    | sed -n 's/.*"command"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
    | head -n 1
}

roger_hook_sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    printf '%s' "$1" | sha256sum | awk '{print "sha256:" $1}'
  elif command -v shasum >/dev/null 2>&1; then
    printf '%s' "$1" | shasum -a 256 | awk '{print "sha256:" $1}'
  else
    printf ''
  fi
}
