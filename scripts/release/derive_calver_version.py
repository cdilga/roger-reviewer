#!/usr/bin/env python3
"""Derive Roger's canonical CalVer release metadata from git ref provenance."""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import pathlib
import re
import sys
import tomllib

STABLE_TAG_RE = re.compile(r"^v(?P<date>\d{4}\.\d{2}\.\d{2})$")
RC_TAG_RE = re.compile(r"^v(?P<date>\d{4}\.\d{2}\.\d{2})-rc\.(?P<rc>[1-9]\d*)$")
SHA_RE = re.compile(r"^[0-9a-fA-F]{7,40}$")


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Derive Roger CalVer tag/channel/asset metadata from git provenance."
    )
    parser.add_argument("--ref", help="Git ref (for example refs/tags/v2026.03.31)")
    parser.add_argument("--sha", help="Commit SHA associated with the build")
    parser.add_argument(
        "--today",
        help="UTC date used for nightly derivation in YYYY.MM.DD format",
    )
    parser.add_argument(
        "--workspace-version",
        help="Optional workspace semver override; defaults to Cargo.toml",
    )
    parser.add_argument(
        "--cargo-toml",
        default="Cargo.toml",
        help="Path to workspace Cargo.toml (default: Cargo.toml)",
    )
    parser.add_argument(
        "--pretty",
        action="store_true",
        help="Pretty-print JSON output",
    )
    return parser.parse_args()


def _require(value: str | None, env_name: str) -> str:
    if value:
        return value
    env_value = os.getenv(env_name)
    if env_value:
        return env_value
    raise ValueError(f"Missing required value: --{env_name.lower().replace('_', '-')} or {env_name}")


def _validate_sha(sha: str) -> str:
    if not SHA_RE.fullmatch(sha):
        raise ValueError(f"Invalid commit SHA: {sha!r}")
    return sha.lower()


def _parse_calver_date(value: str) -> str:
    dt.datetime.strptime(value, "%Y.%m.%d")
    return value


def _load_workspace_version(cargo_toml_path: pathlib.Path) -> str:
    with cargo_toml_path.open("rb") as handle:
        parsed = tomllib.load(handle)

    try:
        return str(parsed["workspace"]["package"]["version"])
    except KeyError as exc:
        raise ValueError(
            f"Could not locate [workspace.package].version in {cargo_toml_path}"
        ) from exc


def _derive(ref: str, sha: str, today: str, workspace_version: str) -> dict[str, object]:
    short_sha = sha[:12]

    channel: str
    version: str
    tag: str
    prerelease: bool
    promotable: bool
    provenance_source: str

    if ref.startswith("refs/tags/"):
        tag = ref[len("refs/tags/") :]

        stable_match = STABLE_TAG_RE.fullmatch(tag)
        if stable_match:
            date_component = _parse_calver_date(stable_match.group("date"))
            channel = "stable"
            version = date_component
            prerelease = False
            promotable = True
            provenance_source = "tag"
        else:
            rc_match = RC_TAG_RE.fullmatch(tag)
            if not rc_match:
                raise ValueError(
                    "Tag refs must match vYYYY.MM.DD or vYYYY.MM.DD-rc.N "
                    f"(got {tag!r})"
                )

            date_component = _parse_calver_date(rc_match.group("date"))
            rc_number = int(rc_match.group("rc"))
            channel = "rc"
            version = f"{date_component}-rc.{rc_number}"
            prerelease = True
            promotable = True
            provenance_source = "tag"
    else:
        date_component = _parse_calver_date(today)
        channel = "nightly"
        version = f"{date_component}-nightly.{short_sha}"
        tag = f"nightly-{date_component}-{short_sha}"
        prerelease = True
        promotable = False
        provenance_source = "derived-ref"

    artifact_stem = f"roger-reviewer-{version}"

    return {
        "channel": channel,
        "version": version,
        "tag": tag,
        "prerelease": prerelease,
        "promotable": promotable,
        "workspace_version": workspace_version,
        "release_name": f"Roger Reviewer {version}",
        "artifact_stem": artifact_stem,
        "artifacts": {
            "cli_archive": f"{artifact_stem}-cli.tar.gz",
            "bridge_archive": f"{artifact_stem}-bridge-host.tar.gz",
            "extension_archive": f"{artifact_stem}-extension.zip",
            "checksums": f"{artifact_stem}-checksums.txt",
            "manifest": f"{artifact_stem}-manifest.json",
        },
        "provenance": {
            "source_ref": ref,
            "source_sha": sha,
            "source_short_sha": short_sha,
            "date_basis": date_component,
            "version_source": provenance_source,
        },
    }


def main() -> int:
    args = _parse_args()

    try:
        ref = _require(args.ref, "GITHUB_REF")
        sha = _validate_sha(_require(args.sha, "GITHUB_SHA"))
        today = args.today or dt.datetime.now(dt.UTC).strftime("%Y.%m.%d")
        today = _parse_calver_date(today)
        workspace_version = args.workspace_version or _load_workspace_version(
            pathlib.Path(args.cargo_toml)
        )
        metadata = _derive(ref, sha, today, workspace_version)
    except ValueError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    json.dump(
        metadata,
        sys.stdout,
        indent=2 if args.pretty else None,
        sort_keys=True,
    )
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
