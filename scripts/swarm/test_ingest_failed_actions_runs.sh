#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="${ROOT_DIR}/scripts/swarm/ingest_failed_actions_runs.py"
BR_BIN="/Users/cdilga/.local/bin/br-0.1.34.pinned"

if [[ ! -x "${BR_BIN}" ]]; then
  echo "missing pinned br binary: ${BR_BIN}" >&2
  exit 1
fi

workdir="$(mktemp -d)"
trap 'rm -rf "${workdir}"' EXIT
cd "${workdir}"

"${BR_BIN}" init --no-daemon >/dev/null

cat >runs-1.json <<'EOF'
{
  "workflow_runs": [
    {
      "id": 101,
      "html_url": "https://github.com/cdilga/roger-reviewer/actions/runs/101",
      "path": ".github/workflows/release-build-core.yml",
      "name": "release-build-core",
      "head_branch": "main",
      "head_sha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "event": "push",
      "status": "completed",
      "conclusion": "failure",
      "display_title": "release-build-core failed",
      "created_at": "2026-04-02T00:00:00Z",
      "updated_at": "2026-04-02T00:01:00Z"
    },
    {
      "id": 102,
      "html_url": "https://github.com/cdilga/roger-reviewer/actions/runs/102",
      "path": ".github/workflows/release-build-core.yml",
      "name": "release-build-core",
      "head_branch": "main",
      "head_sha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      "event": "push",
      "status": "completed",
      "conclusion": "failure",
      "display_title": "release-build-core failed again",
      "created_at": "2026-04-02T00:02:00Z",
      "updated_at": "2026-04-02T00:03:00Z"
    },
    {
      "id": 201,
      "html_url": "https://github.com/cdilga/roger-reviewer/actions/runs/201",
      "path": ".github/workflows/validation-pr.yml",
      "name": "validation-pr",
      "head_branch": "feature-x",
      "head_sha": "cccccccccccccccccccccccccccccccccccccccc",
      "event": "pull_request",
      "status": "completed",
      "conclusion": "failure",
      "display_title": "validation-pr failed",
      "created_at": "2026-04-02T00:04:00Z",
      "updated_at": "2026-04-02T00:05:00Z"
    },
    {
      "id": 301,
      "html_url": "https://github.com/cdilga/roger-reviewer/actions/runs/301",
      "path": ".github/workflows/unrelated.yml",
      "name": "unrelated",
      "head_branch": "main",
      "head_sha": "dddddddddddddddddddddddddddddddddddddddd",
      "event": "push",
      "status": "completed",
      "conclusion": "failure",
      "display_title": "unrelated failure",
      "created_at": "2026-04-02T00:06:00Z",
      "updated_at": "2026-04-02T00:07:00Z"
    }
  ]
}
EOF

python3 "${SCRIPT}" \
  --repo cdilga/roger-reviewer \
  --project-root "${workdir}" \
  --runs-json runs-1.json \
  --br-binary "${BR_BIN}" \
  --parent-id none >out-1.json

jq -e '.created | length == 2' out-1.json >/dev/null
jq -e '.updated | length == 0' out-1.json >/dev/null
jq -e '.ingested_keys == 2' out-1.json >/dev/null

count_after_first=$("${BR_BIN}" list --status open --json --no-daemon | jq '.issues | map(select(.labels | index("ci-failure-intake"))) | length')
[[ "${count_after_first}" == "2" ]] || {
  echo "expected two ci-failure-intake issues after first ingestion, got ${count_after_first}" >&2
  exit 1
}

release_issue_id=$("${BR_BIN}" list --status open --json --no-daemon | jq -r '.issues[] | select(.title == "CI failure intake: .github/workflows/release-build-core.yml [main]") | .id')
[[ -n "${release_issue_id}" ]] || {
  echo "expected release intake issue after first ingestion" >&2
  exit 1
}

"${BR_BIN}" show "${release_issue_id}" --json --no-daemon | jq -r '.[0].notes' | rg -n "run_id: 102" >/dev/null

cat >runs-2.json <<'EOF'
{
  "workflow_runs": [
    {
      "id": 103,
      "html_url": "https://github.com/cdilga/roger-reviewer/actions/runs/103",
      "path": ".github/workflows/release-build-core.yml",
      "name": "release-build-core",
      "head_branch": "main",
      "head_sha": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
      "event": "push",
      "status": "completed",
      "conclusion": "failure",
      "display_title": "release-build-core failed third time",
      "created_at": "2026-04-02T00:08:00Z",
      "updated_at": "2026-04-02T00:09:00Z"
    }
  ]
}
EOF

python3 "${SCRIPT}" \
  --repo cdilga/roger-reviewer \
  --project-root "${workdir}" \
  --runs-json runs-2.json \
  --br-binary "${BR_BIN}" \
  --parent-id none >out-2.json

jq -e '.created | length == 0' out-2.json >/dev/null
jq -e '.updated | length == 1' out-2.json >/dev/null

count_after_second=$("${BR_BIN}" list --status open --json --no-daemon | jq '.issues | map(select(.labels | index("ci-failure-intake"))) | length')
[[ "${count_after_second}" == "2" ]] || {
  echo "expected still two ci-failure-intake issues after duplicate update, got ${count_after_second}" >&2
  exit 1
}

"${BR_BIN}" show "${release_issue_id}" --json --no-daemon | jq -r '.[0].notes' | rg -n "run_id: 103" >/dev/null

echo "test_ingest_failed_actions_runs: PASS"
