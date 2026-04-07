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
import pathlib
import re
import subprocess
import sys
import unicodedata
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from typing import Any


DEFAULT_WORKFLOW_PREFIXES = (
    ".github/workflows/release-",
    ".github/workflows/validation-",
)
DEFAULT_LABELS = "ci,github-actions,triage,ci-failure-intake"
DEFAULT_PARENT = "rr-aip"
DEFAULT_INSTRUCTIONS = (
    "Required follow-up instructions:\n"
    "1. claim the failure or link it to an existing open repair bead within 15 minutes\n"
    "2. include the GitHub Actions run URL, workflow path, ref, and local owner bead in notes\n"
    "3. announce ownership or reuse on Agent Mail topic `ci-failure`\n"
    "4. dedupe against existing open/in-progress repair beads before creating a new one\n"
    "5. preserve remote closeout evidence with scripts/swarm/check_ci_closeout_evidence.sh"
)
DEFAULT_AGENT_MAIL_API = "http://127.0.0.1:8765/api/"
DEFAULT_AGENT_MAIL_TOKEN_PATHS = (
    pathlib.Path("/Users/cdilga/Documents/dev/mcp_agent_mail/codex.mcp.json"),
    pathlib.Path(
        "/Users/cdilga/Documents/dev/roger-reviewer/mcp_agent_mail/codex.mcp.json"
    ),
)
DEFAULT_AGENT_MAIL_SENDER = "BlueHarbor"
DEFAULT_AGENT_MAIL_PROGRAM = "ci-failure-watch"
DEFAULT_AGENT_MAIL_MODEL = "deterministic-script"
DEFAULT_AGENT_MAIL_TASK = "CI failure intake watcher"
DEFAULT_AGENT_MAIL_TOPIC = "ci-failure"
DEFAULT_AGENT_MAIL_IMPORTANCE = "high"
DEFAULT_AGENT_MAIL_ACTIVE_WITHIN_MINUTES = 180
DEFAULT_AGENT_MAIL_MAX_RECIPIENTS = 12
MAX_FIELD_LENGTH = 200
SUSPICIOUS_FIELD_PATTERNS = (
    "```",
    "<script",
    "<!--",
    "ignore previous",
    "ignore all previous",
    "system prompt",
    "assistant:",
    "user:",
)


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
    sanitization_reasons: tuple[str, ...]
    quarantined_fields: tuple[str, ...]
    external_ref_url: str | None

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


@dataclass(frozen=True)
class IntakeConfig:
    parent_id: str
    labels_csv: str
    workflow_prefixes: tuple[str, ...]
    instructions_md: str
    agent_mail: "AgentMailConfig"


@dataclass(frozen=True)
class AgentMailConfig:
    enabled: bool
    am_binary: str
    api_url: str
    token_path: str | None
    sender_name: str
    sender_program: str
    sender_model: str
    sender_task: str
    topic: str
    importance: str
    ack_required: bool
    active_within_minutes: int
    max_recipients: int
    recipients: tuple[str, ...]


