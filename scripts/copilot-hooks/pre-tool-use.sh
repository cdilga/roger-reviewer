#!/bin/sh
set -eu

input_json="$(cat || true)"
audit_dir="${RR_COPILOT_HOOK_AUDIT_DIR:-}"

if [ -n "$audit_dir" ]; then
  mkdir -p "$audit_dir"
  printf '%s\n' "$input_json" >> "${audit_dir}/pre-tool-use.jsonl"
fi

case "$input_json" in
  *'"toolName":"bash"'*|*'"toolName": "bash"'*)
    printf '{"permissionDecision":"deny","permissionDecisionReason":"Roger review_readonly policy denies shell execution during Copilot review sessions"}\n'
    exit 0
    ;;
  *'"toolName":"edit"'*|*'"toolName": "edit"'*|*'"toolName":"create"'*|*'"toolName": "create"'*|*'"toolName":"write"'*|*'"toolName": "write"'*)
    printf '{"permissionDecision":"deny","permissionDecisionReason":"Roger review_readonly policy denies repository writes during Copilot review sessions"}\n'
    exit 0
    ;;
  *gh\ pr\ review*|*gh\ api*|*gh\ issue\ comment*|*gh\ pr\ comment*)
    printf '{"permissionDecision":"deny","permissionDecisionReason":"Roger review policy forbids direct GitHub mutation commands"}\n'
    exit 0
    ;;
esac

exit 0
