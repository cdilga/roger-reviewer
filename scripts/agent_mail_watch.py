#!/usr/bin/env python3
"""Read-only Agent Mail watcher for browser/ngrok access.

This is intentionally separate from the MCP endpoint:
- Browser clients authenticate with Basic Auth to this watcher.
- The watcher authenticates to Agent Mail with the local bearer token.
- The watcher only exposes health and inbox reads.
"""

from __future__ import annotations

import argparse
import base64
import html
import json
import secrets
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any


DEFAULT_AGENT_MAIL_API = "http://127.0.0.1:8765/api/"
DEFAULT_PROJECT_KEY = "/Users/cdilga/Documents/dev/roger-reviewer"
DEFAULT_TOKEN_PATHS = (
    Path("/Users/cdilga/Documents/dev/mcp_agent_mail/codex.mcp.json"),
    Path("/Users/cdilga/Documents/dev/roger-reviewer/mcp_agent_mail/codex.mcp.json"),
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bind", default="127.0.0.1", help="bind host")
    parser.add_argument("--port", type=int, default=8781, help="bind port")
    parser.add_argument("--agent-mail-api", default=DEFAULT_AGENT_MAIL_API, help="Agent Mail MCP HTTP endpoint")
    parser.add_argument("--project-key", default=DEFAULT_PROJECT_KEY, help="default Agent Mail project key")
    parser.add_argument("--agents", default="", help="comma-separated default agent names")
    parser.add_argument("--limit", type=int, default=20, help="default inbox fetch limit")
    parser.add_argument(
        "--watch-password",
        default="",
        help="Basic Auth password for the browser watcher. If omitted, a random password is generated at startup.",
    )
    return parser.parse_args()


def discover_agent_mail_token() -> str:
    for path in DEFAULT_TOKEN_PATHS:
        if not path.exists():
            continue
        data = json.loads(path.read_text())
        header = data["mcpServers"]["mcp-agent-mail"]["headers"]["Authorization"]
        if header.startswith("Bearer "):
            return header.split(" ", 1)[1]
    raise RuntimeError("Could not discover Agent Mail bearer token from codex.mcp.json")


def coerce_structured_result(payload: dict[str, Any]) -> Any:
    result = payload.get("result", {})
    if "structuredContent" in result:
        return result["structuredContent"]
    for item in result.get("content", []):
        text = item.get("text")
        if not isinstance(text, str):
            continue
        try:
            return json.loads(text)
        except json.JSONDecodeError:
            return text
    if payload.get("error"):
        return {"error": payload["error"]}
    return result


@dataclass
class AppConfig:
    agent_mail_api: str
    project_key: str
    default_agents: list[str]
    default_limit: int
    bearer_token: str
    watch_password: str
    started_at: float


class AgentMailClient:
    def __init__(self, base_url: str, bearer_token: str) -> None:
        self.base_url = base_url
        self.bearer_token = bearer_token

    def _request(self, url: str, *, payload: dict[str, Any] | None = None) -> Any:
        data = None
        headers = {}
        if payload is None:
            method = "GET"
        else:
            method = "POST"
            data = json.dumps(payload).encode("utf-8")
            headers["Content-Type"] = "application/json"
            headers["Authorization"] = f"Bearer {self.bearer_token}"
        req = urllib.request.Request(url=url, data=data, headers=headers, method=method)
        with urllib.request.urlopen(req, timeout=10) as response:
            body = response.read()
            if not body:
                return None
            return json.loads(body.decode("utf-8"))

    def health(self) -> dict[str, Any]:
        readiness_url = urllib.parse.urljoin(self.base_url, "../health/readiness")
        return self._request(readiness_url)

    def call_tool(self, name: str, arguments: dict[str, Any]) -> Any:
        payload = {
            "jsonrpc": "2.0",
            "id": str(int(time.time() * 1000)),
            "method": "tools/call",
            "params": {"name": name, "arguments": arguments},
        }
        return coerce_structured_result(self._request(self.base_url, payload=payload))


def render_html() -> str:
    return """<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Agent Mail Watch</title>
  <style>
    :root {
      color-scheme: light;
      --bg: #f5f0e8;
      --panel: #fffaf4;
      --line: #c9b8a2;
      --ink: #1d1a17;
      --muted: #6f6255;
      --accent: #0b6e4f;
      --accent-soft: #d8efe6;
      --warn: #8e5a1f;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      font-family: ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      color: var(--ink);
      background:
        radial-gradient(circle at top left, #f9e8c7 0, transparent 30%),
        radial-gradient(circle at bottom right, #dcebdc 0, transparent 28%),
        var(--bg);
    }
    main {
      max-width: 1100px;
      margin: 0 auto;
      padding: 20px;
    }
    .panel {
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 16px;
      padding: 16px;
      box-shadow: 0 12px 30px rgba(0, 0, 0, 0.04);
      margin-bottom: 16px;
    }
    h1, h2, h3 { margin: 0 0 12px; }
    p { margin: 0; }
    form {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
      gap: 12px;
      align-items: end;
    }
    label {
      display: grid;
      gap: 6px;
      font-size: 14px;
      color: var(--muted);
    }
    input, button {
      border-radius: 10px;
      border: 1px solid var(--line);
      padding: 10px 12px;
      font: inherit;
    }
    button {
      background: var(--accent);
      color: white;
      border: 0;
      font-weight: 600;
      cursor: pointer;
    }
    .meta {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
      gap: 12px;
    }
    .metric {
      background: #fff;
      border: 1px solid var(--line);
      border-radius: 12px;
      padding: 12px;
    }
    .metric strong {
      display: block;
      font-size: 24px;
      margin-bottom: 4px;
    }
    .muted { color: var(--muted); }
    .warn { color: var(--warn); }
    .agent-grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
      gap: 16px;
    }
    .message {
      border-top: 1px solid #eadcc8;
      padding-top: 12px;
      margin-top: 12px;
    }
    .message:first-child {
      border-top: 0;
      padding-top: 0;
      margin-top: 0;
    }
    .subject {
      font-weight: 700;
      margin-bottom: 6px;
    }
    .pill {
      display: inline-block;
      border-radius: 999px;
      padding: 2px 8px;
      font-size: 12px;
      background: var(--accent-soft);
      color: var(--accent);
      margin-right: 6px;
      margin-bottom: 6px;
    }
    pre {
      white-space: pre-wrap;
      word-break: break-word;
      font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
      background: #fff;
      border: 1px solid #eadcc8;
      border-radius: 10px;
      padding: 10px;
      margin: 8px 0 0;
      max-height: 240px;
      overflow: auto;
    }
  </style>
</head>
<body>
  <main>
    <section class="panel">
      <h1>Agent Mail Watch</h1>
      <p class="muted">Read-only browser view backed by the local Agent Mail MCP server. Auto-refresh runs every 15 seconds.</p>
    </section>
    <section class="panel">
      <form id="watch-form">
        <label>Project Key
          <input id="project" name="project" placeholder="/abs/path/project">
        </label>
        <label>Agents
          <input id="agents" name="agents" placeholder="BlueLake,GreenStone">
        </label>
        <label>Limit
          <input id="limit" name="limit" type="number" min="1" max="100" value="20">
        </label>
        <button type="submit">Refresh</button>
      </form>
    </section>
    <section class="panel">
      <div class="meta" id="meta"></div>
    </section>
    <section class="agent-grid" id="agents-grid"></section>
  </main>
  <script>
    const metaEl = document.getElementById("meta");
    const gridEl = document.getElementById("agents-grid");
    const formEl = document.getElementById("watch-form");
    const projectEl = document.getElementById("project");
    const agentsEl = document.getElementById("agents");
    const limitEl = document.getElementById("limit");

    function escapeHtml(value) {
      return value
        .replaceAll("&", "&amp;")
        .replaceAll("<", "&lt;")
        .replaceAll(">", "&gt;");
    }

    function parseState() {
      const params = new URLSearchParams(window.location.search);
      return {
        project: params.get("project") || "",
        agents: params.get("agents") || "",
        limit: params.get("limit") || "20",
      };
    }

    function syncForm() {
      const state = parseState();
      projectEl.value = state.project;
      agentsEl.value = state.agents;
      limitEl.value = state.limit;
    }

    function renderMeta(state) {
      const cards = [
        ["Agent Mail", state.health && state.health.status ? state.health.status : "unknown"],
        ["Project", state.project_key || "unset"],
        ["Watching", String((state.agents || []).length)],
        ["Updated", state.generated_at || "unknown"],
      ];
      metaEl.innerHTML = cards.map(([label, value]) => `
        <div class="metric">
          <div class="muted">${escapeHtml(label)}</div>
          <strong>${escapeHtml(String(value))}</strong>
        </div>
      `).join("");
    }

    function renderAgents(state) {
      if (state.error) {
        gridEl.innerHTML = `<section class="panel"><h2 class="warn">Watcher Error</h2><pre>${escapeHtml(JSON.stringify(state.error, null, 2))}</pre></section>`;
        return;
      }
      if (!state.agents || state.agents.length === 0) {
        gridEl.innerHTML = `<section class="panel"><h2>No agents configured</h2><p class="muted">Set one or more comma-separated agent names above.</p></section>`;
        return;
      }
      gridEl.innerHTML = state.agents.map((agent) => {
        const messages = Array.isArray(agent.messages) ? agent.messages : [];
        const messageHtml = messages.length === 0
          ? `<p class="muted">No messages returned.</p>`
          : messages.map((message) => `
              <article class="message">
                <div class="subject">${escapeHtml(message.subject || "(no subject)")}</div>
                <div>
                  <span class="pill">${escapeHtml(message.importance || "normal")}</span>
                  <span class="pill">${escapeHtml(message.from || "unknown sender")}</span>
                  <span class="pill">${escapeHtml(message.created_ts || "no timestamp")}</span>
                </div>
                <p class="muted">${escapeHtml(message.topic || "")}</p>
                <pre>${escapeHtml(message.body_md || "")}</pre>
              </article>
            `).join("");
        return `
          <section class="panel">
            <h2>${escapeHtml(agent.agent_name)}</h2>
            <p class="muted">Unread snapshot of the agent inbox.</p>
            ${messageHtml}
          </section>
        `;
      }).join("");
    }

    async function refresh() {
      const state = parseState();
      const query = new URLSearchParams(state);
      const response = await fetch(`/api/state?${query.toString()}`, { cache: "no-store" });
      const payload = await response.json();
      renderMeta(payload);
      renderAgents(payload);
    }

    formEl.addEventListener("submit", (event) => {
      event.preventDefault();
      const params = new URLSearchParams({
        project: projectEl.value.trim(),
        agents: agentsEl.value.trim(),
        limit: limitEl.value.trim() || "20",
      });
      history.replaceState(null, "", `/?${params.toString()}`);
      refresh().catch((error) => {
        gridEl.innerHTML = `<section class="panel"><h2 class="warn">Request Failed</h2><pre>${escapeHtml(String(error))}</pre></section>`;
      });
    });

    syncForm();
    refresh();
    setInterval(() => { refresh().catch(() => {}); }, 15000);
  </script>
</body>
</html>"""


class WatchHandler(BaseHTTPRequestHandler):
    server_version = "AgentMailWatch/0.1"

    def do_GET(self) -> None:  # noqa: N802
        if self.path.startswith("/healthz"):
            self.respond_json(200, {"status": "ok"})
            return

        if not self.check_basic_auth():
            return

        if self.path.startswith("/api/state"):
            self.serve_state()
            return

        if self.path == "/" or self.path.startswith("/?"):
            self.respond_html(200, render_html())
            return

        self.respond_json(404, {"error": "Not found"})

    def log_message(self, format: str, *args: object) -> None:
        sys.stderr.write("%s - - [%s] %s\n" % (self.address_string(), self.log_date_time_string(), format % args))

    @property
    def config(self) -> AppConfig:
        return self.server.config  # type: ignore[attr-defined]

    @property
    def client(self) -> AgentMailClient:
        return self.server.client  # type: ignore[attr-defined]

    def check_basic_auth(self) -> bool:
        auth = self.headers.get("Authorization", "")
        if not auth.startswith("Basic "):
            self.require_auth()
            return False
        try:
            decoded = base64.b64decode(auth.split(" ", 1)[1]).decode("utf-8")
        except Exception:
            self.require_auth()
            return False
        username, _, password = decoded.partition(":")
        if username != "watch" or password != self.config.watch_password:
            self.require_auth()
            return False
        return True

    def require_auth(self) -> None:
        self.send_response(401)
        self.send_header("WWW-Authenticate", 'Basic realm="Agent Mail Watch"')
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(b'{"error":"Unauthorized"}')

    def parse_watch_query(self) -> tuple[str, list[str], int]:
        parsed = urllib.parse.urlparse(self.path)
        params = urllib.parse.parse_qs(parsed.query)
        project_key = params.get("project", [self.config.project_key])[0] or self.config.project_key
        agents_raw = params.get("agents", [",".join(self.config.default_agents)])[0]
        limit_raw = params.get("limit", [str(self.config.default_limit)])[0]
        try:
            limit = max(1, min(100, int(limit_raw)))
        except ValueError:
            limit = self.config.default_limit
        agents = [item.strip() for item in agents_raw.split(",") if item.strip()]
        return project_key, agents, limit

    def serve_state(self) -> None:
        project_key, agents, limit = self.parse_watch_query()
        state: dict[str, Any] = {
            "project_key": project_key,
            "agents": [],
            "generated_at": time.strftime("%Y-%m-%d %H:%M:%S"),
        }
        try:
            state["health"] = self.client.health()
            for agent_name in agents:
                inbox = self.client.call_tool(
                    "fetch_inbox",
                    {
                        "project_key": project_key,
                        "agent_name": agent_name,
                        "limit": limit,
                        "include_bodies": True,
                    },
                )
                state["agents"].append({"agent_name": agent_name, "messages": inbox})
        except urllib.error.HTTPError as exc:
            try:
                details = json.loads(exc.read().decode("utf-8"))
            except Exception:
                details = {"status": exc.code, "reason": exc.reason}
            state["error"] = details
        except Exception as exc:  # pragma: no cover - defensive surface
            state["error"] = {"message": str(exc)}
        self.respond_json(200, state)

    def respond_html(self, status: int, body: str) -> None:
        encoded = body.encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def respond_json(self, status: int, payload: dict[str, Any]) -> None:
        encoded = json.dumps(payload, indent=2).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)


