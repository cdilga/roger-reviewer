#!/usr/bin/env bash
set -euo pipefail

workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

cat >"${workdir}/release-metadata.json" <<'EOF'
{
  "channel": "stable",
  "version": "2026.04.01",
  "tag": "v2026.04.01",
  "prerelease": false,
  "artifact_stem": "roger-reviewer-2026.04.01"
}
EOF

bash scripts/release/package_skills_bundle.sh \
  --version-metadata "${workdir}/release-metadata.json" \
  --output-dir "${workdir}/dist"

archive="${workdir}/dist/roger-reviewer-2026.04.01-skills.zip"
manifest="${workdir}/dist/skills-bundle-manifest.json"

test -f "${archive}"
test -f "${manifest}"

jq -e '.schema == "roger.release.skills_bundle.v1"' "${manifest}" >/dev/null
jq -e '.lane == "release-package-skills"' "${manifest}" >/dev/null
jq -e '.version == "2026.04.01"' "${manifest}" >/dev/null
jq -e '.tag == "v2026.04.01"' "${manifest}" >/dev/null
jq -e '.channel == "stable"' "${manifest}" >/dev/null
jq -e '.archive_name == "roger-reviewer-2026.04.01-skills.zip"' "${manifest}" >/dev/null
jq -e '.archive_sha256 | test("^[0-9a-f]{64}$")' "${manifest}" >/dev/null

# Exactly the eight roger-* skills, each with name + dir, sorted by dir.
jq -e '(.skills | length) == 8' "${manifest}" >/dev/null
jq -e '[.skills[].dir] == [
  "roger-alien-artifact-decision-contract",
  "roger-copilot-harness",
  "roger-extreme-software-optimization",
  "roger-inside-roger-agent",
  "roger-operator-quickstart",
  "roger-review-driver",
  "roger-tui-cheatsheet",
  "roger-worker-protocol"
]' "${manifest}" >/dev/null
jq -e 'all(.skills[]; (.name | length) > 0 and (.dir | startswith("roger-")))' "${manifest}" >/dev/null
jq -e '.skills[] | select(.dir == "roger-review-driver") | .name == "Roger Review Driver"' "${manifest}" >/dev/null

# Verify manifest archive_sha256 matches the actual archive bytes.
actual_sha="$(python3 -c "import hashlib,pathlib,sys; print(hashlib.sha256(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest())" "${archive}")"
manifest_sha="$(jq -r '.archive_sha256' "${manifest}")"
if [[ "${actual_sha}" != "${manifest_sha}" ]]; then
  echo "test_package_skills_bundle: archive sha mismatch (${actual_sha} != ${manifest_sha})" >&2
  exit 1
fi

python3 - "${archive}" <<'PY'
import pathlib
import sys
import zipfile

archive = pathlib.Path(sys.argv[1])
with zipfile.ZipFile(archive) as zf:
    names = set(zf.namelist())

required = {
    "roger-alien-artifact-decision-contract/SKILL.md",
    "roger-copilot-harness/SKILL.md",
    "roger-extreme-software-optimization/SKILL.md",
    "roger-inside-roger-agent/SKILL.md",
    "roger-operator-quickstart/SKILL.md",
    "roger-review-driver/SKILL.md",
    "roger-tui-cheatsheet/SKILL.md",
    "roger-worker-protocol/SKILL.md",
}
missing = sorted(required - names)
if missing:
    raise SystemExit(f"skills archive missing required entries: {missing}")

# Allowlist integrity: every archived entry must live under a roger-* skill.
strays = sorted(name for name in names if not name.startswith("roger-"))
if strays:
    raise SystemExit(f"skills archive shipped non-roger entries: {strays}")

# Determinism: a re-zip of the same source produces identical member order.
ordered = [n for n in zf.namelist()]
if ordered != sorted(ordered):
    raise SystemExit("skills archive member order is not deterministic (sorted)")
PY

echo "test_package_skills_bundle: PASS"
