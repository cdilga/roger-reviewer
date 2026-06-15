#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/release/package_skills_bundle.sh \
    --version-metadata <path> \
    [--output-dir <path>]

Package the Roger `roger-*` project skills into a deterministic release
bundle. Mirrors package_extension_bundle.sh: it copies the explicit
roger-* skill allowlist into an unpacked/ tree, deterministically zips it
to <artifact_stem>-skills.zip, and writes skills-bundle-manifest.json
(schema roger.release.skills_bundle.v1).
EOF
}

version_metadata=""
output_dir=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version-metadata)
      version_metadata="${2:-}"
      shift 2
      ;;
    --output-dir)
      output_dir="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$version_metadata" ]]; then
  echo "error: --version-metadata is required" >&2
  usage >&2
  exit 2
fi
if [[ ! -f "$version_metadata" ]]; then
  echo "error: version metadata not found: $version_metadata" >&2
  exit 2
fi

output_dir="${output_dir:-dist/skills}"
mkdir -p "$output_dir"

version="$(jq -r '.version // empty' "$version_metadata")"
artifact_stem="$(jq -r '.artifact_stem // empty' "$version_metadata")"
tag="$(jq -r '.tag // empty' "$version_metadata")"
channel="$(jq -r '.channel // empty' "$version_metadata")"
if [[ -z "$version" || -z "$artifact_stem" || -z "$tag" || -z "$channel" ]]; then
  echo "error: version metadata missing required keys" >&2
  exit 2
fi

# Resolve the repository root so the script runs identically from CI and from a
# developer checkout regardless of the invoking working directory.
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
skills_root="${repo_root}/.claude/skills"

# Explicit allowlist: only these roger-* skills ship. Never glob the skills
# directory — unrelated project skills must never leak into the release.
ROGER_SKILLS=(
  roger-alien-artifact-decision-contract
  roger-copilot-harness
  roger-extreme-software-optimization
  roger-inside-roger-agent
  roger-review-driver
)

unpacked_dir="${output_dir}/unpacked"
rm -rf "$unpacked_dir"
mkdir -p "$unpacked_dir"

for skill in "${ROGER_SKILLS[@]}"; do
  src="${skills_root}/${skill}"
  if [[ ! -d "$src" ]]; then
    echo "error: expected roger skill directory not found: ${src}" >&2
    exit 2
  fi
  if [[ ! -f "${src}/SKILL.md" ]]; then
    echo "error: roger skill missing SKILL.md: ${src}/SKILL.md" >&2
    exit 2
  fi
  # Copy the whole skill directory (SKILL.md plus any referenced assets) under
  # its own name so the unpacked tree mirrors ~/.claude/skills/<name>.
  cp -R "$src" "${unpacked_dir}/${skill}"
done

archive_path="${output_dir}/${artifact_stem}-skills.zip"
python3 - "$unpacked_dir" "$archive_path" <<'PY'
import pathlib
import sys
import zipfile

source = pathlib.Path(sys.argv[1])
archive = pathlib.Path(sys.argv[2])
archive.parent.mkdir(parents=True, exist_ok=True)

# Deterministic zip: sorted walk + ZIP_DEFLATED (mirrors the extension lane).
with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as zf:
    for path in sorted(p for p in source.rglob("*") if p.is_file()):
        rel = path.relative_to(source).as_posix()
        zf.write(path, rel)
PY

python3 - "$archive_path" "$unpacked_dir" "$output_dir" "$version" "$tag" "$channel" "${ROGER_SKILLS[@]}" <<'PY'
import hashlib
import json
import pathlib
import sys

archive = pathlib.Path(sys.argv[1])
unpacked_dir = pathlib.Path(sys.argv[2])
output_dir = pathlib.Path(sys.argv[3])
version = sys.argv[4]
tag = sys.argv[5]
channel = sys.argv[6]
skill_dirs = sys.argv[7:]


def frontmatter_name(skill_md: pathlib.Path) -> str:
    """Extract the `name:` field from a SKILL.md YAML frontmatter block."""
    text = skill_md.read_text(encoding="utf-8")
    lines = text.splitlines()
    if not lines or lines[0].strip() != "---":
        raise SystemExit(f"{skill_md}: missing leading frontmatter delimiter")
    for line in lines[1:]:
        if line.strip() == "---":
            break
        stripped = line.strip()
        if stripped.startswith("name:"):
            return stripped[len("name:"):].strip()
    raise SystemExit(f"{skill_md}: frontmatter missing name field")


skills = []
for skill_dir in skill_dirs:
    skill_md = unpacked_dir / skill_dir / "SKILL.md"
    skills.append({"name": frontmatter_name(skill_md), "dir": skill_dir})
skills.sort(key=lambda item: item["dir"])

manifest = {
    "schema": "roger.release.skills_bundle.v1",
    "lane": "release-package-skills",
    "channel": channel,
    "version": version,
    "tag": tag,
    "archive_name": archive.name,
    "archive_sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
    "skills": skills,
}
manifest_path = output_dir / "skills-bundle-manifest.json"
manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")

print(json.dumps({
    "archive_path": archive.as_posix(),
    "manifest_path": manifest_path.as_posix(),
}))
PY