def default_config() -> IntakeConfig:
    return IntakeConfig(
        parent_id=DEFAULT_PARENT,
        labels_csv=DEFAULT_LABELS,
        workflow_prefixes=DEFAULT_WORKFLOW_PREFIXES,
        instructions_md=DEFAULT_INSTRUCTIONS,
        agent_mail=AgentMailConfig(
            enabled=False,
            am_binary="am",
            api_url=DEFAULT_AGENT_MAIL_API,
            token_path=None,
            sender_name=DEFAULT_AGENT_MAIL_SENDER,
            sender_program=DEFAULT_AGENT_MAIL_PROGRAM,
            sender_model=DEFAULT_AGENT_MAIL_MODEL,
            sender_task=DEFAULT_AGENT_MAIL_TASK,
            topic=DEFAULT_AGENT_MAIL_TOPIC,
            importance=DEFAULT_AGENT_MAIL_IMPORTANCE,
            ack_required=False,
            active_within_minutes=DEFAULT_AGENT_MAIL_ACTIVE_WITHIN_MINUTES,
            max_recipients=DEFAULT_AGENT_MAIL_MAX_RECIPIENTS,
            recipients=(),
        ),
    )


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
        default=None,
        help="Optional parent issue id; pass 'none' to disable parent linking.",
    )
    parser.add_argument(
        "--config",
        help="Optional JSON config with parent_id, labels, workflow_prefixes, and instructions_md.",
    )
    parser.add_argument(
        "--state-file",
        help="Optional JSON state file used to avoid rewriting unchanged failed-run intake.",
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


def _sanitize_scalar(
    raw: Any, *, field_name: str, max_len: int = MAX_FIELD_LENGTH
) -> tuple[str, tuple[str, ...], bool]:
    text = str(raw or "")
    reasons: list[str] = []
    normalized = unicodedata.normalize("NFKC", text)
    normalized = normalized.replace("\r", " ").replace("\n", " ").replace("\t", " ")
    cleaned = "".join(
        ch
        for ch in normalized
        if ch.isprintable() or ch == " "
    )
    collapsed = re.sub(r"\s+", " ", cleaned).strip()
    if collapsed != text.strip():
        reasons.append(f"{field_name}:normalized")
    lowered = collapsed.lower()
    quarantined = any(marker in lowered for marker in SUSPICIOUS_FIELD_PATTERNS)
    if quarantined:
        reasons.append(f"{field_name}:quarantined")
        return f"[quarantined {field_name}]", tuple(reasons), True
    if len(collapsed) > max_len:
        collapsed = collapsed[: max_len - 1].rstrip() + "…"
        reasons.append(f"{field_name}:truncated")
    return collapsed, tuple(reasons), False


def _sanitize_url(raw: Any) -> tuple[str | None, tuple[str, ...]]:
    value = str(raw or "").strip()
    if not value:
        return None, ()
    parsed = urllib.parse.urlparse(value)
    if (
        parsed.scheme == "https"
        and parsed.netloc == "github.com"
        and parsed.path.startswith("/")
    ):
        return value, ()
    return None, ("run_url:quarantined",)


def _workflow_supported(path: str, prefixes: tuple[str, ...]) -> bool:
    return any(path.startswith(prefix) for prefix in prefixes)


def _parse_run(repo: str, run: dict[str, Any], prefixes: tuple[str, ...]) -> FailureRun | None:
    workflow_path_raw = _workflow_path(run)
    workflow_path, workflow_path_reasons, _ = _sanitize_scalar(
        workflow_path_raw, field_name="workflow_path"
    )
    if not workflow_path or not _workflow_supported(workflow_path, prefixes):
        return None

    run_id = run.get("id")
    run_url, run_url_reasons = _sanitize_url(run.get("html_url"))
    if not isinstance(run_id, int) or run_url is None:
        return None

    status, status_reasons, _ = _sanitize_scalar(run.get("status"), field_name="status")
    conclusion, conclusion_reasons, _ = _sanitize_scalar(
        run.get("conclusion"), field_name="conclusion"
    )
    if conclusion != "failure":
        return None

    workflow_name, workflow_name_reasons, workflow_name_quarantined = _sanitize_scalar(
        run.get("name") or run.get("display_title") or "workflow",
        field_name="workflow_name",
    )
    head_branch, head_branch_reasons, head_branch_quarantined = _sanitize_scalar(
        run.get("head_branch"), field_name="head_branch"
    )
    head_sha, head_sha_reasons, _ = _sanitize_scalar(
        run.get("head_sha"), field_name="head_sha"
    )
    event, event_reasons, _ = _sanitize_scalar(run.get("event") or "unknown", field_name="event")
    created_at, created_at_reasons, _ = _sanitize_scalar(
        run.get("created_at"), field_name="created_at"
    )
    updated_at, updated_at_reasons, _ = _sanitize_scalar(
        run.get("updated_at"), field_name="updated_at"
    )
    summary, summary_reasons, summary_quarantined = _sanitize_scalar(
        run.get("display_title") or workflow_name,
        field_name="summary",
    )
    reasons = tuple(
        reason
        for reason in (
            *workflow_path_reasons,
            *workflow_name_reasons,
            *head_branch_reasons,
            *head_sha_reasons,
            *event_reasons,
            *status_reasons,
            *conclusion_reasons,
            *created_at_reasons,
            *updated_at_reasons,
            *summary_reasons,
            *run_url_reasons,
        )
    )
    quarantined_fields = tuple(
        field
        for field, active in (
            ("workflow_name", workflow_name_quarantined),
            ("head_branch", head_branch_quarantined),
            ("summary", summary_quarantined),
        )
        if active
    )

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
        sanitization_reasons=reasons,
        quarantined_fields=quarantined_fields,
        external_ref_url=run_url,
    )


