#!/usr/bin/env python3
"""Build machine-readable optional lane summary metadata for release workflows."""

from __future__ import annotations

import argparse
import datetime as dt
import json
import pathlib
import sys
from typing import List

ALLOWED_STATUS = {"built", "skipped", "failed"}


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build optional release lane summary metadata."
    )
    parser.add_argument(
        "--version-metadata",
        required=True,
        help="Path to release metadata JSON (from derive_calver_version.py).",
    )
    parser.add_argument(
        "--bridge-status",
        required=True,
        choices=sorted(ALLOWED_STATUS),
        help="Status for release-package-bridge lane.",
    )
    parser.add_argument(
        "--bridge-artifact",
        action="append",
        default=[],
        help="Bridge lane artifact name or path (repeatable).",
    )
    parser.add_argument(
        "--bridge-note",
        action="append",
        default=[],
        help="Bridge lane note (repeatable).",
    )
    parser.add_argument(
        "--extension-status",
        required=True,
        choices=sorted(ALLOWED_STATUS),
        help="Status for release-package-extension lane.",
    )
    parser.add_argument(
        "--extension-artifact",
        action="append",
        default=[],
        help="Extension lane artifact name or path (repeatable).",
    )
    parser.add_argument(
        "--extension-note",
        action="append",
        default=[],
        help="Extension lane note (repeatable).",
    )
    parser.add_argument(
        "--skills-status",
        default=None,
        choices=sorted(ALLOWED_STATUS),
        help=(
            "Status for the optional release-package-skills lane. Omit entirely "
            "to leave the skills lane out of the summary (warns-and-skips: a "
            "missing skills bundle never enters the support claims)."
        ),
    )
    parser.add_argument(
        "--skills-artifact",
        action="append",
        default=[],
        help="Skills lane artifact name or path (repeatable).",
    )
    parser.add_argument(
        "--skills-note",
        action="append",
        default=[],
        help="Skills lane note (repeatable).",
    )
    parser.add_argument(
        "--scope",
        default="workflow_lane",
        help="Summary scope label (default: workflow_lane).",
    )
    parser.add_argument(
        "--output",
        required=True,
        help="Path to output summary JSON.",
    )
    return parser.parse_args()


def _load_json(path: pathlib.Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def _lane_payload(status: str, artifacts: List[str], notes: List[str]) -> dict:
    artifact_items = sorted({str(item) for item in artifacts if item})
    note_items = [note for note in notes if note]
    return {
        "status": status,
        "built": status == "built",
        "artifacts": artifact_items,
        "notes": note_items,
    }


def _support_posture(bridge_status: str, extension_status: str) -> str:
    if bridge_status == "built" and extension_status == "built":
        return "core_plus_bridge_plus_extension"
    if bridge_status == "built":
        return "core_plus_bridge"
    return "core_only"


def main() -> int:
    args = _parse_args()
    metadata_path = pathlib.Path(args.version_metadata)
    output_path = pathlib.Path(args.output)

    try:
        release_metadata = _load_json(metadata_path)
    except OSError as exc:
        print(f"error: failed to read version metadata: {exc}", file=sys.stderr)
        return 2
    except json.JSONDecodeError as exc:
        print(f"error: invalid version metadata JSON: {exc}", file=sys.stderr)
        return 2

    channel = release_metadata.get("channel")
    version = release_metadata.get("version")
    tag = release_metadata.get("tag")
    artifact_stem = release_metadata.get("artifact_stem")
    prerelease = bool(release_metadata.get("prerelease", False))
    if not all([channel, version, tag, artifact_stem]):
        print(
            "error: version metadata is missing required fields "
            "(channel, version, tag, artifact_stem)",
            file=sys.stderr,
        )
        return 2

    posture = _support_posture(args.bridge_status, args.extension_status)
    shipped_optional_lanes = []
    if args.bridge_status == "built":
        shipped_optional_lanes.append("release-package-bridge")
    if args.extension_status == "built":
        shipped_optional_lanes.append("release-package-extension")
    if args.skills_status == "built":
        shipped_optional_lanes.append("release-package-skills")

    narrowed_claims = []
    if args.bridge_status != "built":
        narrowed_claims.append("bridge_registration_unshipped")
    if args.extension_status != "built":
        narrowed_claims.append("extension_sideload_unshipped")
    if args.extension_status == "built" and args.bridge_status != "built":
        narrowed_claims.append("browser_launch_claim_blocked_without_bridge")
    # The skills lane is fully optional: only narrow the claim when the lane is
    # explicitly present but did not build. An absent skills lane (status None)
    # leaves the support claims untouched.
    if args.skills_status is not None and args.skills_status != "built":
        narrowed_claims.append("skills_bundle_unshipped")

    warnings = []
    if args.extension_status == "built" and args.bridge_status != "built":
        warnings.append(
            "extension lane built without bridge lane; browser launch support claims stay narrowed"
        )
    if args.bridge_status == "failed" or args.extension_status == "failed":
        warnings.append(
            "one or more optional lanes failed; release-publish must not imply missing lane parity"
        )
    if args.skills_status == "failed":
        warnings.append(
            "skills lane failed; release-publish must not imply skills-bundle parity"
        )

    lanes = {
        "release-package-bridge": _lane_payload(
            args.bridge_status, args.bridge_artifact, args.bridge_note
        ),
        "release-package-extension": _lane_payload(
            args.extension_status,
            args.extension_artifact,
            args.extension_note,
        ),
    }
    # Skills are emitted only when the lane was actually exercised; an absent
    # skills lane stays absent so existing two-lane consumers are unaffected.
    if args.skills_status is not None:
        lanes["release-package-skills"] = _lane_payload(
            args.skills_status, args.skills_artifact, args.skills_note
        )

    summary = {
        "schema": "roger.release.optional_lanes.v1",
        "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
        "scope": args.scope,
        "release": {
            "channel": channel,
            "version": version,
            "tag": tag,
            "prerelease": prerelease,
            "artifact_stem": artifact_stem,
        },
        "lanes": lanes,
        "support_claims": {
            "posture": posture,
            "shipped_optional_lanes": shipped_optional_lanes,
            "narrowed_claims": narrowed_claims,
            "warnings": warnings,
        },
    }

    output_path.parent.mkdir(parents=True, exist_ok=True)
    with output_path.open("w", encoding="utf-8") as handle:
        json.dump(summary, handle, indent=2, sort_keys=True)
        handle.write("\n")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
