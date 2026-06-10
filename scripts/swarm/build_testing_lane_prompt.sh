#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
BROWSER_HARNESS_ROOT="${HOME}/Developer/browser-harness"
BROWSER_HARNESS_EDGE_WRAPPER="${PROJECT_ROOT}/scripts/browser-harness-edge.sh"
EXTENSION_PREPARE_SCRIPT="${PROJECT_ROOT}/scripts/extension/prepare_browser_test_env.sh"
EXTENSION_LAUNCH_SCRIPT="${PROJECT_ROOT}/scripts/extension/launch_preloaded_browser.sh"

if [[ $# -lt 4 || $# -gt 5 ]]; then
  echo "Usage: $(basename "$0") <base-prompt-file> <output-file> <session-name> <lane-role> [agent-type]" >&2
  exit 1
fi

BASE_PROMPT_FILE="$1"
OUTPUT_FILE="$2"
SESSION_NAME="$3"
LANE_ROLE="$4"
AGENT_TYPE="${5:-unknown}"

case "$LANE_ROLE" in
  browser-owner|cli-smoke|docker-integration|recovery-and-contention|general-cli)
    ;;
  *)
    echo "Invalid lane role '$LANE_ROLE'." >&2
    exit 1
    ;;
esac

if [[ ! -f "$BASE_PROMPT_FILE" ]]; then
  echo "Prompt file not found: $BASE_PROMPT_FILE" >&2
  exit 1
fi

lane_guidance() {
  case "$LANE_ROLE" in
    browser-owner)
      cat <<'EOF' | sed \
        -e 's|__DOLLAR__|$|g' \
        -e "s|__BROWSER_HARNESS_ROOT__|${BROWSER_HARNESS_ROOT}|g" \
        -e "s|__BROWSER_HARNESS_EDGE_WRAPPER__|${BROWSER_HARNESS_EDGE_WRAPPER}|g" \
        -e "s|__EXTENSION_PREPARE_SCRIPT__|${EXTENSION_PREPARE_SCRIPT}|g" \
        -e "s|__EXTENSION_LAUNCH_SCRIPT__|${EXTENSION_LAUNCH_SCRIPT}|g"
Browser-owner lane:

- You are the only pane allowed to use the real browser for this session.
- Set a dedicated extension profile root after the common bootstrap:

```bash
export RR_EXTENSION_PROFILE_ROOT="__DOLLAR__HOME/.cache/roger-reviewer-testing/__DOLLAR__{RR_SWARM_SESSION}/browser-profile-__DOLLAR__RR_SWARM_SLOT"
mkdir -p "__DOLLAR__RR_EXTENSION_PROFILE_ROOT"
```

- Browser harness is installed globally as `browser-harness`. Before first
  browser use in this pane, read:
  - `__BROWSER_HARNESS_ROOT__/install.md` for attach/bootstrap
  - `__BROWSER_HARNESS_ROOT__/SKILL.md` for normal usage
  - `__BROWSER_HARNESS_ROOT__/helpers.py` because that is the real function surface
- For Roger browser work, prefer the Edge-targeted wrapper:

```bash
__BROWSER_HARNESS_EDGE_WRAPPER__ <<'PY'
print(page_info())
PY
```

- The global `browser-harness` install is still available for Chrome or Brave
  verification when a target specifically needs those browsers.
- When you open a setup tab or verification tab through the harness, activate it
  so the operator can see the active browser tab.
- Focus on `rr extension setup`, `rr extension doctor`, Native Messaging truth,
  extension install/repair loops, and browser-assisted Roger smoke.
- For the primary scripted setup/reload path, prefer Roger's higher-level
  browser bootstrap helper first:

```bash
__EXTENSION_PREPARE_SCRIPT__ --browser edge --profile-root "__DOLLAR__RR_EXTENSION_PROFILE_ROOT" --reset-profile
```

- When you only need a quick reload against an already-good dedicated profile,
  the lower-level launcher remains available:

```bash
__EXTENSION_LAUNCH_SCRIPT__ --browser edge --profile-root "__DOLLAR__RR_EXTENSION_PROFILE_ROOT" --close-existing
```

- Use the harness to drive visible, reproducible browser verification instead of
  ad hoc AppleScript-only pokes wherever possible.
- Watch Agent Mail for browser-task requests from the other testing lanes and
  acknowledge them when they need you to run a browser-only step.
- Announce when you begin and end browser use so other lanes do not contend for
  it indirectly.
- If a task does not require the browser, prefer CLI validation from the same
  isolated worktree/store instead of keeping the browser busy.
EOF
      ;;
    cli-smoke)
      cat <<'EOF'
CLI smoke lane:

- Focus on high-frequency CLI truth checks: `rr review --dry-run --robot`,
  `rr status --robot`, `rr sessions --robot`, `rr findings --robot`, and
  `rr search --robot`.
- Exercise provider selection truth without assuming `rr return` support for
  bounded Tier A providers.