def _load_config(path: pathlib.Path | None) -> IntakeConfig:
    config = default_config()
    if path is None:
        return config

    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise ValueError("config must be a JSON object")

    parent_id = payload.get("parent_id", config.parent_id)
    labels = payload.get("labels")
    workflow_prefixes = payload.get("workflow_prefixes")
    instructions_md = payload.get("instructions_md", config.instructions_md)
    agent_mail_payload = payload.get("agent_mail", {})

    labels_csv = config.labels_csv
    if isinstance(labels, list):
        labels_csv = ",".join(str(item).strip() for item in labels if str(item).strip())
    elif isinstance(labels, str) and labels.strip():
        labels_csv = labels.strip()

    prefixes = config.workflow_prefixes
    if isinstance(workflow_prefixes, list):
        prefixes = tuple(
            str(item).strip() for item in workflow_prefixes if str(item).strip()
        )
        if not prefixes:
            prefixes = config.workflow_prefixes

    if not isinstance(instructions_md, str) or not instructions_md.strip():
        instructions_md = config.instructions_md

    if not isinstance(parent_id, str) or not parent_id.strip():
        parent_id = "none"

    agent_mail = config.agent_mail
    if isinstance(agent_mail_payload, dict):
        recipients = agent_mail_payload.get("recipients", [])
        token_path = agent_mail_payload.get("token_path", agent_mail.token_path)
        agent_mail = AgentMailConfig(
            enabled=bool(agent_mail_payload.get("enabled", agent_mail.enabled)),
            am_binary=str(agent_mail_payload.get("am_binary", agent_mail.am_binary) or agent_mail.am_binary),
            api_url=str(agent_mail_payload.get("api_url", agent_mail.api_url) or agent_mail.api_url),
            token_path=str(token_path).strip() if isinstance(token_path, str) and token_path.strip() else None,
            sender_name=str(agent_mail_payload.get("sender_name", agent_mail.sender_name) or agent_mail.sender_name),
            sender_program=str(agent_mail_payload.get("sender_program", agent_mail.sender_program) or agent_mail.sender_program),
            sender_model=str(agent_mail_payload.get("sender_model", agent_mail.sender_model) or agent_mail.sender_model),
            sender_task=str(agent_mail_payload.get("sender_task", agent_mail.sender_task) or agent_mail.sender_task),
            topic=str(agent_mail_payload.get("topic", agent_mail.topic) or agent_mail.topic),
            importance=str(agent_mail_payload.get("importance", agent_mail.importance) or agent_mail.importance),
            ack_required=bool(agent_mail_payload.get("ack_required", agent_mail.ack_required)),
            active_within_minutes=max(
                1,
                int(agent_mail_payload.get("active_within_minutes", agent_mail.active_within_minutes)),
            ),
            max_recipients=max(
                1,
                int(agent_mail_payload.get("max_recipients", agent_mail.max_recipients)),
            ),
            recipients=tuple(
                str(item).strip() for item in recipients if str(item).strip()
            )
            if isinstance(recipients, list)
            else agent_mail.recipients,
        )

    return IntakeConfig(
        parent_id=parent_id,
        labels_csv=labels_csv,
        workflow_prefixes=prefixes,
        instructions_md=instructions_md.strip(),
        agent_mail=agent_mail,
    )