def main() -> int:
    args = parse_args()
    password = args.watch_password or secrets.token_urlsafe(18)
    config = AppConfig(
        agent_mail_api=args.agent_mail_api,
        project_key=args.project_key,
        default_agents=[item.strip() for item in args.agents.split(",") if item.strip()],
        default_limit=max(1, min(100, args.limit)),
        bearer_token=discover_agent_mail_token(),
        watch_password=password,
        started_at=time.time(),
    )
    client = AgentMailClient(base_url=config.agent_mail_api, bearer_token=config.bearer_token)
    server = ThreadingHTTPServer((args.bind, args.port), WatchHandler)
    server.config = config  # type: ignore[attr-defined]
    server.client = client  # type: ignore[attr-defined]

    print(f"Agent Mail Watch listening on http://{args.bind}:{args.port}")
    print("Browser auth username: watch")
    print(f"Browser auth password: {password}")
    print("Tunnel this watcher, not the raw MCP endpoint:")
    print(f"  ngrok http {args.port}")
    print("Suggested browser path:")
    default_agents = ",".join(config.default_agents)
    query = urllib.parse.urlencode(
        {"project": config.project_key, "agents": default_agents, "limit": str(config.default_limit)}
    )
    print(f"  http://{args.bind}:{args.port}/?{query}")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down watcher.")
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
