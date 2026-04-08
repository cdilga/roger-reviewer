#!/usr/bin/env python3
"""Build installer/update metadata bundle from release metadata and core manifest."""

from __future__ import annotations

import argparse
import datetime as dt
import json
import pathlib
import sys
from typing import Dict, List, Optional, Tuple

BUNDLE_SCHEMA = "roger.release.install-metadata.v1"
CORE_SCHEMA = "roger.release-build-core.v1"
ALLOWED_MIGRATION_POLICIES = {
    "binary_only",
    "auto_safe",
    "explicit_operator_gate",
    "unsupported",
}
ALLOWED_MIGRATION_CLASS_MAX_AUTO = {"class_a", "class_b", "none"}
DEFAULT_STORE_COMPATIBILITY = {
    "envelope_version": 1,
    "store_schema_version": 10,
    "min_supported_store_schema": 0,
    "auto_migrate_from": 0,
    "migration_policy": "binary_only",
    "migration_class_max_auto": "none",
    "sidecar_generation": "v1",
    "backup_required": True,
}


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build release install-metadata bundle from core manifest outputs."
    )
    parser.add_argument(
        "--version-metadata",
        required=True,
        help="Path to release metadata JSON (derive_calver_version.py output).",
    )
    parser.add_argument(
        "--core-manifest",
        required=True,
        help="Path to release-core-manifest JSON (build_core_manifest.py output).",
    )
    parser.add_argument(
        "--output",
        required=True,
        help="Path to output install metadata JSON.",
    )
    return parser.parse_args()