def _load_state(path: pathlib.Path | None) -> dict[str, dict[str, Any]]:
    if path is None or not path.exists():
        return {}
    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise ValueError("state file must contain a JSON object")
    state: dict[str, dict[str, Any]] = {}
    for key, value in payload.items():
        if isinstance(key, str) and isinstance(value, dict):
            state[key] = value
    return state


def _save_state(path: pathlib.Path | None, state: dict[str, dict[str, Any]]) -> None:
    if path is None:
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(state, indent=2, sort_keys=True) + "\n", encoding="utf-8")


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


def _build_description(run: FailureRun, instructions_md: str) -> str:
    description = (
        "Auto-generated from failing GitHub Actions run ingestion.\n\n"
        f"- repo: {run.repo}\n"
        f"- workflow_path: {run.workflow_path}\n"
        f"- workflow_name: {run.workflow_name}\n"
        f"- ref: {run.ref_label}\n"
        f"- event: {run.event}\n\n"
        "Duplicate failures for this workflow/ref/event key update this same issue."
    )
    if run.quarantined_fields:
        description = (
            f"{description}\n\n"
            f"Sanitization: quarantined untrusted run fields "
            f"({', '.join(run.quarantined_fields)})."
        )
    if instructions_md:
        description = f"{description}\n\n{instructions_md}"
    return description


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
    if run.sanitization_reasons:
        lines.append(f"sanitization_reasons: {', '.join(run.sanitization_reasons)}")
    if run.quarantined_fields:
        lines.append(f"quarantined_fields: {', '.join(run.quarantined_fields)}")
    return "\n".join(lines)