- If a browser-only action is required, send a concrete Agent Mail request to
  the browser-owner lane instead of touching the browser yourself.
- Prefer concise, repeatable repro commands over long manual exploration.
EOF
      ;;
    docker-integration)
      cat <<'EOF'
Docker integration lane:

- Focus on container-backed, service-backed, and heavier integration journeys.
- Keep `COMPOSE_PROJECT_NAME` unique per pane and derive unique port offsets
  from `RR_SWARM_SLOT` if a service wants fixed ports.
- Prefer `rch exec -- <cargo/test command>` for heavy builds or Rust test
  loops; continue locally if no workers are available.
- If a browser-only step becomes necessary, route it through Agent Mail to the
  browser-owner lane with the exact command and expected artifact.
- Do not touch the real browser.
EOF
      ;;
    recovery-and-contention)
      cat <<'EOF'
Recovery-and-contention lane:

- Focus on repeated start/stop, fail-closed behavior, concurrent invocation,
  bridge contract commands, update dry-runs, and contention repros.
- Exercise restart/recovery paths and race-prone journeys with isolated state.
- Use Agent Mail to request any browser-only reproduction you need from the
  browser-owner lane; keep this lane focused on non-browser contention work.
- Do not touch the real browser unless the operator explicitly reassigns you.
EOF
      ;;
    general-cli)
      cat <<'EOF'
General CLI lane:

- Stay in isolated CLI-only Roger paths and pick the next highest-value
  untested or weakly tested operator journey.
- Prefer breadth across real `rr` commands before deep provider-specific
  digging, unless a reproducible failure already points you elsewhere.
- Use Agent Mail to hand browser-only steps to the browser-owner lane instead
  of competing for the browser yourself.
- Do not touch the real browser.
EOF
      ;;
  esac
}

lane_target_catalog() {
  case "$LANE_ROLE" in
    browser-owner)
      cat <<'EOF'
Suggested starting targets for this lane:

- `E2E-05` -> `e2e_browser_setup_first_launch`
  Persona anchors: `PJ-01A`, `PJ-01B`, `PJ-01C`
  Flow anchors: `F02`, `F02.1`, `F10`, `F14`
  Cheaper proof owners when heavyweight E2E is unnecessary:
  `smoke_browser_launch_chrome`, `smoke_browser_launch_brave`,
  `smoke_browser_launch_edge`, `int_bridge_launch_only_no_status`
- For narrower truthful browser recovery work, start from `PJ-01C + F10` or
  `PJ-01A + F02.1` before inventing ad hoc setup pokes.
EOF
      ;;
    cli-smoke)
      cat <<'EOF'
Suggested starting targets for this lane:

- `E2E-01` -> `e2e_core_review_happy_path`
  Persona anchors: `PJ-03A`, `PJ-05A`
  Flow anchors: `F01`, `F03`, `F07`
- `E2E-03` -> `e2e_tui_first_memory_triage`
  Persona anchors: `PJ-03A`, `PJ-03C`, `PJ-04A`
  Flow anchors: `F01`, `F04`, `F05`, `F08`, `F09`
- Strong narrower CLI-first pairs:
  `PJ-03A + F01`, `PJ-03C + F01.1`, `PJ-03D + F01.2`, `PJ-03A + F08`
- Cheaper proof owners worth preferring for narrower truth:
  `int_cli_session_aware`, `int_harness_opencode_resume`
EOF
      ;;
    docker-integration)
      cat <<'EOF'
Suggested starting targets for this lane:

- `E2E-04` -> `e2e_refresh_draft_reconciliation`
  Persona anchors: `PJ-02D`, `PJ-04D`, `PJ-05B`
  Flow anchors: `F06`, `F07`, `F08`, `F13`
- `E2E-01` -> `e2e_core_review_happy_path`
  Persona anchors: `PJ-03A`, `PJ-05A`
  Flow anchors: `F01`, `F03`, `F07`
- If you need the aggregate only because the operator explicitly wants it, use:
  `./scripts/swarm/run_stitched_full_surface_e2e.sh --artifact-root <out-dir>`
- Cheaper proof owners to prefer before a heavyweight rerun:
  `int_github_outbound_audit`, `int_github_posting_safety_recovery`,
  `prop_refresh_identity_lifecycle`
EOF
      ;;
    recovery-and-contention)
      cat <<'EOF'
Suggested starting targets for this lane:

- `E2E-06` -> `e2e_harness_dropout_return`
  Persona anchors: `PJ-03C`, `PJ-04A`, `PJ-04B`
  Flow anchors: `F01.1`, `F05`, `F17`, `F17.1`
- Recovery-heavy flow pairs:
  `PJ-04A + F17`, `PJ-04B + F17.1`, `PJ-02D + F06`, `PJ-01C + F10`,
  `PJ-06A + F10`, `PJ-06C + F14`
- Cheaper proof owners worth preferring:
  `accept_opencode_dropout_return`, `accept_opencode_resume`,
  `smoke_opencode_continuity`, `int_storage_opencode_dropout_return`