def _load_json(path: pathlib.Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def _collect_targets(core_manifest: dict) -> Tuple[List[dict], List[str]]:
    errors: List[str] = []
    raw_targets = core_manifest.get("targets")
    if not isinstance(raw_targets, list) or not raw_targets:
        return [], ["core manifest targets must be a non-empty array"]

    normalized: List[dict] = []
    seen = set()
    for index, entry in enumerate(raw_targets):
        if not isinstance(entry, dict):
            errors.append(f"targets[{index}] must be an object")
            continue

        target = entry.get("target")
        archive_name = entry.get("archive_name")
        archive_sha256 = entry.get("archive_sha256")
        payload_dir = entry.get("payload_dir")
        binary_name = entry.get("binary_name")

        missing = [
            key
            for key, value in (
                ("target", target),
                ("archive_name", archive_name),
                ("archive_sha256", archive_sha256),
                ("payload_dir", payload_dir),
                ("binary_name", binary_name),
            )
            if not isinstance(value, str) or not value
        ]
        if missing:
            errors.append(
                f"targets[{index}] missing required string fields: {', '.join(missing)}"
            )
            continue

        if target in seen:
            errors.append(f"duplicate target entry in core manifest: {target}")
            continue
        seen.add(target)

        normalized.append(
            {
                "target": target,
                "archive_name": archive_name,
                "archive_sha256": archive_sha256.lower(),
                "payload_dir": payload_dir,
                "binary_name": binary_name,
                "runner_os": entry.get("runner_os"),
            }
        )

    normalized.sort(key=lambda item: item["target"])
    return normalized, errors


def _normalize_store_compatibility(raw: object) -> Tuple[Optional[dict], List[str]]:
    if raw is None:
        return dict(DEFAULT_STORE_COMPATIBILITY), []
    if not isinstance(raw, dict):
        return None, ["core manifest store_compatibility must be an object"]

    errors: List[str] = []

    def _int_field(name: str) -> Optional[int]:
        value = raw.get(name)
        if not isinstance(value, int) or value < 0:
            errors.append(f"store_compatibility.{name} must be a non-negative integer")
            return None
        return int(value)

    def _str_field(name: str) -> Optional[str]:
        value = raw.get(name)
        if not isinstance(value, str) or not value:
            errors.append(f"store_compatibility.{name} must be a non-empty string")
            return None
        return value

    envelope_version = _int_field("envelope_version")
    store_schema_version = _int_field("store_schema_version")
    min_supported_store_schema = _int_field("min_supported_store_schema")
    auto_migrate_from = _int_field("auto_migrate_from")
    migration_policy = _str_field("migration_policy")
    migration_class_max_auto = _str_field("migration_class_max_auto")
    sidecar_generation = _str_field("sidecar_generation")
    backup_required = raw.get("backup_required")
    if not isinstance(backup_required, bool):
        errors.append("store_compatibility.backup_required must be a boolean")

    if envelope_version is not None and envelope_version < 1:
        errors.append("store_compatibility.envelope_version must be >= 1")
    if (
        store_schema_version is not None
        and min_supported_store_schema is not None
        and min_supported_store_schema > store_schema_version
    ):
        errors.append(
            "store_compatibility.min_supported_store_schema cannot exceed store_schema_version"
        )
    if (
        store_schema_version is not None
        and auto_migrate_from is not None
        and auto_migrate_from > store_schema_version
    ):
        errors.append(
            "store_compatibility.auto_migrate_from cannot exceed store_schema_version"
        )
    if (
        migration_policy is not None
        and migration_policy not in ALLOWED_MIGRATION_POLICIES
    ):
        errors.append(
            "store_compatibility.migration_policy must be one of: "
            + ", ".join(sorted(ALLOWED_MIGRATION_POLICIES))
        )
    if (
        migration_class_max_auto is not None
        and migration_class_max_auto not in ALLOWED_MIGRATION_CLASS_MAX_AUTO
    ):
        errors.append(
            "store_compatibility.migration_class_max_auto must be one of: "
            + ", ".join(sorted(ALLOWED_MIGRATION_CLASS_MAX_AUTO))
        )

    if errors:
        return None, errors

    return (
        {
            "envelope_version": envelope_version,
            "store_schema_version": store_schema_version,
            "min_supported_store_schema": min_supported_store_schema,
            "auto_migrate_from": auto_migrate_from,
            "migration_policy": migration_policy,
            "migration_class_max_auto": migration_class_max_auto,
            "sidecar_generation": sidecar_generation,
            "backup_required": backup_required,
        },
        [],
    )


def main() -> int:
    args = _parse_args()

    version_metadata_path = pathlib.Path(args.version_metadata)
    core_manifest_path = pathlib.Path(args.core_manifest)
    output_path = pathlib.Path(args.output)

    errors: List[str] = []

    if not version_metadata_path.exists():
        errors.append(f"version metadata not found: {version_metadata_path}")
    if not core_manifest_path.exists():
        errors.append(f"core manifest not found: {core_manifest_path}")
    if errors:
        for err in errors:
            print(f"error: {err}", file=sys.stderr)
        return 2

    try:
        version_metadata = _load_json(version_metadata_path)
    except OSError as exc:
        print(f"error: failed to read version metadata: {exc}", file=sys.stderr)
        return 2
    except json.JSONDecodeError as exc:
        print(f"error: invalid version metadata json: {exc}", file=sys.stderr)
        return 2

    try:
        core_manifest = _load_json(core_manifest_path)
    except OSError as exc:
        print(f"error: failed to read core manifest: {exc}", file=sys.stderr)
        return 2
    except json.JSONDecodeError as exc:
        print(f"error: invalid core manifest json: {exc}", file=sys.stderr)
        return 2

    if core_manifest.get("schema") != CORE_SCHEMA:
        errors.append(
            f"core manifest schema mismatch: expected {CORE_SCHEMA!r}, "
            f"got {core_manifest.get('schema')!r}"
        )

    for key in ("version", "channel", "tag", "prerelease", "artifact_stem"):
        if version_metadata.get(key) is None:
            errors.append(f"version metadata missing required field: {key}")

    for key in ("version", "channel", "tag", "prerelease", "artifact_stem"):
        if core_manifest.get(key) != version_metadata.get(key):
            errors.append(
                f"core manifest {key} mismatch: expected {version_metadata.get(key)!r}, "
                f"got {core_manifest.get(key)!r}"
            )

    targets, target_errors = _collect_targets(core_manifest)
    errors.extend(target_errors)
    store_compatibility, compatibility_errors = _normalize_store_compatibility(
        core_manifest.get("store_compatibility")
    )
    errors.extend(compatibility_errors)

    if errors:
        for err in errors:
            print(f"error: {err}", file=sys.stderr)
        return 2

    version = str(version_metadata["version"])
    artifact_stem = str(version_metadata["artifact_stem"])
    bundle = {
        "schema": BUNDLE_SCHEMA,
        "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
        "release": {
            "version": version,
            "channel": version_metadata["channel"],
            "tag": version_metadata["tag"],
            "prerelease": bool(version_metadata["prerelease"]),
            "artifact_stem": artifact_stem,
            "workspace_version": version_metadata.get("workspace_version"),
            "provenance": version_metadata.get("provenance", {}),
        },
        "checksums_name": f"{artifact_stem}-checksums.txt",
        "core_manifest_name": f"release-core-manifest-{version}.json",
        "targets": targets,
        "store_compatibility": store_compatibility,
        "lookup": {
            "allowed_channels": ["stable", "rc"],
            "target_key": "target",
            "channel_default": "stable",
            "source_schema": CORE_SCHEMA,
        },
    }

    output_path.parent.mkdir(parents=True, exist_ok=True)
    with output_path.open("w", encoding="utf-8") as handle:
        json.dump(bundle, handle, indent=2, sort_keys=True)
        handle.write("\n")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
