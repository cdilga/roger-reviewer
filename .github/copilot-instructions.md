Roger Reviewer is a local-first pull request review system. When working in this repository, optimize for truthful review workflows, durable local state, and explicit operator approval before any outbound GitHub action.

Default posture in this repository:
- Review and suggest only unless a human explicitly enables fix mode.
- Keep all findings, notes, and draft material local to Roger until the operator explicitly approves a post.
- Do not use direct GitHub write commands, raw `gh` review/comment posting, or ad hoc outbound mutations as a shortcut around Roger.
- Treat local session state, hook artifacts, and stored review records as the durable source of continuity.
- Prefer bounded, reproducible CLI and test flows over hidden background services or one-off shell state.

Inside a Roger-managed review session (review_readonly policy), the pre-tool-use hook denies almost all shell, every repository write, and all direct GitHub mutation. A fail-closed carve-out still permits the Roger worker transport and read-only Roger surfaces, so you can and should stay inside Roger truth:
- `rr agent <op> --task-file <path>` is the dedicated in-session worker transport (for example `rr agent worker.get_status --task-file <path>`, `worker.get_review_context`, `worker.search_memory`, `worker.list_findings`, `worker.submit_stage_result`). Every worker operation reads or submits within Roger's own nonce-gated boundary; `rr agent` rejects `--robot`.
- Read-only Roger surfaces are allowed only in robot form: `rr status --robot`, `rr findings --robot`, `rr sessions --robot`, `rr search <term> --robot`.
- Everything else (any other command, command chaining with `&&`/`||`/`|`/`;`, quoting, `cd` prefixes, env-var prefixes, subshells, redirection, `rr init|triage|draft|approve|post|update|setup`, and `gh` writes) stays denied. Do not attempt them; issue one clean allowlisted command at a time.

Before changing code, read `AGENTS.md` and follow the repo's live constraints, support-claim rules, and testing expectations.
