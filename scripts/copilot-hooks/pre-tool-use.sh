#!/bin/sh
set -eu

# Roger review_readonly pre-tool-use policy hook (POSIX sh).
#
# Baseline posture: DENY every bash / edit / create / write tool call and every
# direct GitHub mutation command, so a Copilot review session cannot execute
# shell, mutate the repository, or post to GitHub.
#
# FAIL-CLOSED CARVE-OUT (bash only): a narrow allowlist permits ONLY the Roger
# worker transport and read-only Roger robot surfaces. Matching rules (STRICT):
#
#   1. Extract the bash command string from toolArgs (`"command":"..."`). If it
#      cannot be extracted, DENY.
#   2. Reject the command outright (DENY) if it contains ANY shell
#      chaining / quoting / expansion metacharacter anywhere:
#         \  "  ;  &  |  `  $  <  >  (  )
#      This rejects `&&`, `||`, `|`, `;`, backticks, `$(...)`, redirection,
#      subshells, and any embedded/escaped quotes. Leading `env=VAL` assignments
#      and `cd <dir> &&` prefixes are therefore rejected too (env prefixes never
#      start with `rr `; cd-prefixes carry `&`). No prefix trimming is performed.
#   3. The command must, after trimming leading whitespace, start literally with
#      `rr ` and match exactly one of:
#         - `rr agent <op> ...`  (the dedicated in-session worker transport;
#           every worker.* operation is read/submit inside Roger's own
#           nonce-gated boundary and rr agent itself rejects --robot)
#         - `rr status|findings|sessions|search ... --robot`  (read-only robot
#           surfaces; `--robot` must be present as a whitespace-delimited token)
#      Everything else stays DENIED: init, triage, draft, approve, post, update,
#      setup, extension, bridge, review, send, return, doctor, help, etc.
#   Copilot exposes no in-harness `/roger-*` bindings (safe_harness_command_bindings
#   returns bindings only for opencode), so no `rr roger-help`-style forms appear
#   in Copilot hook traffic; the read-only fallbacks are `rr status`/`rr findings
#   --robot`, which the allowlist above covers.
#
# gh-write denial is preserved unchanged (checked before the carve-out).
# Both allow and deny decisions are recorded to
# ${RR_COPILOT_HOOK_AUDIT_DIR}/pre-tool-use-decisions.jsonl with the matched rule.

input_json="$(cat || true)"
audit_dir="${RR_COPILOT_HOOK_AUDIT_DIR:-}"
compact="$(printf '%s' "$input_json" | tr '\r\n' '  ')"

# Retain the raw tool-use event (existing audit contract / event counting).
if [ -n "$audit_dir" ]; then
  mkdir -p "$audit_dir" 2>/dev/null || true
  printf '%s\n' "$input_json" >> "${audit_dir}/pre-tool-use.jsonl" 2>/dev/null || true
fi

tool_name=""
case "$compact" in
  *'"toolName":"bash"'*|*'"toolName": "bash"'*) tool_name="bash" ;;
  *'"toolName":"edit"'*|*'"toolName": "edit"'*) tool_name="edit" ;;
  *'"toolName":"create"'*|*'"toolName": "create"'*) tool_name="create" ;;
  *'"toolName":"write"'*|*'"toolName": "write"'*) tool_name="write" ;;
esac

matched_command=""

emit_decision() {
  # $1 = decision (allow|deny), $2 = rule
  [ -n "$audit_dir" ] || return 0
  esc_cmd="$(printf '%s' "$matched_command" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g')"
  printf '{"hook":"pre-tool-use","decision":"%s","rule":"%s","tool":"%s","command":"%s"}\n' \
    "$1" "$2" "$tool_name" "$esc_cmd" \
    >> "${audit_dir}/pre-tool-use-decisions.jsonl" 2>/dev/null || true
}

deny() {
  # $1 = reason, $2 = rule
  emit_decision "deny" "$2"
  printf '{"permissionDecision":"deny","permissionDecisionReason":"%s"}\n' "$1"
  exit 0
}

allow() {
  # $1 = rule
  emit_decision "allow" "$1"
  printf '{"permissionDecision":"allow","permissionDecisionReason":"Roger review_readonly policy permits the Roger worker transport / read-only robot surface"}\n'
  exit 0
}

# Repository writes are always denied.
case "$tool_name" in
  edit|create|write)
    deny "Roger review_readonly policy denies repository writes during Copilot review sessions" "repo_write_denied"
    ;;
esac

if [ "$tool_name" = "bash" ]; then
  # Decode one JSON level (\" -> ") so the embedded command string is plain,
  # then extract the value between "command":" and the next unescaped quote.
  decoded="$(printf '%s' "$compact" | sed 's/\\"/"/g')"
  matched_command="$(printf '%s' "$decoded" \
    | sed -n 's/.*"command"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
    | head -n 1)"

  # No command payload -> deny as generic shell execution.
  if [ -z "$matched_command" ]; then
    deny "Roger review_readonly policy denies shell execution during Copilot review sessions" "bash_no_command"
  fi

  # Preserve gh-write denial (specific reason) before generic checks.
  case "$matched_command" in
    *'gh pr review'*|*'gh api'*|*'gh issue comment'*|*'gh pr comment'*)
      deny "Roger review policy forbids direct GitHub mutation commands" "gh_write_denied"
      ;;
  esac

  # Reject any command carrying shell chaining / quoting / expansion metacharacters.
  case "$matched_command" in
    *'\'*|*'"'*|*';'*|*'&'*|*'|'*|*'`'*|*'$'*|*'<'*|*'>'*|*'('*|*')'*)
      deny "Roger review_readonly policy denies chained or quoted shell commands" "bash_metacharacter"
      ;;
  esac

  # Trim leading whitespace.
  cmd_trimmed="$(printf '%s' "$matched_command" | sed 's/^[[:space:]]*//')"

  # Worker transport: rr agent <op> ...
  case "$cmd_trimmed" in
    'rr agent '*)
      allow "rr_agent_worker_transport"
      ;;
  esac

  # Read-only robot surfaces requiring --robot.
  case "$cmd_trimmed" in
    'rr status '*|'rr findings '*|'rr sessions '*|'rr search '*)
      case " $cmd_trimmed " in
        *' --robot '*)
          allow "rr_readonly_robot_surface"
          ;;
      esac
      deny "Roger review_readonly policy allows read-only rr surfaces only with --robot" "rr_readonly_missing_robot"
      ;;
  esac

  # Anything else is denied shell execution.
  deny "Roger review_readonly policy denies shell execution during Copilot review sessions" "bash_not_allowlisted"
fi

# Non-bash / non-write tools: honor gh-write denial on raw text, then allow.
case "$compact" in
  *'gh pr review'*|*'gh api'*|*'gh issue comment'*|*'gh pr comment'*)
    deny "Roger review policy forbids direct GitHub mutation commands" "gh_write_denied"
    ;;
esac

exit 0
