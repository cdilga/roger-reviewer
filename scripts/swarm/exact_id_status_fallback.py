#!/usr/bin/env python3
"""Canonical JSONL fallback for imported beads that br cannot mutate by exact ID.

The Roger workspace treats `.beads/issues.jsonl` as canonical when the SQLite
store needs to be rebuilt. This helper patches that canonical JSONL for a very
small exact-ID mutation subset so `scripts/swarm/br_pinned.sh` can recover from
known `br` import-only ID-resolution failures:

- `close <id> [--reason] [--session]`
- `reopen <id> [--session]`
- `update <id> --status <status> [--session]`
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import tempfile
from datetime import datetime, timezone
from typing import Any


class FallbackError(Exception):
    def __init__(self, message: str, exit_code: int = 2) -> None:
        super().__init__(message)
        self.exit_code = exit_code


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Patch canonical issues.jsonl for exact-ID fallback mutations.")
    parser.add_argument("--beads-dir", required=True)
    parser.add_argument("--command", choices=("close", "reopen", "update"), required=True)
    parser.add_argument("--id", required=True)
    parser.add_argument("--status")
    parser.add_argument("--reason", default="")
    parser.add_argument("--session", default="")
    parser.add_argument("--json", action="store_true")
    return parser.parse_args()


def now_utc_iso() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="microseconds").replace("+00:00", "Z")


def load_issues(jsonl_path: str) -> list[dict[str, Any]]:
    issues: list[dict[str, Any]] = []
    with open(jsonl_path, "r", encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            issues.append(json.loads(line))
    return issues


def write_issues(jsonl_path: str, issues: list[dict[str, Any]]) -> None:
    directory = os.path.dirname(jsonl_path) or "."
    fd, temp_path = tempfile.mkstemp(prefix=".issues.jsonl.", suffix=".tmp", dir=directory)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            for issue in issues:
                handle.write(json.dumps(issue, separators=(",", ":"), ensure_ascii=False))
                handle.write("\n")
        os.replace(temp_path, jsonl_path)
    except Exception:
        try:
            os.unlink(temp_path)
        except FileNotFoundError:
            pass
        raise


def find_issue(issues: list[dict[str, Any]], issue_id: str) -> dict[str, Any]:
    for issue in issues:
        if issue.get("id") == issue_id:
            return issue
    raise FallbackError(f"canonical_jsonl_exact_id could not find issue: {issue_id}", exit_code=3)


def status_by_id(issues: list[dict[str, Any]]) -> dict[str, str]:
    return {issue["id"]: issue.get("status", "open") for issue in issues}


def assert_closeable(issue: dict[str, Any], statuses: dict[str, str]) -> None:
    blocking_dependencies: list[str] = []
    for dependency in issue.get("dependencies", []) or []:
        depends_on_id = dependency.get("depends_on_id")
        if not depends_on_id:
            continue
        dependency_type = dependency.get("type", "blocks")
        if dependency_type != "blocks":
            continue
        dependency_status = statuses.get(depends_on_id)
        if dependency_status and dependency_status not in {"closed", "tombstone"}:
            blocking_dependencies.append(depends_on_id)
    if blocking_dependencies:
        joined = ", ".join(blocking_dependencies)
        raise FallbackError(
            f"canonical_jsonl_exact_id refuses to close blocked issue {issue['id']}: open dependencies: {joined}",
            exit_code=4,
        )


def clear_closed_fields(issue: dict[str, Any]) -> None:
    issue.pop("closed_at", None)
    issue.pop("close_reason", None)
    issue.pop("closed_by_session", None)


def apply_close(issue: dict[str, Any], *, reason: str, session: str) -> tuple[str, str]:
    timestamp = now_utc_iso()
    old_status = issue.get("status", "open")
    issue["status"] = "closed"
    issue["updated_at"] = timestamp
    issue["closed_at"] = timestamp
    issue["close_reason"] = reason
    if session:
        issue["closed_by_session"] = session
    else:
        issue.pop("closed_by_session", None)
    return old_status, "closed"


def apply_reopen(issue: dict[str, Any]) -> tuple[str, str]:
    timestamp = now_utc_iso()
    old_status = issue.get("status", "open")
    issue["status"] = "open"
    issue["updated_at"] = timestamp
    clear_closed_fields(issue)
    return old_status, "open"


def apply_update(issue: dict[str, Any], *, status: str, session: str, statuses: dict[str, str]) -> tuple[str, str]:
    if status == "closed":
        assert_closeable(issue, statuses)
    timestamp = now_utc_iso()
    old_status = issue.get("status", "open")
    issue["status"] = status
    issue["updated_at"] = timestamp
    if status == "closed":
        issue["closed_at"] = timestamp
        issue.setdefault("close_reason", "")
        if session:
            issue["closed_by_session"] = session
        else:
            issue.pop("closed_by_session", None)
    else:
        clear_closed_fields(issue)
    return old_status, status


def emit_result(
    *,
    command: str,
    issue: dict[str, Any],
    old_status: str,
    new_status: str,
    reason: str,
    as_json: bool,
) -> None:
    if as_json:
        print(
            json.dumps(
                {
                    "fallback": "canonical_jsonl_exact_id",
                    "command": command,
                    "id": issue["id"],
                    "title": issue.get("title", ""),
                    "old_status": old_status,
                    "new_status": new_status,
                    "close_reason": reason,
                },
                separators=(",", ":"),
            )
        )
        return

    title = issue.get("title", "")
    if command == "close":
        if reason:
            print(f"Closed {issue['id']}: {title} ({reason})")
        else:
            print(f"Closed {issue['id']}: {title}")
    elif command == "reopen":
        print(f"Reopened {issue['id']}: {title}")
    else:
        print(f"Updated {issue['id']}: {title}")
        print(f"  status: {old_status} → {new_status}")


def main() -> int:
    args = parse_args()
    jsonl_path = os.path.join(args.beads_dir, "issues.jsonl")
    if not os.path.isfile(jsonl_path):
        raise FallbackError(f"missing canonical JSONL: {jsonl_path}")

    issues = load_issues(jsonl_path)
    issue = find_issue(issues, args.id)
    statuses = status_by_id(issues)

    if args.command == "close":
        assert_closeable(issue, statuses)
        old_status, new_status = apply_close(issue, reason=args.reason, session=args.session)
    elif args.command == "reopen":
        old_status, new_status = apply_reopen(issue)
    else:
        if not args.status:
            raise FallbackError("update fallback requires --status")
        old_status, new_status = apply_update(
            issue, status=args.status, session=args.session, statuses=statuses
        )

    write_issues(jsonl_path, issues)
    emit_result(
        command=args.command,
        issue=issue,
        old_status=old_status,
        new_status=new_status,
        reason=args.reason,
        as_json=args.json,
    )
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except FallbackError as exc:
        print(str(exc), file=sys.stderr)
        sys.exit(exc.exit_code)
