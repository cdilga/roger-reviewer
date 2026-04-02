#!/usr/bin/env python3
"""Ingest failing GitHub Actions runs into deduplicated local beads.

This script turns remote failed workflow runs into local actionable work.
It focuses on release/validation workflows first and deduplicates by
workflow-path + ref + event so repeated failures update the same issue.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import pathlib
import subprocess
import sys
from dataclasses import dataclass
from typing import Any


WORKFLOW_PREFIXES = (
    ".github/workflows/release-",
    ".github/workflows/validation-",
)
DEFAULT_LABELS = "ci,github-actions,triage,ci-failure-intake"
DEFAULT_PARENT = "rr-aip"


@dataclass
class FailureRun:
    repo: str
    run_id: int
    run_url: str
    workflow_path: str
    workflow_name: str
    head_branch: str
    head_sha: str
    event: str
    status: str
    conclusion: str
    created_at: str
    updated_at: str
    summary: str

    @property
    def ref_label(self) -> str:
        if self.head_branch:
            return self.head_branch
        if self.head_sha:
            return self.head_sha[:12]
        return "detached"

    @property
    def dedupe_key(self) -> str:
        return f"{self.workflow_path}|{self.ref_label}|{self.event}"

    @property
    def issue_title(self) -> str:
        return f"CI failure intake: {self.workflow_path} [{self.ref_label}]"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--repo",
        required=True,
        help="Repository slug (owner/repo) for run ingestion identity.",
    )
    parser.add_argument(
        "--project-root",
        default=".",
        help="Repository/project root containing .beads (default: cwd).",
    )
    parser.add_argument(
        "--runs-json",
        help="Optional workflow-runs payload fixture path. When omitted, uses gh api live fetch.",
    )
    parser.add_argument(
        "--per-page",
        type=int,
        default=30,
        help="Max failed runs to fetch from GitHub when --runs-json is omitted.",
    )
    parser.add_argument(
        "--br-binary",
        default="scripts/swarm/br_pinned.sh",
        help="br command path (default: scripts/swarm/br_pinned.sh).",
    )
    parser.add_argument(
        "--parent-id",
        default=DEFAULT_PARENT,
        help="Optional parent issue id; pass 'none' to disable parent linking.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Compute actions and print summary without mutating beads.",
    )
    return parser.parse_args()


def _run(
    cmd: list[str],
    *,
    cwd: pathlib.Path,
    env: dict[str, str] | None = None,
    allow_failure: bool = False,
) -> subprocess.CompletedProcess[str]:
    proc = subprocess.run(
        cmd,
        cwd=str(cwd),
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if not allow_failure and proc.returncode != 0:
        raise RuntimeError(
            f"command failed ({proc.returncode}): {' '.join(cmd)}\n{proc.stderr.strip()}"
        )
    return proc


def _load_runs_fixture(path: pathlib.Path) -> list[dict[str, Any]]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if isinstance(payload, dict):
        runs = payload.get("workflow_runs", [])
    elif isinstance(payload, list):
        runs = payload
    else:
        raise ValueError("runs fixture must be an object or array")
    if not isinstance(runs, list):
        raise ValueError("workflow_runs must be an array")
    return [item for item in runs if isinstance(item, dict)]


def _load_runs_live(repo: str, per_page: int, cwd: pathlib.Path) -> list[dict[str, Any]]:
    gh_cmd = [
        "gh",
        "api",
        f"repos/{repo}/actions/runs?status=completed&conclusion=failure&per_page={per_page}",
    ]
    proc = _run(gh_cmd, cwd=cwd, allow_failure=True)
    if proc.returncode != 0:
        raise RuntimeError(
            "failed to fetch runs via gh api; ensure gh auth is configured "
            f"(stderr: {proc.stderr.strip()})"
        )
    payload = json.loads(proc.stdout)
    runs = payload.get("workflow_runs", [])
    if not isinstance(runs, list):
        raise ValueError("live workflow_runs payload missing array")
    return [item for item in runs if isinstance(item, dict)]


def _workflow_path(run: dict[str, Any]) -> str:
    raw = run.get("path")
    if isinstance(raw, str):
        return raw
    return ""


def _workflow_supported(path: str) -> bool:
    return any(path.startswith(prefix) for prefix in WORKFLOW_PREFIXES)


def _parse_run(repo: str, run: dict[str, Any]) -> FailureRun | None:
    workflow_path = _workflow_path(run)
    if not workflow_path or not _workflow_supported(workflow_path):
        return None

    run_id = run.get("id")
    run_url = run.get("html_url")
    if not isinstance(run_id, int) or not isinstance(run_url, str) or not run_url:
        return None

    status = str(run.get("status") or "")
    conclusion = str(run.get("conclusion") or "")
    if conclusion != "failure":
        return None

    workflow_name = str(run.get("name") or run.get("display_title") or "workflow")
    head_branch = str(run.get("head_branch") or "")
    head_sha = str(run.get("head_sha") or "")
    event = str(run.get("event") or "unknown")
    created_at = str(run.get("created_at") or "")
    updated_at = str(run.get("updated_at") or "")
    summary = str(run.get("display_title") or workflow_name)

    return FailureRun(
        repo=repo,
        run_id=run_id,
        run_url=run_url,
        workflow_path=workflow_path,
        workflow_name=workflow_name,
        head_branch=head_branch,
        head_sha=head_sha,
        event=event,
        status=status,
        conclusion=conclusion,
        created_at=created_at,
        updated_at=updated_at,
        summary=summary,
    )


def _choose_latest(entries: list[FailureRun]) -> list[FailureRun]:
    by_key: dict[str, FailureRun] = {}
    for entry in entries:
        previous = by_key.get(entry.dedupe_key)
        if previous is None:
            by_key[entry.dedupe_key] = entry
            continue
        prev_key = (previous.updated_at, previous.run_id)
        next_key = (entry.updated_at, entry.run_id)
        if next_key > prev_key:
            by_key[entry.dedupe_key] = entry
    return sorted(by_key.values(), key=lambda item: (item.workflow_path, item.ref_label))


def _parent_exists(parent_id: str, *, project_root: pathlib.Path, br_bin: str) -> bool:
    if not parent_id or parent_id.lower() == "none":
        return False
    proc = _run(
        [br_bin, "show", parent_id, "--json", "--no-daemon"],
        cwd=project_root,
        allow_failure=True,
    )
    return proc.returncode == 0


def _active_intake_issues(*, project_root: pathlib.Path, br_bin: str) -> dict[str, str]:
    issues_by_title: dict[str, str] = {}
    for status in ("open", "in_progress"):
        proc = _run(
            [br_bin, "list", "--status", status, "--json", "--no-daemon"],
            cwd=project_root,
            allow_failure=True,
        )
        if proc.returncode != 0:
            continue
        payload = json.loads(proc.stdout)
        issues = payload.get("issues", [])
        if not isinstance(issues, list):
            continue
        for issue in issues:
            if not isinstance(issue, dict):
                continue
            labels = issue.get("labels", [])
            title = issue.get("title")
            issue_id = issue.get("id")
            if (
                isinstance(labels, list)
                and "ci-failure-intake" in labels
                and isinstance(title, str)
                and isinstance(issue_id, str)
            ):
                issues_by_title[title] = issue_id
    return issues_by_title


def _build_description(run: FailureRun) -> str:
    return (
        "Auto-generated from failing GitHub Actions run ingestion.\n\n"
        f"- repo: {run.repo}\n"
        f"- workflow_path: {run.workflow_path}\n"
        f"- workflow_name: {run.workflow_name}\n"
        f"- ref: {run.ref_label}\n"
        f"- event: {run.event}\n\n"
        "Duplicate failures for this workflow/ref/event key update this same issue."
    )


def _build_notes(run: FailureRun) -> str:
    timestamp = dt.datetime.now(dt.timezone.utc).isoformat()
    lines = [
        "[ci-failure-intake]",
        f"ingested_at: {timestamp}",
        f"repo: {run.repo}",
        f"workflow_path: {run.workflow_path}",
        f"workflow_name: {run.workflow_name}",
        f"run_id: {run.run_id}",
        f"run_url: {run.run_url}",
        f"head_branch: {run.head_branch}",
        f"head_sha: {run.head_sha}",
        f"event: {run.event}",
        f"status: {run.status}",
        f"conclusion: {run.conclusion}",
        f"created_at: {run.created_at}",
        f"updated_at: {run.updated_at}",
        f"summary: {run.summary}",
        f"dedupe_key: {run.dedupe_key}",
    ]
    return "\n".join(lines)


def _create_issue(
    run: FailureRun,
    *,
    project_root: pathlib.Path,
    br_bin: str,
    parent_id: str | None,
    dry_run: bool,
) -> str:
    if dry_run:
        return "dry-run"

    cmd = [
        br_bin,
        "create",
        "--title",
        run.issue_title,
        "-t",
        "bug",
        "-p",
        "0",
        "--labels",
        DEFAULT_LABELS,
        "--description",
        _build_description(run),
        "--external-ref",
        run.run_url,
        "--silent",
        "--no-daemon",
    ]
    if parent_id:
        cmd.extend(["--parent", parent_id])
    proc = _run(cmd, cwd=project_root)
    issue_id = proc.stdout.strip()
    if not issue_id:
        raise RuntimeError(f"failed to parse created issue id for {run.issue_title}")
    _run(
        [
            br_bin,
            "update",
            issue_id,
            "--notes",
            _build_notes(run),
            "--external-ref",
            run.run_url,
            "--no-daemon",
        ],
        cwd=project_root,
    )
    return issue_id


def _update_issue(
    issue_id: str,
    run: FailureRun,
    *,
    project_root: pathlib.Path,
    br_bin: str,
    dry_run: bool,
) -> None:
    if dry_run:
        return
    _run(
        [
            br_bin,
            "update",
            issue_id,
            "--notes",
            _build_notes(run),
            "--external-ref",
            run.run_url,
            "--no-daemon",
        ],
        cwd=project_root,
    )


def main() -> int:
    args = parse_args()
    project_root = pathlib.Path(args.project_root).resolve()
    br_bin = args.br_binary
    if not pathlib.Path(br_bin).is_absolute():
        br_bin = str((project_root / br_bin).resolve())

    if args.runs_json:
        runs = _load_runs_fixture(pathlib.Path(args.runs_json))
        source = "fixture"
    else:
        runs = _load_runs_live(args.repo, args.per_page, project_root)
        source = "gh_api"

    parsed = []
    skipped = 0
    for run in runs:
        parsed_run = _parse_run(args.repo, run)
        if parsed_run is None:
            skipped += 1
            continue
        parsed.append(parsed_run)

    latest_runs = _choose_latest(parsed)
    parent_id: str | None = None
    if _parent_exists(args.parent_id, project_root=project_root, br_bin=br_bin):
        parent_id = args.parent_id

    existing = _active_intake_issues(project_root=project_root, br_bin=br_bin)
    created = []
    updated = []
    untouched = []
    for run in latest_runs:
        existing_id = existing.get(run.issue_title)
        if existing_id:
            _update_issue(
                existing_id,
                run,
                project_root=project_root,
                br_bin=br_bin,
                dry_run=args.dry_run,
            )
            updated.append(
                {
                    "issue_id": existing_id,
                    "title": run.issue_title,
                    "run_id": run.run_id,
                    "run_url": run.run_url,
                }
            )
            continue

        issue_id = _create_issue(
            run,
            project_root=project_root,
            br_bin=br_bin,
            parent_id=parent_id,
            dry_run=args.dry_run,
        )
        created.append(
            {
                "issue_id": issue_id,
                "title": run.issue_title,
                "run_id": run.run_id,
                "run_url": run.run_url,
            }
        )
        existing[run.issue_title] = issue_id

    if not latest_runs:
        untouched.append("no release/validation failures found")

    result = {
        "source": source,
        "repo": args.repo,
        "dry_run": args.dry_run,
        "parent_linked": parent_id is not None,
        "candidates_total": len(runs),
        "candidates_supported": len(parsed),
        "candidates_skipped": skipped,
        "ingested_keys": len(latest_runs),
        "created": created,
        "updated": updated,
        "untouched": untouched,
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:  # pragma: no cover - top-level fatal diagnostics
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(2)
