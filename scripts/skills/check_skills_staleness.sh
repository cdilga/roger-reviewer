#!/usr/bin/env bash
set -euo pipefail

# Pragmatic staleness guard for the shipped roger-* skills (the same
# allowlist scripts/release/package_skills_bundle.sh ships). This is a
# grep-based net, not a parser: it catches known classes of drift cheaply,
# it does not prove full CLI-grammar correctness. Nothing here is wired into
# CI yet; run it by hand or from a future CI step.
#
# Checks:
#   1. forbidden stale phrases (e.g. "copilot ... preferred provider")
#   2. `rr <verb>` invocations inside fenced code blocks must use a verb from
#      a hardcoded vocabulary drawn from `rr --help` (primary + compat names)
#   3. `rr prs` as a bare example must be paired with a "compat" mention in
#      the same file (primary usage should read `rr queue`)
#   4. any skill documenting the outbound send chain (`rr send draft` /
#      `rr send approve`) must also mention `rr send edit`

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
skills_root="${repo_root}/.claude/skills"

# Same allowlist as scripts/release/package_skills_bundle.sh. Kept as an
# independent literal (not sourced) so this guard does not silently start
# checking zero skills if that script's array shape ever changes.
ROGER_SKILLS=(
  roger-alien-artifact-decision-contract
  roger-copilot-harness
  roger-extreme-software-optimization
  roger-inside-roger-agent
  roger-operator-quickstart
  roger-review-driver
  roger-tui-cheatsheet
  roger-worker-protocol
)

# The live `rr` vocabulary: seven verbs, machine surfaces, and compat aliases
# per docs/CLI_SURFACE_SIMPLIFICATION_CONTRACT.md and `rr --help`. Hardcoded
# deliberately -- this script does not invoke `rr` itself, so it stays usable
# even when the binary isn't built.
KNOWN_VERBS=(
  doctor queue review open findings send setup
  api agent
  prs tui resume return search sessions
  triage draft approve post
  extension update assets
  robot-docs init bridge status
)

is_known_verb() {
  local candidate="$1"
  local verb
  for verb in "${KNOWN_VERBS[@]}"; do
    if [[ "$candidate" == "$verb" ]]; then
      return 0
    fi
  done
  return 1
}

fail=0
checked=0

# --- forbidden stale phrases -------------------------------------------------
# Each entry is an extended-regex (grep -E), matched case-insensitively.
FORBIDDEN_PATTERNS=(
  'is Roger.s preferred provider'
  'copilot[^.]*preferred when available'
  'Authoritative support order: copilot, ?opencode'
)

for skill in "${ROGER_SKILLS[@]}"; do
  skill_md="${skills_root}/${skill}/SKILL.md"
  if [[ ! -f "$skill_md" ]]; then
    echo "FAIL: missing shipped skill file: ${skill_md}" >&2
    fail=1
    continue
  fi
  checked=$((checked + 1))

  for pattern in "${FORBIDDEN_PATTERNS[@]}"; do
    if grep -qiE "$pattern" "$skill_md"; then
      echo "FAIL: ${skill}/SKILL.md matches forbidden stale pattern: ${pattern}" >&2
      fail=1
    fi
  done

  # --- rr <verb> vocabulary check, restricted to fenced code blocks --------
  # Extract fenced ```bash / ```sh blocks and pull the first `rr <word>`
  # token off each command-shaped line. Restricting to code fences avoids
  # false positives from prose ("the rr contract", "rr's grammar", ...).
  in_fence=0
  while IFS= read -r line; do
    if [[ "$line" =~ ^\`\`\`(bash|sh)[[:space:]]*$ ]]; then
      in_fence=1
      continue
    fi
    if [[ "$in_fence" -eq 1 && "$line" =~ ^\`\`\`[[:space:]]*$ ]]; then
      in_fence=0
      continue
    fi
    if [[ "$in_fence" -eq 1 ]]; then
      # Strip a leading shell prompt/comment marker if present, then look for
      # an `rr <verb>` invocation anywhere on the line (a line may show a
      # command after a comment, or multiple commands are not expected here).
      if [[ "$line" =~ (^|[^a-zA-Z_-])rr[[:space:]]+([a-zA-Z][a-zA-Z_-]*) ]]; then
        verb="${BASH_REMATCH[2]}"
        if ! is_known_verb "$verb"; then
          echo "FAIL: ${skill}/SKILL.md uses unknown rr verb '${verb}' in a code example: ${line}" >&2
          fail=1
        fi
      fi
    fi
  done <"$skill_md"

  # --- rr prs primary-usage guard ------------------------------------------
  if grep -q 'rr prs' "$skill_md" && ! grep -qi 'compat' "$skill_md"; then
    echo "FAIL: ${skill}/SKILL.md uses 'rr prs' without a 'compat' mention -- prefer 'rr queue' as the primary form" >&2
    fail=1
  fi

  # --- send-edit completeness guard ----------------------------------------
  if grep -qE 'rr send draft|rr send approve' "$skill_md" && ! grep -q 'rr send edit' "$skill_md"; then
    echo "FAIL: ${skill}/SKILL.md documents the send chain but is missing 'rr send edit'" >&2
    fail=1
  fi
done

echo "checked ${checked}/${#ROGER_SKILLS[@]} shipped roger-* skills"

if [[ "$fail" -ne 0 ]]; then
  echo "check_skills_staleness: FAIL" >&2
  exit 1
fi

echo "check_skills_staleness: PASS"