EOF
      ;;
    general-cli)
      cat <<'EOF'
Suggested starting targets for this lane:

- `E2E-02` -> `e2e_cross_surface_review_continuity`
  Persona anchors: `PJ-02A`, `PJ-02D`, `PJ-04A`, `PJ-04B`
  Flow anchors: `F02`, `F01.1`, `F05`, `F07`, `F09`
- `E2E-03` -> `e2e_tui_first_memory_triage`
  Persona anchors: `PJ-03A`, `PJ-03C`, `PJ-04A`
  Flow anchors: `F01`, `F04`, `F05`, `F08`, `F09`
- Strong narrower pairs:
  `PJ-02A + F01.1`, `PJ-02D + F06`, `PJ-03A + F09`, `PJ-04A + F17`
EOF
      ;;
  esac
}

mkdir -p "$(dirname "$OUTPUT_FILE")"

{
  cat "$BASE_PROMPT_FILE"
  cat <<EOF

Testing swarm isolation bootstrap:

- Derive a stable slot from \`TMUX_PANE\` and isolate Roger state before using \`rr\`.
- Run the following shell bootstrap once per pane, reusing the same paths on later cycles:

\`\`\`bash
slot="\${TMUX_PANE#%}"
export RR_SWARM_SESSION="${SESSION_NAME}"
export RR_SWARM_SLOT="\$slot"
export RR_STORE_ROOT="\$HOME/.local/share/roger-reviewer-testing/${SESSION_NAME}/store-\$slot"
export CARGO_TARGET_DIR="\$HOME/.cache/roger-reviewer-testing/${SESSION_NAME}/cargo-target-\$slot"
export COMPOSE_PROJECT_NAME="rrtest-${SESSION_NAME}-\$slot"
export TMPDIR="\$HOME/.cache/roger-reviewer-testing/${SESSION_NAME}/tmp-\$slot"
mkdir -p "\$RR_STORE_ROOT" "\$CARGO_TARGET_DIR" "\$TMPDIR"
wt="\$HOME/.cache/roger-reviewer-testing/${SESSION_NAME}/worktrees/wt-\$slot"
if [[ ! -e "\$wt/.git" ]]; then
  mkdir -p "\$(dirname "\$wt")"
  git worktree add --detach "\$wt" HEAD
fi
cd "\$wt"
\`\`\`

- Never hand-edit \`.beads/issues.jsonl\`. If bead storage looks unhealthy, report it and avoid direct JSONL surgery.
- If you edit repo files, reserve them through Agent Mail first.
- Use Agent Mail as the coordination channel between testing lanes. If you need
  another lane to run a step, send a concrete request with the exact command,
  inputs, and expected artifact rather than assuming ambient coordination.
- Prefer installed \`rr\` on PATH for operator-path testing; use repo-local cargo invocations only when you are explicitly validating build-from-source behavior.
- Use \`rch exec -- <command>\` for CPU-heavy Cargo work when available, but treat local fail-open execution as normal if no worker fleet is configured.
- Browser harness availability:
  - global general harness: \`browser-harness\`
  - Roger Edge-preferring wrapper: \`${BROWSER_HARNESS_EDGE_WRAPPER}\`
  - only the browser-owner lane should touch the real browser unless the
    operator explicitly reassigns it
- Lane assignment: \`${LANE_ROLE}\`.
- Agent type for this pane: \`${AGENT_TYPE}\`.

Required flow-selection workflow for this pane:

1. Pick one concrete target before running broad operator-path commands:
   - a budgeted E2E such as \`E2E-01\` through \`E2E-06\`
   - or one persona-plus-flow pair such as \`PJ-03A + F01\`
   - or one named cheaper proof owner when the testing docs say that is the
     truthful place to start
2. Announce the target explicitly in your first substantive output using:
   - \`Target: ...\`
   - \`Why this lane owns it: ...\`
   - \`Planned proof: ...\`
3. Start the live operator path immediately after that declaration. Do not spend
   turns on a full testing-doc reread before the first real attempt.
4. If the operator path blocks or a support claim is ambiguous, consult only the
   minimum canonical testing doc needed to disambiguate it:
   - \`docs/TESTING.md\`
   - \`docs/PERSONA_JOURNEYS_AND_CHAOS_RECOVERY.md\`
   - \`docs/REVIEW_FLOW_MATRIX.md\`
   - \`docs/RELEASE_AND_TEST_MATRIX.md\`
   - \`docs/TEST_EXECUTION_TIERS_AND_E2E_BUDGET.md\`
5. Prefer one specific journey over the stitched all-E2E aggregate unless the
   operator explicitly asks for the aggregate run.

Machine-readable E2E catalog source:

- \`docs/AUTOMATED_E2E_BUDGET.json\` maps each blessed E2E to persona ids,
  flow ids, executable suites, and cheaper proof owners.

EOF
  lane_guidance
  echo
  lane_target_catalog
} >"$OUTPUT_FILE"
