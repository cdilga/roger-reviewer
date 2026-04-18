Roger Reviewer is a local-first pull request review system. When working in this repository, optimize for truthful review workflows, durable local state, and explicit operator approval before any outbound GitHub action.

Default posture in this repository:
- Review and suggest only unless a human explicitly enables fix mode.
- Keep all findings, notes, and draft material local to Roger until the operator explicitly approves a post.
- Do not use direct GitHub write commands, raw `gh` review/comment posting, or ad hoc outbound mutations as a shortcut around Roger.
- Treat local session state, hook artifacts, and stored review records as the durable source of continuity.
- Prefer bounded, reproducible CLI and test flows over hidden background services or one-off shell state.

Before changing code, read `AGENTS.md` and follow the repo's live constraints, support-claim rules, and testing expectations.