def _create_issue(
    run: FailureRun,
    *,
    project_root: pathlib.Path,
    br_bin: str,
    parent_id: str | None,
    labels_csv: str,
    instructions_md: str,
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
        labels_csv,
        "--description",
        _build_description(run, instructions_md),
        "--silent",
        "--no-daemon",
    ]
    if run.external_ref_url:
        cmd.extend(["--external-ref", run.external_ref_url])
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
            "--no-daemon",
        ],
        cwd=project_root,
    )
    if run.external_ref_url:
        _run(
            [
                br_bin,
                "update",
                issue_id,
                "--external-ref",
                run.external_ref_url,
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
        [br_bin, "update", issue_id, "--notes", _build_notes(run), "--no-daemon"],
        cwd=project_root,
    )
    if run.external_ref_url:
        _run(
            [
                br_bin,
                "update",
                issue_id,
                "--external-ref",
                run.external_ref_url,
                "--no-daemon",
            ],
            cwd=project_root,
        )


def _parse_timestamp(raw: str) -> dt.datetime | None:
    if not raw:
        return None
    value = raw.replace("Z", "+00:00")
    try:
        parsed = dt.datetime.fromisoformat(value)
    except ValueError:
        return None
    if parsed.tzinfo is None:
        return parsed.replace(tzinfo=dt.timezone.utc)
    return parsed.astimezone(dt.timezone.utc)


def _discover_agent_mail_token(explicit_token_path: str | None) -> str:
    candidate_paths: list[pathlib.Path] = []
    if explicit_token_path:
        candidate_paths.append(pathlib.Path(explicit_token_path))
    candidate_paths.extend(DEFAULT_AGENT_MAIL_TOKEN_PATHS)
    for path in candidate_paths:
        if not path.exists():
            continue
        payload = json.loads(path.read_text(encoding="utf-8"))
        header = payload["mcpServers"]["mcp-agent-mail"]["headers"]["Authorization"]
        if isinstance(header, str) and header.startswith("Bearer "):
            return header.split(" ", 1)[1]
    raise RuntimeError("could not discover Agent Mail bearer token")


def _call_agent_mail_tool(
    *,
    api_url: str,
    token: str,
    tool_name: str,
    arguments: dict[str, Any],
) -> Any:
    payload = {
        "jsonrpc": "2.0",
        "id": str(int(dt.datetime.now(dt.timezone.utc).timestamp() * 1000)),
        "method": "tools/call",
        "params": {"name": tool_name, "arguments": arguments},
    }
    request = urllib.request.Request(
        url=api_url,
        method="POST",
        data=json.dumps(payload).encode("utf-8"),
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {token}",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=10) as response:
            body = response.read()
    except urllib.error.URLError as exc:
        raise RuntimeError(f"agent mail request failed: {exc}") from exc
    payload = json.loads(body.decode("utf-8"))
    if payload.get("error"):
        raise RuntimeError(f"agent mail error: {payload['error']}")
    result = payload.get("result", {})
    if isinstance(result, dict) and "structuredContent" in result:
        return result["structuredContent"]
    return result


def _agent_mail_sender_ready(
    *,
    config: AgentMailConfig,
    project_root: pathlib.Path,
) -> None:
    _run(
        [
            config.am_binary,
            "macros",
            "start-session",
            "--project",
            str(project_root),
            "--program",
            config.sender_program,
            "--model",
            config.sender_model,
            "--agent-name",
            config.sender_name,
            "--task",
            config.sender_task,
            "--json",
        ],
        cwd=project_root,
    )


def _notification_recipients(
    *,
    config: AgentMailConfig,
    project_root: pathlib.Path,
) -> list[str]:
    if config.recipients:
        return list(config.recipients[: config.max_recipients])
    proc = _run(
        [
            config.am_binary,
            "agents",
            "list",
            "--project",
            str(project_root),
            "--json",
        ],
        cwd=project_root,
    )
    payload = json.loads(proc.stdout)
    if not isinstance(payload, list):
        raise RuntimeError("agent list response was not a JSON array")
    now = dt.datetime.now(dt.timezone.utc)
    cutoff = now - dt.timedelta(minutes=config.active_within_minutes)
    candidates: list[tuple[dt.datetime, str]] = []
    for item in payload:
        if not isinstance(item, dict):
            continue
        name = item.get("name")
        if not isinstance(name, str) or not name or name == config.sender_name:
            continue
        last_active = _parse_timestamp(str(item.get("last_active_ts") or ""))
        if last_active is None or last_active < cutoff:
            continue
        candidates.append((last_active, name))
    candidates.sort(reverse=True)
    return [name for _, name in candidates[: config.max_recipients]]


def _build_notification_body(run: FailureRun, *, issue_id: str, action: str, topic: str) -> str:
    lines = [
        f"topic: {topic}",
        f"- intake_action: {action}",
        f"- bead_id: {issue_id}",
        f"- run_id: {run.run_id}",
        f"- run_url: {run.run_url}",
        f"- workflow_path: {run.workflow_path}",
        f"- workflow_name: {run.workflow_name}",
        f"- ref: {run.ref_label}",
        f"- event: {run.event}",
        f"- summary: {run.summary}",
    ]
    if run.quarantined_fields:
        lines.append(f"- quarantined_fields: {', '.join(run.quarantined_fields)}")
    return "\n".join(lines)


def _notify_agent_mail(
    *,
    config: AgentMailConfig,
    project_root: pathlib.Path,
    run: FailureRun,
    issue_id: str,
    action: str,
    dry_run: bool,
) -> dict[str, Any]:
    if not config.enabled:
        return {"status": "disabled"}
    recipients = _notification_recipients(config=config, project_root=project_root)
    if not recipients:
        return {"status": "skipped", "reason": "no_recent_recipients"}
    if dry_run:
        return {
            "status": "dry-run",
            "topic": config.topic,
            "recipients": recipients,
            "issue_id": issue_id,
            "action": action,
        }
    _agent_mail_sender_ready(config=config, project_root=project_root)
    token = _discover_agent_mail_token(config.token_path)
    response = _call_agent_mail_tool(
        api_url=config.api_url,
        token=token,
        tool_name="send_message",
        arguments={
            "project_key": str(project_root),
            "sender_name": config.sender_name,
            "to": recipients,
            "subject": f"CI failure {action}: {run.workflow_path} [{run.ref_label}] -> {issue_id}",
            "body_md": _build_notification_body(
                run, issue_id=issue_id, action=action, topic=config.topic
            ),
            "importance": config.importance,
            "ack_required": config.ack_required,
            "topic": config.topic,
        },
    )
    return {
        "status": "sent",
        "topic": config.topic,
        "recipients": recipients,
        "issue_id": issue_id,
        "action": action,
        "response": response,
    }


def main() -> int:
    args = parse_args()
    project_root = pathlib.Path(args.project_root).resolve()
    br_bin = args.br_binary
    config_path = pathlib.Path(args.config).resolve() if args.config else None
    state_path = pathlib.Path(args.state_file).resolve() if args.state_file else None
    if not pathlib.Path(br_bin).is_absolute():
        br_bin = str((project_root / br_bin).resolve())
    config = _load_config(config_path)
    state = _load_state(state_path)

    if args.runs_json:
        runs = _load_runs_fixture(pathlib.Path(args.runs_json))
        source = "fixture"
    else:
        runs = _load_runs_live(args.repo, args.per_page, project_root)
        source = "gh_api"

    parsed = []
    skipped = 0
    for run in runs:
        parsed_run = _parse_run(args.repo, run, config.workflow_prefixes)
        if parsed_run is None:
            skipped += 1
            continue
        parsed.append(parsed_run)

    latest_runs = _choose_latest(parsed)
    parent_id: str | None = None
    requested_parent = args.parent_id if args.parent_id is not None else config.parent_id
    if _parent_exists(requested_parent, project_root=project_root, br_bin=br_bin):
        parent_id = requested_parent

    existing = _active_intake_issues(project_root=project_root, br_bin=br_bin)
    created = []
    updated = []
    untouched = []
    notifications = []
    for run in latest_runs:
        existing_id = existing.get(run.issue_title)
        state_key = run.dedupe_key
        previous = state.get(state_key, {})
        previous_run_id = previous.get("run_id")
        if existing_id and previous_run_id == run.run_id:
            untouched.append(
                {
                    "issue_id": existing_id,
                    "title": run.issue_title,
                    "run_id": run.run_id,
                    "reason": "already_ingested",
                }
            )
            continue
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
                    "quarantined_fields": list(run.quarantined_fields),
                }
            )
            notifications.append(
                _notify_agent_mail(
                    config=config.agent_mail,
                    project_root=project_root,
                    run=run,
                    issue_id=existing_id,
                    action="updated",
                    dry_run=args.dry_run,
                )
            )
            state[state_key] = {
                "issue_id": existing_id,
                "run_id": run.run_id,
                "run_url": run.run_url,
                "updated_at": run.updated_at,
            }
            continue

        issue_id = _create_issue(
            run,
            project_root=project_root,
            br_bin=br_bin,
            parent_id=parent_id,
            labels_csv=config.labels_csv,
            instructions_md=config.instructions_md,
            dry_run=args.dry_run,
        )
        created.append(
            {
                "issue_id": issue_id,
                "title": run.issue_title,
                "run_id": run.run_id,
                "run_url": run.run_url,
                "quarantined_fields": list(run.quarantined_fields),
            }
        )
        notifications.append(
            _notify_agent_mail(
                config=config.agent_mail,
                project_root=project_root,
                run=run,
                issue_id=issue_id,
                action="created",
                dry_run=args.dry_run,
            )
        )
        existing[run.issue_title] = issue_id
        state[state_key] = {
            "issue_id": issue_id,
            "run_id": run.run_id,
            "run_url": run.run_url,
            "updated_at": run.updated_at,
        }

    if not latest_runs:
        untouched.append("no release/validation failures found")

    if not args.dry_run:
        _save_state(state_path, state)

    result = {
        "source": source,
        "repo": args.repo,
        "dry_run": args.dry_run,
        "config_path": str(config_path) if config_path else None,
        "state_file": str(state_path) if state_path else None,
        "parent_linked": parent_id is not None,
        "candidates_total": len(runs),
        "candidates_supported": len(parsed),
        "candidates_skipped": skipped,
        "ingested_keys": len(latest_runs),
        "created": created,
        "updated": updated,
        "untouched": untouched,
        "notifications": notifications,
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:  # pragma: no cover - top-level fatal diagnostics
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(2)
