#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  check_ci_closeout_evidence.sh --bead <id> [--run-url <url> --outcome <outcome>]
                                [--local-only-reason <reason>] [--json]

Checks whether a bead closeout has the minimum required remote evidence.

CI-sensitive categories (remote evidence required):
  - labels: ci or github-actions (remote CI validation category)
  - labels: release or publish (release/publication truth category)

Remote evidence requires both:
  - --run-url  https://github.com/<owner>/<repo>/actions/runs/<id>
  - --outcome  success|failure|cancelled|skipped|timed_out|neutral|action_required

Local-only evidence is sufficient only when the bead is not CI-sensitive.
EOF
}

fail() {
  local msg="$1"
  if [[ "$OUTPUT_JSON" == "1" ]]; then
    jq -n \
      --arg bead_id "$BEAD_ID" \
      --arg message "$msg" \
      --argjson ci_sensitive "$CI_SENSITIVE" \
      --argjson categories "$CATEGORIES_JSON" \
      --argjson labels "$LABELS_JSON" \
      '{
        ok: false,
        bead_id: $bead_id,
        ci_sensitive: $ci_sensitive,
        categories: $categories,
        labels: $labels,
        error: $message
      }'
  else
    echo "ERROR: $msg" >&2
  fi
  exit 1
}

BEAD_ID=""
RUN_URL=""
OUTCOME=""
LOCAL_ONLY_REASON=""
OUTPUT_JSON="0"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bead)
      BEAD_ID="${2:-}"
      shift 2
      ;;
    --run-url)
      RUN_URL="${2:-}"
      shift 2
      ;;
    --outcome)
      OUTCOME="${2:-}"
      shift 2
      ;;
    --local-only-reason)
      LOCAL_ONLY_REASON="${2:-}"
      shift 2
      ;;
    --json)
      OUTPUT_JSON="1"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage
      exit 2
      ;;
  esac
done

if [[ -z "$BEAD_ID" ]]; then
  echo "--bead is required" >&2
  usage
  exit 2
fi

if ! command -v br >/dev/null 2>&1; then
  echo "br command is required" >&2
  exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "jq command is required" >&2
  exit 2
fi

SHOW_JSON="$(br show "$BEAD_ID" --json --no-auto-import --no-auto-flush 2>/dev/null || true)"
if [[ -z "$SHOW_JSON" ]]; then
  echo "failed to read bead: $BEAD_ID" >&2
  exit 1
fi

if ! jq -e 'length > 0 and .[0].id != null' >/dev/null 2>&1 <<<"$SHOW_JSON"; then
  echo "bead not found or malformed response: $BEAD_ID" >&2
  exit 1
fi

LABELS_JSON="$(jq -c '.[0].labels // []' <<<"$SHOW_JSON")"
CATEGORIES_JSON="$(
  jq -c '
    (.[0].labels // [] | map(ascii_downcase)) as $labels |
    [
      if (($labels | index("ci")) != null) or (($labels | index("github-actions")) != null) then
        "remote-ci-validation"
      else
        empty
      end,
      if (($labels | index("release")) != null) or (($labels | index("publish")) != null) then
        "release-publication-truth"
      else
        empty
      end
    ]
  ' <<<"$SHOW_JSON"
)"

if [[ "$CATEGORIES_JSON" == "[]" ]]; then
  CI_SENSITIVE="false"
else
  CI_SENSITIVE="true"
fi

has_remote="false"
if [[ -n "$RUN_URL" || -n "$OUTCOME" ]]; then
  has_remote="true"
fi

if [[ -n "$RUN_URL" ]]; then
  if ! [[ "$RUN_URL" =~ ^https://github\.com/[^/]+/[^/]+/actions/runs/[0-9]+/?$ ]]; then
    fail "--run-url must be a canonical GitHub Actions run URL (https://github.com/<owner>/<repo>/actions/runs/<id>)"
  fi
fi

if [[ -n "$OUTCOME" ]]; then
  case "$OUTCOME" in
    success|failure|cancelled|skipped|timed_out|neutral|action_required)
      ;;
    *)
      fail "--outcome must be one of: success|failure|cancelled|skipped|timed_out|neutral|action_required"
      ;;
  esac
fi

if [[ "$has_remote" == "true" && ( -z "$RUN_URL" || -z "$OUTCOME" ) ]]; then
  fail "remote evidence requires both --run-url and --outcome"
fi

if [[ "$CI_SENSITIVE" == "true" ]]; then
  if [[ -n "$LOCAL_ONLY_REASON" ]]; then
    fail "local-only evidence is not sufficient for CI-sensitive beads; provide --run-url and --outcome"
  fi
  if [[ "$has_remote" != "true" ]]; then
    fail "CI-sensitive bead requires remote run evidence (--run-url and --outcome)"
  fi
else
  if [[ "$has_remote" != "true" && -z "$LOCAL_ONLY_REASON" ]]; then
    fail "non-CI-sensitive bead must provide either remote evidence (--run-url and --outcome) or --local-only-reason"
  fi
fi

if [[ "$OUTPUT_JSON" == "1" ]]; then
  jq -n \
    --arg bead_id "$BEAD_ID" \
    --arg run_url "$RUN_URL" \
    --arg outcome "$OUTCOME" \
    --arg local_only_reason "$LOCAL_ONLY_REASON" \
    --argjson ci_sensitive "$CI_SENSITIVE" \
    --argjson categories "$CATEGORIES_JSON" \
    --argjson labels "$LABELS_JSON" \
    '{
      ok: true,
      bead_id: $bead_id,
      ci_sensitive: $ci_sensitive,
      categories: $categories,
      labels: $labels,
      run_url: (if $run_url == "" then null else $run_url end),
      outcome: (if $outcome == "" then null else $outcome end),
      local_only_reason: (if $local_only_reason == "" then null else $local_only_reason end)
    }'
else
  if [[ "$CI_SENSITIVE" == "true" ]]; then
    echo "OK: $BEAD_ID is CI-sensitive; remote evidence accepted ($RUN_URL, outcome=$OUTCOME)."
  else
    if [[ "$has_remote" == "true" ]]; then
      echo "OK: $BEAD_ID is not CI-sensitive; remote evidence accepted ($RUN_URL, outcome=$OUTCOME)."
    else
      echo "OK: $BEAD_ID is not CI-sensitive; local-only reason recorded: $LOCAL_ONLY_REASON"
    fi
  fi
fi
