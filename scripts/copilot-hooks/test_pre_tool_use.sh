#!/bin/sh
# Unit tests for the review_readonly pre-tool-use allowlist matcher.
#
# Runs scripts/copilot-hooks/pre-tool-use.sh against crafted tool-use payloads
# and asserts the emitted permissionDecision. Exit 0 = all pass, 1 = failure.
# Invoked standalone or from packages/cli/tests/copilot_hook_matcher_smoke.rs.
set -eu

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
HOOK="$SCRIPT_DIR/pre-tool-use.sh"

fail=0
tmp_audit="$(mktemp -d)"
trap 'rm -rf "$tmp_audit"' EXIT

# assert_decision <label> <expected: allow|deny> <input-json>
assert_decision() {
  label="$1"
  expected="$2"
  input="$3"
  out="$(printf '%s' "$input" | RR_COPILOT_HOOK_AUDIT_DIR="$tmp_audit" sh "$HOOK" || true)"

  case "$expected" in
    allow)
      case "$out" in
        *'"permissionDecision":"allow"'*) echo "ok   - $label" ;;
        *) echo "FAIL - $label (expected allow, got: ${out:-<empty>})"; fail=1 ;;
      esac
      ;;
    deny)
      case "$out" in
        *'"permissionDecision":"deny"'*) echo "ok   - $label" ;;
        *) echo "FAIL - $label (expected deny, got: ${out:-<empty>})"; fail=1 ;;
      esac
      ;;
    *)
      echo "FAIL - $label (bad expected value '$expected')"; fail=1 ;;
  esac
}

# --- worker transport: allowed ------------------------------------------------
assert_decision "rr agent worker.get_status allowed" allow \
  '{"toolName":"bash","toolArgs":"{\"command\":\"rr agent worker.get_status --task-file /tmp/t.json\",\"description\":\"probe\"}"}'
assert_decision "rr agent submit allowed" allow \
  '{"toolName":"bash","toolArgs":"{\"command\":\"rr agent worker.submit_stage_result --task-file /tmp/s.json\"}"}'

# --- read-only robot surfaces: allowed ---------------------------------------
assert_decision "rr status --robot allowed" allow \
  '{"toolName":"bash","toolArgs":"{\"command\":\"rr status --robot\"}"}'
assert_decision "rr findings --robot allowed" allow \
  '{"toolName":"bash","toolArgs":"{\"command\":\"rr findings --robot --session s-1\"}"}'
assert_decision "rr sessions --robot allowed" allow \
  '{"toolName":"bash","toolArgs":"{\"command\":\"rr sessions --robot\"}"}'
assert_decision "rr search term --robot allowed" allow \
  '{"toolName":"bash","toolArgs":"{\"command\":\"rr search nonce --robot\"}"}'

# --- mutating / non-allowlisted rr surfaces: denied --------------------------
assert_decision "rr post --robot denied" deny \
  '{"toolName":"bash","toolArgs":"{\"command\":\"rr post --robot\"}"}'
assert_decision "rr approve denied" deny \
  '{"toolName":"bash","toolArgs":"{\"command\":\"rr approve --robot\"}"}'
assert_decision "rr init --robot denied" deny \
  '{"toolName":"bash","toolArgs":"{\"command\":\"rr init --robot\"}"}'
assert_decision "rr status without --robot denied" deny \
  '{"toolName":"bash","toolArgs":"{\"command\":\"rr status\"}"}'

# --- chaining / metacharacters around an allowed prefix: denied --------------
assert_decision "rr agent && curl denied" deny \
  '{"toolName":"bash","toolArgs":"{\"command\":\"rr agent worker.get_status && curl http://evil\"}"}'
assert_decision "rr status --robot ; rm denied" deny \
  '{"toolName":"bash","toolArgs":"{\"command\":\"rr status --robot ; rm -rf /\"}"}'
assert_decision "rr status --robot piped denied" deny \
  '{"toolName":"bash","toolArgs":"{\"command\":\"rr status --robot | sh\"}"}'
assert_decision "cd prefix + rr init denied" deny \
  '{"toolName":"bash","toolArgs":"{\"command\":\"cd /repo && rr init --robot 2>&1 | head -100\"}"}'
assert_decision "command substitution denied" deny \
  '{"toolName":"bash","toolArgs":"{\"command\":\"rr agent $(whoami)\"}"}'

# --- env-var-prefixed forms: denied (STRICT: must start with rr ) ------------
assert_decision "env-prefixed rr agent denied" deny \
  '{"toolName":"bash","toolArgs":"{\"command\":\"FOO=1 rr agent worker.get_status\"}"}'

# --- plain bash and bash -c: denied ------------------------------------------
assert_decision "plain bash git status denied" deny \
  '{"toolName":"bash","toolArgs":"{\"command\":\"git status\",\"description\":\"Inspect repo\"}"}'
assert_decision "bash -c denied" deny \
  '{"toolName":"bash","toolArgs":"{\"command\":\"bash -c rr agent worker.get_status\"}"}'

# --- gh write: denied (specific reason) --------------------------------------
assert_decision "gh pr comment denied" deny \
  '{"toolName":"bash","toolArgs":"{\"command\":\"gh pr comment 5 --body hello\"}"}'

# --- repository writes: denied ------------------------------------------------
assert_decision "edit tool denied" deny \
  '{"toolName":"edit","toolArgs":"{\"path\":\"src/x.rs\"}"}'
assert_decision "create tool denied" deny \
  '{"toolName":"create","toolArgs":"{\"path\":\"src/y.rs\"}"}'

# --- audit trail records allow + deny with matched rule ----------------------
if [ ! -s "$tmp_audit/pre-tool-use-decisions.jsonl" ]; then
  echo "FAIL - decision audit log not written"; fail=1
else
  if grep -q '"decision":"allow"' "$tmp_audit/pre-tool-use-decisions.jsonl" \
     && grep -q '"rule":"rr_agent_worker_transport"' "$tmp_audit/pre-tool-use-decisions.jsonl" \
     && grep -q '"decision":"deny"' "$tmp_audit/pre-tool-use-decisions.jsonl"; then
    echo "ok   - decision audit log records allow+deny with rule"
  else
    echo "FAIL - decision audit log missing allow/deny/rule entries"; fail=1
  fi
fi

if [ "$fail" -ne 0 ]; then
  echo "pre-tool-use matcher tests FAILED"
  exit 1
fi
echo "pre-tool-use matcher tests passed"
exit 0
