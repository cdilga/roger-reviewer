#!/usr/bin/env python3
"""Build an approval-gated GitHub release publication plan.

This script validates publication policy against verified release artifacts and
emits a deterministic plan + notes bundle for the release-publish workflow.

Exit codes:
- 0 on success
- 1 on policy/verification failure
- 2 on invalid invocation/input shape
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import pathlib
import re
import sys
from typing import Any, Dict, Iterable, List, Optional

STABLE_TAG_RE = re.compile(r"^v\d{4}\.\d{2}\.\d{2}$")
RC_TAG_RE = re.compile(r"^v\d{4}\.\d{2}\.\d{2}-rc\.[1-9]\d*$")
VERIFY_MANIFEST_SCHEMA = "roger.release-verify-assets.v1"
ALLOWED_OPTIONAL_LANE_STATUSES = {"built", "skipped"}
OPTIONAL_LANE_NAMES = ("release-package-bridge", "release-package-extension")
INSTALL_BOOTSTRAP_ASSETS = ("rr-install.sh", "rr-install.ps1")
GITHUB_RUN_URL_RE = re.compile(r"^https://github\.com/[^/]+/[^/]+/actions/runs/\d+$")
RELEASES_BASE_URL = "https://github.com/cdilga/roger-reviewer/releases"


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build a release-publish plan from verified release artifacts."
    )
    parser.add_argument(
        "--version-metadata",
        required=True,
        help="Path to release metadata JSON from derive_calver_version.py",
    )
    parser.add_argument(
        "--verified-manifest",
        required=True,
        help="Path to re-verified release-asset-manifest.json",
    )
    parser.add_argument(
        "--upstream-verified-manifest",
        help=(
            "Optional path to upstream release-verify-assets manifest; if provided, "
            "publish gating must match"
        ),
    )
    parser.add_argument(
        "--core-run-url",
        help="Optional html_url for the upstream release-build-core run",
    )
    parser.add_argument(
        "--verify-run-url",
        help="Optional html_url for the upstream release-verify-assets run",
    )
    parser.add_argument(
        "--bridge-run-url",
        help="Optional html_url for the upstream release-package-bridge run",
    )
    parser.add_argument(
        "--extension-run-url",
        help="Optional html_url for the upstream release-package-extension run",
    )
    parser.add_argument(
        "--asset-root",
        required=True,
        help="Root directory containing verified release artifacts",
    )
    parser.add_argument(
        "--checksums",
        required=True,
        help="Path to verified SHA256SUMS file",
    )
    parser.add_argument(
        "--signing-notes",
        required=True,
        help="Path to signing notes fragment emitted by verify_release_assets.py",
    )
    parser.add_argument(
        "--publish-mode",
        required=True,
        choices=("draft", "publish"),
        help="Explicit release behavior: draft rehearsal or stable publish",
    )
    parser.add_argument(
        "--operator-smoke-ack",
        action="store_true",
        help="Required acknowledgement for publish mode; ignored for draft mode",
    )
    parser.add_argument(
        "--output-dir",
        required=True,
        help="Directory where release-plan.json and release-notes.md are written",
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


def _is_approved_tag(tag: str) -> bool:
    return bool(STABLE_TAG_RE.fullmatch(tag) or RC_TAG_RE.fullmatch(tag))


def _approved_ref_from_metadata(metadata: Dict[str, Any], errors: List[str]) -> Optional[str]:
    provenance = metadata.get("provenance")
    if not isinstance(provenance, dict):
        errors.append("version metadata missing provenance object")
        return None

    source_ref = provenance.get("source_ref")
    if not isinstance(source_ref, str) or not source_ref:
        errors.append("version metadata provenance.source_ref is required")
        return None

    if not source_ref.startswith("refs/tags/"):
        errors.append(
            f"approved ref policy: source_ref must be a tag ref (got {source_ref!r})"
        )
        return None

    tag_from_ref = source_ref[len("refs/tags/") :]
    if not _is_approved_tag(tag_from_ref):
        errors.append(
            "approved ref policy: source_ref tag must match "
            "vYYYY.MM.DD or vYYYY.MM.DD-rc.N"
        )
        return None

    metadata_tag = metadata.get("tag")
    if metadata_tag != tag_from_ref:
        errors.append(
            "version metadata tag mismatch: "
            f"tag={metadata_tag!r} source_ref_tag={tag_from_ref!r}"
        )
        return None

    return source_ref


def _lane_posture(bridge_status: str, extension_status: str) -> str:
    if bridge_status == "built" and extension_status == "built":
        return "core_plus_bridge_plus_extension"
    if bridge_status == "built":
        return "core_plus_bridge"
    return "core_only"


def _lane_support_claims(lane_summary: Dict[str, Any]) -> Dict[str, Any]:
    bridge_status = str(
        lane_summary.get("release-package-bridge", {}).get("status", "skipped")
    )
    extension_status = str(
        lane_summary.get("release-package-extension", {}).get("status", "skipped")
    )

    shipped_optional_lanes: List[str] = []
    if bridge_status == "built":
        shipped_optional_lanes.append("release-package-bridge")
    if extension_status == "built":
        shipped_optional_lanes.append("release-package-extension")

    narrowed_claims: List[str] = []
    if bridge_status != "built":
        narrowed_claims.append("bridge_registration_unshipped")
    if extension_status != "built":
        narrowed_claims.append("extension_sideload_unshipped")
    if extension_status == "built" and bridge_status != "built":
        narrowed_claims.append("browser_launch_claim_blocked_without_bridge")

    return {
        "posture": _lane_posture(bridge_status, extension_status),
        "shipped_optional_lanes": shipped_optional_lanes,
        "narrowed_claims": narrowed_claims,
        "bridge_status": bridge_status,
        "extension_status": extension_status,
    }


def _expect_release_match(
    metadata: Dict[str, Any],
    manifest: Dict[str, Any],
    errors: List[str],
    label: str,
) -> Optional[Dict[str, Any]]:
    release = manifest.get("release")
    if not isinstance(release, dict):
        errors.append(f"{label}: missing release object")
        return None

    for key in ("channel", "version", "tag"):
        if release.get(key) != metadata.get(key):
            errors.append(
                f"{label}: release {key} mismatch "
                f"(expected {metadata.get(key)!r}, got {release.get(key)!r})"
            )

    if bool(release.get("prerelease", False)) != bool(metadata.get("prerelease", False)):
        errors.append(
            f"{label}: prerelease mismatch "
            f"(expected {bool(metadata.get('prerelease', False))}, "
            f"got {bool(release.get('prerelease', False))})"
        )

    publish_gate = manifest.get("publish_gate")
    if not isinstance(publish_gate, dict):
        errors.append(f"{label}: missing publish_gate object")
    elif publish_gate.get("publish_allowed") is not True:
        errors.append(f"{label}: publish_gate.publish_allowed must be true")

    return release


def _expect_verify_manifest_schema(
    manifest: Dict[str, Any], errors: List[str], label: str
) -> None:
    schema = manifest.get("schema")
    if schema != VERIFY_MANIFEST_SCHEMA:
        errors.append(
            f"{label}: schema must be {VERIFY_MANIFEST_SCHEMA!r} (got {schema!r})"
        )


def _collect_verified_assets(
    manifest: Dict[str, Any], asset_root: pathlib.Path, errors: List[str]
) -> List[pathlib.Path]:
    paths: List[pathlib.Path] = []

    def _ingest(entries: Iterable[Any], label: str) -> None:
        for item in entries:
            if not isinstance(item, dict):
                errors.append(f"{label}: expected asset object entry")
                continue
            raw_path = item.get("path")
            if not isinstance(raw_path, str) or not raw_path:
                errors.append(f"{label}: missing asset path")
                continue
            candidate = pathlib.Path(raw_path)
            resolved = candidate if candidate.is_absolute() else asset_root / candidate
            if not resolved.exists() or not resolved.is_file():
                errors.append(
                    f"{label}: missing verified asset {raw_path!r} under {asset_root}"
                )
                continue
            paths.append(resolved)

    core = manifest.get("core")
    if not isinstance(core, dict):
        errors.append("verified manifest missing core object")
    else:
        _ingest(core.get("assets", []), "core.assets")

    optional_lanes = manifest.get("optional_lanes")
    if not isinstance(optional_lanes, dict):
        errors.append("verified manifest missing optional_lanes object")
    else:
        _ingest(optional_lanes.get("assets", []), "optional_lanes.assets")

    deduped: List[pathlib.Path] = []
    seen = set()
    for path in paths:
        key = path.resolve().as_posix()
        if key not in seen:
            seen.add(key)
            deduped.append(path)
    return deduped


def _require_install_bootstrap_entries(
    manifest: Dict[str, Any], errors: List[str]
) -> None:
    core = manifest.get("core")
    if not isinstance(core, dict):
        errors.append("verified manifest missing core object for install bootstrap checks")
        return

    entries = core.get("assets")
    if not isinstance(entries, list):
        errors.append("verified manifest core.assets must be an array")
        return

    observed_names = set()
    for item in entries:
        if not isinstance(item, dict):
            continue
        if item.get("kind") != "install_bootstrap":
            continue

        raw_path = item.get("path")
        raw_label = item.get("label")

        if isinstance(raw_path, str) and raw_path:
            observed_names.add(pathlib.Path(raw_path).name)
            continue
        if isinstance(raw_label, str) and raw_label:
            observed_names.add(pathlib.Path(raw_label).name)
            continue

        errors.append(
            "verified manifest install_bootstrap entry missing path/label metadata"
        )

    for required in INSTALL_BOOTSTRAP_ASSETS:
        if required not in observed_names:
            errors.append(
                "verified manifest missing required install bootstrap asset entry: "
                f"{required}"
            )


def _normalize_run_url(
    label: str,
    value: Optional[str],
    errors: List[str],
    *,
    required: bool = False,
) -> Optional[str]:
    if value is None:
        if required:
            errors.append(f"{label}: required for upstream publication provenance")
        return None
    normalized = value.strip()
    if not normalized:
        if required:
            errors.append(f"{label}: required for upstream publication provenance")
        return None
    if not normalized.startswith("https://"):
        errors.append(f"{label}: must be an https URL (got {normalized!r})")
        return None
    if not GITHUB_RUN_URL_RE.fullmatch(normalized):
        errors.append(
            f"{label}: must match GitHub Actions run URL format "
            f"(https://github.com/<owner>/<repo>/actions/runs/<id>; got {normalized!r})"
        )
        return None
    return normalized


def _lane_status(manifest: Dict[str, Any], lane_name: str) -> str:
    optional = manifest.get("optional_lanes")
    if not isinstance(optional, dict):
        return "skipped"
    lane_summary = optional.get("lane_summary")
    if not isinstance(lane_summary, dict):
        return "skipped"
    lane = lane_summary.get(lane_name)
    if not isinstance(lane, dict):
        return "skipped"
    status = lane.get("status")
    if not isinstance(status, str) or not status:
        return "skipped"
    return status


def _validate_optional_lane_entries(
    manifest: Dict[str, Any], errors: List[str], label: str
) -> None:
    optional = manifest.get("optional_lanes")
    if not isinstance(optional, dict):
        errors.append(f"{label}: missing optional_lanes object")
        return

    lane_summary = optional.get("lane_summary")
    if not isinstance(lane_summary, dict):
        errors.append(f"{label}: missing optional_lanes.lane_summary object")
        return

    for lane_name in OPTIONAL_LANE_NAMES:
        lane = lane_summary.get(lane_name)
        if not isinstance(lane, dict):
            errors.append(
                f"{label}: optional_lanes.lane_summary missing lane {lane_name}"
            )
            continue
        status = lane.get("status")
        if not isinstance(status, str) or not status:
            errors.append(f"{label}: lane {lane_name} missing string status")


def _validate_optional_lane_status_domain(
    manifest: Dict[str, Any], errors: List[str], label: str
) -> None:
    for lane_name in OPTIONAL_LANE_NAMES:
        status = _lane_status(manifest, lane_name)
        if status not in ALLOWED_OPTIONAL_LANE_STATUSES:
            errors.append(
                f"{label}: optional lane {lane_name} has unsupported status {status!r}"
            )


def _enforce_upstream_optional_lane_parity(
    verified_manifest: Dict[str, Any],
    upstream_manifest: Dict[str, Any],
    errors: List[str],
) -> Dict[str, bool]:
    lane_requirements = {
        "release-package-bridge": False,
        "release-package-extension": False,
    }

    for lane_name in lane_requirements:
        upstream_status = _lane_status(upstream_manifest, lane_name)
        verified_status = _lane_status(verified_manifest, lane_name)

        if upstream_status == "built":
            lane_requirements[lane_name] = True
            if verified_status != "built":
                errors.append(
                    f"optional lane parity: upstream {lane_name} is built but reverified status is {verified_status!r}"
                )
        elif upstream_status == "skipped" and verified_status == "built":
            errors.append(
                f"optional lane parity: upstream {lane_name} is skipped but reverified status is 'built'"
            )

    return lane_requirements


def _render_notes(
    metadata: Dict[str, Any],
    publish_mode: str,
    lane_summary: Dict[str, Any],
    support_claims: Dict[str, Any],
    checksums_name: str,
    signing_notes_name: str,
    verified_manifest_name: str,
    upstream_runs: Dict[str, Optional[str]],
) -> str:
    release_name = str(metadata.get("release_name") or f"Roger Reviewer {metadata.get('version')}")
    channel = str(metadata.get("channel"))
    tag = str(metadata.get("tag"))
    version = str(metadata.get("version"))
    installer_latest_sh = f"{RELEASES_BASE_URL}/latest/download/rr-install.sh"
    installer_latest_ps1 = f"{RELEASES_BASE_URL}/latest/download/rr-install.ps1"
    installer_tag_sh = f"{RELEASES_BASE_URL}/download/{tag}/rr-install.sh"
    installer_tag_ps1 = f"{RELEASES_BASE_URL}/download/{tag}/rr-install.ps1"

    lines = [
        f"# {release_name}",
        "",
        f"- Release tag: `{tag}`",
        f"- Channel: `{channel}`",
        f"- Publication mode: `{publish_mode}`",
        "",
        "## Artifact Lanes",
        "",
    ]

    for lane_name in ("release-package-bridge", "release-package-extension"):
        lane = lane_summary.get(lane_name, {})
        status = lane.get("status", "skipped")
        artifacts = lane.get("artifacts", [])
        lines.append(f"- `{lane_name}`: `{status}`")
        if isinstance(artifacts, list) and artifacts:
            for artifact in artifacts:
                lines.append(f"  - artifact: `{artifact}`")

    lines.extend(
        [
            "",
            "## Support Claims",
            "",
            f"- Posture: `{support_claims['posture']}`",
        ]
    )

    shipped = support_claims.get("shipped_optional_lanes", [])
    if shipped:
        lines.append(
            "- Shipped optional lanes: " + ", ".join(f"`{lane}`" for lane in shipped)
        )
    else:
        lines.append("- Shipped optional lanes: none")

    narrowed = support_claims.get("narrowed_claims", [])
    if narrowed:
        lines.append(
            "- Narrowed claims: " + ", ".join(f"`{claim}`" for claim in narrowed)
        )
    else:
        lines.append("- Narrowed claims: none")

    lines.extend(
        [
            "",
            "## Upstream Workflow Evidence",
            "",
            f"- Core build run: `{upstream_runs.get('core') or 'not-recorded'}`",
            f"- Verify-assets run: `{upstream_runs.get('verify') or 'not-recorded'}`",
            f"- Bridge package run: `{upstream_runs.get('bridge') or 'not-recorded'}`",
            f"- Extension package run: `{upstream_runs.get('extension') or 'not-recorded'}`",
            "",
            "## Verification Artifacts",
            "",
            f"- Checksums: `{checksums_name}`",
            f"- Verified asset manifest: `{verified_manifest_name}`",
            f"- Signing notes: `{signing_notes_name}`",
            "",
            "## Install Commands (CLI Base Product)",
            "",
            "- Stable/latest (Unix):",
            f"  - `curl -fsSL {installer_latest_sh} | bash`",
            "- Stable/latest (PowerShell):",
            "  - `& ([scriptblock]::Create((Invoke-WebRequest -UseBasicParsing "
            + f"'{installer_latest_ps1}').Content))`",
            f"- Pinned `{tag}` (Unix):",
            f"  - `curl -fsSL {installer_tag_sh} | bash -s -- --version {version}`",
            f"- Pinned `{tag}` (PowerShell):",
            "  - `& ([scriptblock]::Create((Invoke-WebRequest -UseBasicParsing "
            + f"'{installer_tag_ps1}').Content)) -Version '{version}'`",
            "",
            "## Optional Follow-On (Separate From Base Install)",
            "",
            "- Bridge and extension lanes are optional packaging surfaces and are not required for the base CLI install.",
            "- Use the release optional-lane artifacts only when you need browser launch/helper integration.",
            "",
            "## Manual Smoke",
            "",
            "- Release owner must complete documented release-smoke before publish mode is used.",
            "- Required checklist: `docs/release-publish-operator-smoke.md`",
            "",
            f"Release `{version}` prepared by Roger release-publish plan.",
            "",
        ]
    )

    return "\n".join(lines)


def main() -> int:
    args = _parse_args()
    errors: List[str] = []

    metadata_path = pathlib.Path(args.version_metadata)
    verified_manifest_path = pathlib.Path(args.verified_manifest)
    upstream_verified_manifest_path = (
        pathlib.Path(args.upstream_verified_manifest)
        if args.upstream_verified_manifest
        else None
    )
    checksums_path = pathlib.Path(args.checksums)
    signing_notes_path = pathlib.Path(args.signing_notes)
    asset_root = pathlib.Path(args.asset_root)
    output_dir = pathlib.Path(args.output_dir)

    metadata = _load_json(metadata_path, errors, "version metadata")
    verified_manifest = _load_json(verified_manifest_path, errors, "verified manifest")
    upstream_manifest = None
    if upstream_verified_manifest_path is not None:
        upstream_manifest = _load_json(
            upstream_verified_manifest_path,
            errors,
            "upstream verified manifest",
        )

    if metadata is None or verified_manifest is None:
        for err in errors:
            print(f"error: {err}", file=sys.stderr)
        return 2

    if not checksums_path.exists() or not checksums_path.is_file():
        errors.append(f"missing checksums file: {checksums_path}")
    if not signing_notes_path.exists() or not signing_notes_path.is_file():
        errors.append(f"missing signing notes file: {signing_notes_path}")
    if not asset_root.exists() or not asset_root.is_dir():
        errors.append(f"asset root is missing or not a directory: {asset_root}")

    approved_ref = _approved_ref_from_metadata(metadata, errors)
    _expect_verify_manifest_schema(verified_manifest, errors, "verified manifest")
    _validate_optional_lane_entries(verified_manifest, errors, "verified manifest")
    _validate_optional_lane_status_domain(verified_manifest, errors, "verified manifest")
    verified_release = _expect_release_match(metadata, verified_manifest, errors, "verified manifest")

    if upstream_manifest is not None:
        _expect_verify_manifest_schema(
            upstream_manifest, errors, "upstream verified manifest"
        )
        _validate_optional_lane_entries(
            upstream_manifest, errors, "upstream verified manifest"
        )
        _validate_optional_lane_status_domain(
            upstream_manifest, errors, "upstream verified manifest"
        )
        _expect_release_match(metadata, upstream_manifest, errors, "upstream verified manifest")
        lane_requirements = _enforce_upstream_optional_lane_parity(
            verified_manifest,
            upstream_manifest,
            errors,
        )
        require_bridge_run_url = lane_requirements["release-package-bridge"]
        require_extension_run_url = lane_requirements["release-package-extension"]
    else:
        require_bridge_run_url = False
        require_extension_run_url = False

    upstream_runs = {
        "core": _normalize_run_url(
            "core_run_url",
            args.core_run_url,
            errors,
            required=upstream_manifest is not None,
        ),
        "verify": _normalize_run_url(
            "verify_run_url",
            args.verify_run_url,
            errors,
            required=upstream_manifest is not None,
        ),
        "bridge": _normalize_run_url(
            "bridge_run_url",
            args.bridge_run_url,
            errors,
            required=require_bridge_run_url,
        ),
        "extension": _normalize_run_url(
            "extension_run_url",
            args.extension_run_url,
            errors,
            required=require_extension_run_url,
        ),
    }
    if upstream_manifest is not None:
        if not require_bridge_run_url and upstream_runs["bridge"] is not None:
            errors.append(
                "bridge_run_url must be omitted when upstream verify data marks "
                "release-package-bridge as skipped"
            )
        if not require_extension_run_url and upstream_runs["extension"] is not None:
            errors.append(
                "extension_run_url must be omitted when upstream verify data marks "
                "release-package-extension as skipped"
            )

    channel = str(metadata.get("channel", ""))
    prerelease = bool(metadata.get("prerelease", False))
    tag = str(metadata.get("tag", ""))

    if channel not in {"stable", "rc"}:
        errors.append(
            f"publication policy: channel must be stable or rc (got {channel!r})"
        )

    if not _is_approved_tag(tag):
        errors.append(
            "publication policy: metadata tag must match vYYYY.MM.DD or vYYYY.MM.DD-rc.N"
        )

    if args.publish_mode == "publish":
        if channel != "stable" or prerelease:
            errors.append(
                "publish mode requires stable non-prerelease metadata "
                f"(channel={channel!r}, prerelease={prerelease})"
            )
        if not args.operator_smoke_ack:
            errors.append(
                "publish mode requires explicit --operator-smoke-ack confirmation"
            )

    lane_summary_obj = (
        verified_manifest.get("optional_lanes", {}).get("lane_summary", {})
        if isinstance(verified_manifest.get("optional_lanes"), dict)
        else {}
    )
    if not isinstance(lane_summary_obj, dict):
        errors.append("verified manifest optional_lanes.lane_summary must be object")
        lane_summary_obj = {}

    support_claims = _lane_support_claims(lane_summary_obj)
    _require_install_bootstrap_entries(verified_manifest, errors)
    verified_assets = _collect_verified_assets(verified_manifest, asset_root, errors)

    if errors:
        for err in errors:
            print(f"error: {err}", file=sys.stderr)
        return 1

    output_dir.mkdir(parents=True, exist_ok=True)

    notes_path = output_dir / "release-notes.md"
    notes_text = _render_notes(
        metadata=metadata,
        publish_mode=args.publish_mode,
        lane_summary=lane_summary_obj,
        support_claims=support_claims,
        checksums_name=checksums_path.name,
        signing_notes_name=signing_notes_path.name,
        verified_manifest_name=verified_manifest_path.name,
        upstream_runs=upstream_runs,
    )
    notes_path.write_text(notes_text, encoding="utf-8")

    draft = args.publish_mode == "draft"
    asset_paths: List[str] = [path.resolve().as_posix() for path in verified_assets]
    asset_paths.extend(
        [
            checksums_path.resolve().as_posix(),
            verified_manifest_path.resolve().as_posix(),
            signing_notes_path.resolve().as_posix(),
        ]
    )

    deduped_assets: List[str] = []
    seen = set()
    for item in asset_paths:
        if item not in seen:
            seen.add(item)
            deduped_assets.append(item)

    plan = {
        "schema": "roger.release-publish-plan.v1",
        "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
        "release": {
            "tag": metadata["tag"],
            "version": metadata["version"],
            "channel": metadata["channel"],
            "name": metadata.get("release_name") or f"Roger Reviewer {metadata['version']}",
            "prerelease": bool(metadata.get("prerelease", False)),
            "draft": draft,
            "publish_mode": args.publish_mode,
            "approved_ref": approved_ref,
            "operator_smoke_ack": bool(args.operator_smoke_ack),
        },
        "verification": {
            "publish_allowed": True,
            "verified_manifest": verified_manifest_path.resolve().as_posix(),
            "upstream_verified_manifest": (
                upstream_verified_manifest_path.resolve().as_posix()
                if upstream_verified_manifest_path is not None
                else None
            ),
            "checksums": checksums_path.resolve().as_posix(),
            "signing_notes": signing_notes_path.resolve().as_posix(),
            "upstream_runs": upstream_runs,
        },
        "support_claims": {
            **support_claims,
            "lane_summary": lane_summary_obj,
        },
        "assets": deduped_assets,
        "notes_path": notes_path.resolve().as_posix(),
    }

    plan_path = output_dir / "release-plan.json"
    plan_path.write_text(json.dumps(plan, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    print(
        json.dumps(
            {
                "plan": plan_path.resolve().as_posix(),
                "notes": notes_path.resolve().as_posix(),
                "assets": len(deduped_assets),
                "publish_mode": args.publish_mode,
                "channel": metadata["channel"],
                "tag": metadata["tag"],
            },
            sort_keys=True,
        )
    )

    _ = verified_release
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
