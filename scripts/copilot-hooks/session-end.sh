#!/bin/sh
set -eu

input_json="$(cat || true)"
audit_dir="${RR_COPILOT_HOOK_AUDIT_DIR:-}"
[ -n "$audit_dir" ] || exit 0

mkdir -p "$audit_dir"
printf '%s\n' "$input_json" >> "${audit_dir}/session-end.jsonl"
