#!/usr/bin/env python3
"""Verify release artifacts and emit publish-gate metadata.

This script validates:
- core archive presence + checksum + minimal content shape
- optional lane artifact presence and support-claim consistency
- deterministic SHA256SUMS and machine-readable release asset manifest output

Exit codes:
- 0 on successful verification (publish_allowed=true)
- 1 on verification failure (publish_allowed=false)
- 2 on invalid invocation/input shape
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import pathlib
import sys
import tarfile
from dataclasses import dataclass
from typing import Any, Dict, Iterable, List, Optional, Tuple

CORE_MANIFEST_SCHEMA = "roger.release-build-core.v1"
INSTALL_METADATA_SCHEMA = "roger.release.install-metadata.v1"


@dataclass(frozen=True)
class VerifiedAsset:
    lane: str
    kind: str
    label: str
    path: pathlib.Path
    sha256: str
    bytes: int


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Verify release assets and build publish-gate outputs."
    )
    parser.add_argument(
        "--version-metadata",
        required=True,
        help="Path to release metadata JSON from derive_calver_version.py",
    )
    parser.add_argument(
        "--core-manifest",
        required=True,
        help="Path to aggregated core manifest JSON (build_core_manifest.py output)",
    )
    parser.add_argument(
        "--asset-root",
        required=True,
        help="Root directory containing release artifacts referenced by manifests",
    )
    parser.add_argument(
        "--optional-summary",
        action="append",
        default=[],
        help=(
            "Path to optional-lane summary JSON "
            "(build_optional_lane_summary.py output); repeatable"
        ),
    )
    parser.add_argument(
        "--output-dir",
        required=True,
        help="Directory where SHA256SUMS and release-asset-manifest.json are written",
    )
    return parser.parse_args()


def _load_json(path: pathlib.Path, errors: List[str], label: str) -> Optional[Dict[str, Any]]:
    try:
        with path.open("r", encoding="utf-8") as handle:
            value = json.load(handle)
    except OSError as exc:
        errors.append(f"{label}: failed to read {path}: {exc}")
        return None
    except json.JSONDecodeError as exc:
        errors.append(f"{label}: invalid json in {path}: {exc}")
        return None
    if not isinstance(value, dict):
        errors.append(f"{label}: expected object in {path}")
        return None
    return value


def _sha256_bytes(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def _sha256_file(path: pathlib.Path) -> str:
    return _sha256_bytes(path.read_bytes())


def _display_path(path: pathlib.Path, root: pathlib.Path) -> str:
    try:
        return path.resolve().relative_to(root.resolve()).as_posix()
    except Exception:
        return path.as_posix()


def _resolve_asset(
    descriptor: str,
    asset_root: pathlib.Path,
    errors: List[str],
    label: str,
) -> Optional[pathlib.Path]:
    raw = pathlib.Path(descriptor)
    candidates: List[pathlib.Path] = []

    if raw.is_absolute() and raw.exists():
        return raw

    direct_candidates = [
        raw,
        asset_root / descriptor,
        asset_root / raw.name,
    ]
    for candidate in direct_candidates:
        if candidate.exists() and candidate.is_file():
            candidates.append(candidate)

    if not candidates:
        matches = sorted(p for p in asset_root.rglob(raw.name) if p.is_file())
        candidates.extend(matches)

    unique = []
    seen = set()
    for candidate in candidates:
        key = candidate.resolve().as_posix()
        if key not in seen:
            seen.add(key)
            unique.append(candidate)

    if not unique:
        errors.append(f"{label}: missing artifact for descriptor '{descriptor}'")
        return None
    if len(unique) > 1:
        locations = ", ".join(path.as_posix() for path in unique)
        errors.append(
            f"{label}: ambiguous artifact for descriptor '{descriptor}': {locations}"
        )
        return None
    return unique[0]


def _expected_posture(bridge_status: str, extension_status: str) -> str:
    if bridge_status == "built" and extension_status == "built":
        return "core_plus_bridge_plus_extension"
    if bridge_status == "built":
        return "core_plus_bridge"
    return "core_only"


def _validate_summary_claims(
    summary: Dict[str, Any],
    source: pathlib.Path,
    errors: List[str],
) -> None:
    lanes = summary.get("lanes", {})
    support_claims = summary.get("support_claims", {})
    if not isinstance(lanes, dict) or not isinstance(support_claims, dict):
        errors.append(f"optional summary {source}: lanes/support_claims must be objects")
        return

    bridge_status = (
        lanes.get("release-package-bridge", {}).get("status")
        if isinstance(lanes.get("release-package-bridge"), dict)
        else None
    )
    extension_status = (
        lanes.get("release-package-extension", {}).get("status")
        if isinstance(lanes.get("release-package-extension"), dict)
        else None
    )
    narrowed_claims = support_claims.get("narrowed_claims", [])
    posture = support_claims.get("posture")

    if not isinstance(narrowed_claims, list):
        errors.append(f"optional summary {source}: support_claims.narrowed_claims must be array")
        return

    narrowed_set = {str(item) for item in narrowed_claims}

    if bridge_status != "built" and "bridge_registration_unshipped" not in narrowed_set:
        errors.append(
            f"lane-claim drift in {source}: bridge lane is {bridge_status} without "
            "bridge_registration_unshipped"
        )
    if bridge_status == "built" and "bridge_registration_unshipped" in narrowed_set:
        errors.append(
            f"lane-claim drift in {source}: bridge lane built but narrowed_claims still include "
            "bridge_registration_unshipped"
        )

    if extension_status != "built" and "extension_sideload_unshipped" not in narrowed_set:
        errors.append(
            f"lane-claim drift in {source}: extension lane is {extension_status} without "
            "extension_sideload_unshipped"
        )
    if extension_status == "built" and "extension_sideload_unshipped" in narrowed_set:
        errors.append(
            f"lane-claim drift in {source}: extension lane built but narrowed_claims still include "
            "extension_sideload_unshipped"
        )

    if extension_status == "built" and bridge_status != "built":
        if "browser_launch_claim_blocked_without_bridge" not in narrowed_set:
            errors.append(
                f"lane-claim drift in {source}: extension built without bridge built but "
                "browser_launch_claim_blocked_without_bridge is missing"
            )

    expected_posture = _expected_posture(str(bridge_status), str(extension_status))
    if posture != expected_posture:
        errors.append(
            f"lane-claim drift in {source}: posture={posture!r} expected={expected_posture!r}"
        )


def _verify_core_assets(
    metadata: Dict[str, Any],
    core_manifest: Dict[str, Any],
    asset_root: pathlib.Path,
    errors: List[str],
) -> List[VerifiedAsset]:
    assets: List[VerifiedAsset] = []

    if core_manifest.get("schema") != CORE_MANIFEST_SCHEMA:
        errors.append(
            "core manifest schema mismatch: "
            f"expected {CORE_MANIFEST_SCHEMA}, got {core_manifest.get('schema')!r}"
        )
        return assets

    for key in ("version", "channel", "tag"):
        if core_manifest.get(key) != metadata.get(key):
            errors.append(
                f"core manifest {key} mismatch: expected {metadata.get(key)!r}, "
                f"got {core_manifest.get(key)!r}"
            )

    targets = core_manifest.get("targets")
    if not isinstance(targets, list) or not targets:
        errors.append("core manifest has no targets")
        return assets

    for target in targets:
        if not isinstance(target, dict):
            errors.append("core manifest contains non-object target entry")
            continue

        target_name = str(target.get("target", "unknown-target"))
        archive_name = target.get("archive_name")
        archive_sha = target.get("archive_sha256")
        if not archive_name or not archive_sha:
            errors.append(f"core target {target_name}: missing archive_name/archive_sha256")
            continue

        archive_path = _resolve_asset(str(archive_name), asset_root, errors, f"core:{target_name}")
        if archive_path is None:
            continue

        observed_sha = _sha256_file(archive_path)
        if observed_sha != archive_sha:
            errors.append(
                f"checksum mismatch for core target {target_name}: "
                f"expected {archive_sha}, got {observed_sha}"
            )
            continue

        binary_name = str(target.get("binary_name") or "rr")
        try:
            with tarfile.open(archive_path, "r:gz") as tar:
                names = tar.getnames()
        except (OSError, tarfile.TarError) as exc:
            errors.append(f"core archive {archive_path}: invalid tar.gz ({exc})")
            continue

        has_binary = any(name.endswith(f"/{binary_name}") or name == binary_name for name in names)
        if not has_binary:
            errors.append(
                f"core archive {archive_path.name}: missing expected binary {binary_name}"
            )
            continue

        assets.append(
            VerifiedAsset(
                lane="release-build-core",
                kind="core_archive",
                label=target_name,
                path=archive_path,
                sha256=observed_sha,
                bytes=archive_path.stat().st_size,
            )
        )

    return assets


def _verify_core_manifest_asset(
    metadata: Dict[str, Any],
    core_manifest_path: pathlib.Path,
    errors: List[str],
) -> Optional[VerifiedAsset]:
    if not core_manifest_path.exists() or not core_manifest_path.is_file():
        errors.append(f"core manifest file missing: {core_manifest_path}")
        return None

    _ = metadata

    try:
        payload = core_manifest_path.read_bytes()
    except OSError as exc:
        errors.append(f"failed to read core manifest file {core_manifest_path}: {exc}")
        return None

    return VerifiedAsset(
        lane="release-build-core",
        kind="core_manifest",
        label=core_manifest_path.name,
        path=core_manifest_path,
        sha256=_sha256_bytes(payload),
        bytes=len(payload),
    )


def _core_targets_by_name(
    core_manifest: Dict[str, Any], errors: List[str]
) -> Dict[str, Dict[str, str]]:
    targets = core_manifest.get("targets")
    if not isinstance(targets, list) or not targets:
        errors.append("core manifest has no targets")
        return {}

    entries: Dict[str, Dict[str, str]] = {}
    for index, item in enumerate(targets):
        if not isinstance(item, dict):
            errors.append(f"core manifest targets[{index}] must be object")
            continue
        target = item.get("target")
        archive_name = item.get("archive_name")
        archive_sha256 = item.get("archive_sha256")
        payload_dir = item.get("payload_dir")
        binary_name = item.get("binary_name")
        missing = [
            field
            for field, value in (
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
                f"core manifest targets[{index}] missing required string fields: "
                f"{', '.join(missing)}"
            )
            continue
        if target in entries:
            errors.append(f"core manifest has duplicate target entry: {target}")
            continue
        entries[str(target)] = {
            "archive_name": str(archive_name),
            "archive_sha256": str(archive_sha256).lower(),
            "payload_dir": str(payload_dir),
            "binary_name": str(binary_name),
        }
    return entries


def _resolve_install_metadata_path(
    metadata: Dict[str, Any],
    core_manifest_path: pathlib.Path,
    asset_root: pathlib.Path,
    errors: List[str],
) -> Optional[pathlib.Path]:
    version = metadata.get("version")
    if not isinstance(version, str) or not version:
        errors.append("version metadata is missing release version for install metadata lookup")
        return None
    install_metadata_name = f"release-install-metadata-{version}.json"
    candidates = [
        core_manifest_path.parent / install_metadata_name,
        asset_root / install_metadata_name,
    ]
    existing: List[pathlib.Path] = []
    seen = set()
    for candidate in candidates:
        if candidate.exists() and candidate.is_file():
            resolved = candidate.resolve()
            key = resolved.as_posix()
            if key not in seen:
                seen.add(key)
                existing.append(candidate)
    if not existing:
        errors.append(
            f"missing install metadata bundle: expected {install_metadata_name} in "
            f"{core_manifest_path.parent} or {asset_root}"
        )
        return None
    if len(existing) > 1:
        errors.append(
            f"ambiguous install metadata bundle resolution for {install_metadata_name}: "
            + ", ".join(path.as_posix() for path in existing)
        )
        return None
    return existing[0]


def _verify_install_metadata_bundle(
    metadata: Dict[str, Any],
    core_manifest: Dict[str, Any],
    core_manifest_path: pathlib.Path,
    asset_root: pathlib.Path,
    errors: List[str],
) -> Optional[VerifiedAsset]:
    path = _resolve_install_metadata_path(metadata, core_manifest_path, asset_root, errors)
    if path is None:
        return None

    bundle = _load_json(path, errors, "install metadata")
    if bundle is None:
        return None
    if bundle.get("schema") != INSTALL_METADATA_SCHEMA:
        errors.append(
            f"install metadata schema mismatch: expected {INSTALL_METADATA_SCHEMA!r}, "
            f"got {bundle.get('schema')!r}"
        )
        return None

    release = bundle.get("release")
    if not isinstance(release, dict):
        errors.append("install metadata missing release object")
        return None

    for key in ("channel", "version", "tag"):
        if release.get(key) != metadata.get(key):
            errors.append(
                f"install metadata release {key} mismatch: expected {metadata.get(key)!r}, "
                f"got {release.get(key)!r}"
            )

    if bool(release.get("prerelease", False)) != bool(metadata.get("prerelease", False)):
        errors.append(
            "install metadata release prerelease mismatch: "
            f"expected {bool(metadata.get('prerelease', False))}, "
            f"got {bool(release.get('prerelease', False))}"
        )

    artifact_stem = metadata.get("artifact_stem")
    if not isinstance(artifact_stem, str) or not artifact_stem:
        errors.append("version metadata missing artifact_stem")
        return None

    checksums_name = bundle.get("checksums_name")
    expected_checksums_name = f"{artifact_stem}-checksums.txt"
    if checksums_name != expected_checksums_name:
        errors.append(
            "install metadata checksums_name mismatch: "
            f"expected {expected_checksums_name!r}, got {checksums_name!r}"
        )

    core_manifest_name = bundle.get("core_manifest_name")
    if core_manifest_name != core_manifest_path.name:
        errors.append(
            "install metadata core_manifest_name mismatch: "
            f"expected {core_manifest_path.name!r}, got {core_manifest_name!r}"
        )

    core_targets = _core_targets_by_name(core_manifest, errors)
    bundle_targets = bundle.get("targets")
    if not isinstance(bundle_targets, list) or not bundle_targets:
        errors.append("install metadata targets must be a non-empty array")
        return None

    by_target: Dict[str, Dict[str, str]] = {}
    for index, item in enumerate(bundle_targets):
        if not isinstance(item, dict):
            errors.append(f"install metadata targets[{index}] must be object")
            continue
        target = item.get("target")
        archive_name = item.get("archive_name")
        archive_sha256 = item.get("archive_sha256")
        payload_dir = item.get("payload_dir")
        binary_name = item.get("binary_name")
        missing = [
            field
            for field, value in (
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
                f"install metadata targets[{index}] missing required string fields: "
                f"{', '.join(missing)}"
            )
            continue
        if target in by_target:
            errors.append(f"install metadata has duplicate target entry: {target}")
            continue
        by_target[str(target)] = {
            "archive_name": str(archive_name),
            "archive_sha256": str(archive_sha256).lower(),
            "payload_dir": str(payload_dir),
            "binary_name": str(binary_name),
        }

    if set(by_target) != set(core_targets):
        errors.append(
            "install metadata targets mismatch with core manifest: "
            f"bundle={sorted(by_target)} core={sorted(core_targets)}"
        )

    for target, expected in core_targets.items():
        observed = by_target.get(target)
        if observed is None:
            continue
        for field in ("archive_name", "archive_sha256", "payload_dir", "binary_name"):
            if observed[field] != expected[field]:
                errors.append(
                    f"install metadata target {target} {field} mismatch: "
                    f"expected {expected[field]!r}, got {observed[field]!r}"
                )

    if errors:
        return None

    return VerifiedAsset(
        lane="release-build-core",
        kind="install_metadata",
        label=path.name,
        path=path,
        sha256=_sha256_file(path),
        bytes=path.stat().st_size,
    )


def _collect_lane_entries(
    summaries: Iterable[Tuple[pathlib.Path, Dict[str, Any]]]
) -> Dict[str, Dict[str, Any]]:
    lane_data: Dict[str, Dict[str, Any]] = {
        "release-package-bridge": {"statuses": [], "artifacts": set(), "sources": []},
        "release-package-extension": {"statuses": [], "artifacts": set(), "sources": []},
    }
    for source, summary in summaries:
        lanes = summary.get("lanes", {})
        if not isinstance(lanes, dict):
            continue
        for lane_name in lane_data:
            lane = lanes.get(lane_name, {})
            if not isinstance(lane, dict):
                continue
            status = lane.get("status")
            artifacts = lane.get("artifacts", [])
            if status is not None:
                lane_data[lane_name]["statuses"].append(str(status))
            if isinstance(artifacts, list):
                for artifact in artifacts:
                    if artifact:
                        lane_data[lane_name]["artifacts"].add(str(artifact))
            lane_data[lane_name]["sources"].append(source.as_posix())
    return lane_data


def _merged_status(statuses: Iterable[str]) -> str:
    status_set = set(statuses)
    if "failed" in status_set:
        return "failed"
    if "built" in status_set:
        return "built"
    return "skipped"


def _verify_optional_lane_assets(
    summaries: List[Tuple[pathlib.Path, Dict[str, Any]]],
    asset_root: pathlib.Path,
    errors: List[str],
) -> Tuple[List[VerifiedAsset], Dict[str, Any]]:
    assets: List[VerifiedAsset] = []
    lane_data = _collect_lane_entries(summaries)
    lane_summary: Dict[str, Any] = {}

    for lane_name, payload in lane_data.items():
        statuses = payload["statuses"]
        merged = _merged_status(statuses)
        artifacts = sorted(payload["artifacts"])
        lane_summary[lane_name] = {
            "status": merged,
            "observed_statuses": statuses,
            "artifacts": artifacts,
            "sources": payload["sources"],
        }

        if merged == "built" and not artifacts:
            errors.append(
                f"lane-claim drift: {lane_name} resolved built status without artifact list"
            )

        if merged != "built":
            continue

        for artifact in artifacts:
            path = _resolve_asset(artifact, asset_root, errors, lane_name)
            if path is None:
                continue
            assets.append(
                VerifiedAsset(
                    lane=lane_name,
                    kind="optional_lane_artifact",
                    label=path.name,
                    path=path,
                    sha256=_sha256_file(path),
                    bytes=path.stat().st_size,
                )
            )

    return assets, lane_summary


def _build_checksums(assets: Iterable[VerifiedAsset], asset_root: pathlib.Path) -> str:
    rows = []
    for asset in assets:
        display = _display_path(asset.path, asset_root)
        rows.append((display, f"{asset.sha256}  {display}"))
    rows.sort(key=lambda item: item[0])
    return "\n".join(line for _, line in rows) + ("\n" if rows else "")


def main() -> int:
    args = _parse_args()
    errors: List[str] = []
    warnings: List[str] = []

    metadata_path = pathlib.Path(args.version_metadata)
    core_manifest_path = pathlib.Path(args.core_manifest)
    asset_root = pathlib.Path(args.asset_root)
    output_dir = pathlib.Path(args.output_dir)
    optional_summary_paths = [pathlib.Path(item) for item in args.optional_summary]

    metadata = _load_json(metadata_path, errors, "version metadata")
    core_manifest = _load_json(core_manifest_path, errors, "core manifest")
    summaries: List[Tuple[pathlib.Path, Dict[str, Any]]] = []

    if metadata is None or core_manifest is None:
        for err in errors:
            print(f"error: {err}", file=sys.stderr)
        return 2

    for summary_path in optional_summary_paths:
        summary = _load_json(summary_path, errors, "optional summary")
        if summary is None:
            continue
        if summary.get("schema") != "roger.release.optional_lanes.v1":
            errors.append(
                f"optional summary schema mismatch for {summary_path}: "
                f"{summary.get('schema')!r}"
            )
            continue
        release = summary.get("release", {})
        if not isinstance(release, dict):
            errors.append(f"optional summary {summary_path}: release must be object")
            continue
        for key in ("version", "channel", "tag"):
            if release.get(key) != metadata.get(key):
                errors.append(
                    f"optional summary {summary_path}: {key} mismatch "
                    f"(expected {metadata.get(key)!r}, got {release.get(key)!r})"
                )
        _validate_summary_claims(summary, summary_path, errors)
        summaries.append((summary_path, summary))

    if not asset_root.exists():
        errors.append(f"asset root does not exist: {asset_root}")

    core_assets = _verify_core_assets(metadata, core_manifest, asset_root, errors)
    core_manifest_asset = _verify_core_manifest_asset(
        metadata=metadata,
        core_manifest_path=core_manifest_path,
        errors=errors,
    )
    core_manifest_assets = [core_manifest_asset] if core_manifest_asset is not None else []
    install_metadata_asset = _verify_install_metadata_bundle(
        metadata=metadata,
        core_manifest=core_manifest,
        core_manifest_path=core_manifest_path,
        asset_root=asset_root,
        errors=errors,
    )
    install_assets = [install_metadata_asset] if install_metadata_asset is not None else []
    optional_assets, lane_summary = _verify_optional_lane_assets(summaries, asset_root, errors)
    all_assets = core_assets + core_manifest_assets + install_assets + optional_assets

    unsigned_targets = sorted(
        str(target.get("target"))
        for target in core_manifest.get("targets", [])
        if isinstance(target, dict) and target.get("target")
    )
    if unsigned_targets:
        warnings.append(
            "signing placeholders active: targets listed as unsigned until signing policy lane is wired"
        )

    publish_allowed = len(errors) == 0
    output_dir.mkdir(parents=True, exist_ok=True)

    checksums_text = _build_checksums(all_assets, asset_root)
    checksums_path = output_dir / "SHA256SUMS"
    checksums_path.write_text(checksums_text, encoding="utf-8")

    notes_path = output_dir / "release-notes-signing.md"
    notes_lines = [
        "# Signing Status",
        "",
        "Unsigned targets (placeholder surfaced explicitly in verify-assets lane):",
    ]
    notes_lines.extend(f"- `{target}` (unsigned_placeholder)" for target in unsigned_targets)
    notes_lines.append("")
    notes_path.write_text("\n".join(notes_lines), encoding="utf-8")

    manifest = {
        "schema": "roger.release-verify-assets.v1",
        "verified_at": dt.datetime.now(dt.timezone.utc).isoformat(),
        "release": {
            "channel": metadata.get("channel"),
            "version": metadata.get("version"),
            "tag": metadata.get("tag"),
            "prerelease": bool(metadata.get("prerelease", False)),
            "artifact_stem": metadata.get("artifact_stem"),
        },
        "inputs": {
            "version_metadata": metadata_path.as_posix(),
            "core_manifest": core_manifest_path.as_posix(),
            "asset_root": asset_root.as_posix(),
            "optional_summaries": [path.as_posix() for path, _ in summaries],
        },
        "core": {
            "built_target_count": len(core_assets),
            "manifest_target_count": len(core_manifest.get("targets", [])),
            "assets": [
                {
                    "lane": asset.lane,
                    "kind": asset.kind,
                    "label": asset.label,
                    "path": _display_path(asset.path, asset_root),
                    "sha256": asset.sha256,
                    "bytes": asset.bytes,
                }
                for asset in (core_assets + core_manifest_assets + install_assets)
            ],
        },
        "optional_lanes": {
            "lane_summary": lane_summary,
            "assets": [
                {
                    "lane": asset.lane,
                    "kind": asset.kind,
                    "label": asset.label,
                    "path": _display_path(asset.path, asset_root),
                    "sha256": asset.sha256,
                    "bytes": asset.bytes,
                }
                for asset in optional_assets
            ],
        },
        "signing": {
            "status": "unsigned_placeholder",
            "unsigned_targets": unsigned_targets,
            "notes_fragment": notes_path.name,
        },
        "checksums": {
            "path": checksums_path.name,
            "entries": len([line for line in checksums_text.splitlines() if line.strip()]),
        },
        "publish_gate": {
            "publish_allowed": publish_allowed,
            "failure_count": len(errors),
            "warning_count": len(warnings),
        },
        "failures": errors,
        "warnings": warnings,
    }

    manifest_path = output_dir / "release-asset-manifest.json"
    manifest_path.write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )

    if not publish_allowed:
        for err in errors:
            print(f"error: {err}", file=sys.stderr)
        print(
            f"verification failed; publish_allowed=false (report: {manifest_path})",
            file=sys.stderr,
        )
        return 1

    print(
        json.dumps(
            {
                "publish_allowed": True,
                "manifest": manifest_path.as_posix(),
                "checksums": checksums_path.as_posix(),
                "signing_notes": notes_path.as_posix(),
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
