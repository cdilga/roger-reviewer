#!/usr/bin/env python3
"""Aggregate per-target core archive manifests into one release-build-core manifest."""

from __future__ import annotations

import argparse
import datetime as dt
import json
import pathlib
import sys


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build the aggregated release-build-core manifest."
    )
    parser.add_argument(
        "--version-metadata",
        required=True,
        help="Path to release metadata JSON from derive_calver_version.py",
    )
    parser.add_argument(
        "--core-manifests-dir",
        required=True,
        help="Directory containing core-manifest-*.json files",
    )
    parser.add_argument(
        "--matrix-scope",
        default="first_shipped_subset",
        help="Matrix scope label (for example first_shipped_subset or full_matrix)",
    )
    parser.add_argument(
        "--excluded-target",
        action="append",
        default=[],
        help="Target intentionally excluded from this matrix run",
    )
    parser.add_argument(
        "--output",
        required=True,
        help="Path for aggregate manifest JSON output",
    )
    return parser.parse_args()


def _load_json(path: pathlib.Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def main() -> int:
    args = _parse_args()
    version_metadata_path = pathlib.Path(args.version_metadata)
    manifests_dir = pathlib.Path(args.core_manifests_dir)
    output_path = pathlib.Path(args.output)

    if not version_metadata_path.exists():
        print(f"error: version metadata not found: {version_metadata_path}", file=sys.stderr)
        return 2

    if not manifests_dir.exists():
        print(f"error: manifests dir not found: {manifests_dir}", file=sys.stderr)
        return 2

    version_metadata = _load_json(version_metadata_path)
    manifest_files = sorted(manifests_dir.glob("core-manifest-*.json"))
    if not manifest_files:
        print(
            f"error: no per-target core manifests found in {manifests_dir}",
            file=sys.stderr,
        )
        return 2

    targets = [_load_json(path) for path in manifest_files]
    targets.sort(key=lambda item: item.get("target", ""))

    aggregate = {
        "schema": "roger.release-build-core.v1",
        "generated_at": dt.datetime.now(dt.UTC).isoformat(),
        "version": version_metadata["version"],
        "channel": version_metadata["channel"],
        "tag": version_metadata["tag"],
        "prerelease": version_metadata["prerelease"],
        "artifact_stem": version_metadata["artifact_stem"],
        "workspace_version": version_metadata["workspace_version"],
        "matrix_scope": args.matrix_scope,
        "excluded_targets": sorted(set(args.excluded_target)),
        "built_target_count": len(targets),
        "targets": targets,
        "provenance": version_metadata["provenance"],
    }

    output_path.parent.mkdir(parents=True, exist_ok=True)
    with output_path.open("w", encoding="utf-8") as handle:
        json.dump(aggregate, handle, indent=2, sort_keys=True)
        handle.write("\n")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
