---
name: Roger Inside Roger Agent
description: Use only when already inside a Roger-managed provider session or bare-harness continuation and you need the narrow safe Roger-native in-harness command subset. Keeps the agent inside Roger truth, prefers roger-help/status/findings/return, and forbids inventing richer in-harness powers.
---

# Roger Inside Roger Agent

This is a project skill for Claude Code. It is only for the agent while it is
already inside Roger.

For the canonical repo contract version used by Codex and other harnesses, see
`docs/skills/ROGER_INSIDE_ROGER_AGENT.md`.

## Use this skill when

Apply this skill only when:

- Roger already launched or resumed the current provider session
- you are inside that session right now
- you need the smallest safe Roger-native command subset

Do not use it as a launch path or as a way to widen your authority.

## Safe command subset

Use this order:

1. `roger-help`
2. `roger-status`
3. `roger-findings`
4. `roger-return`

If one of these is unsupported in the current harness, fail closed and use the
equivalent `rr` command outside the harness.

## Rules

- Stay inside Roger truth, not provider-local assumptions.
- Keep work read-mostly unless Roger explicitly elevated the mode.
- Do not use raw `gh`, approval, or posting flows from inside the harness.
- Do not invent `rr agent` semantics or richer in-harness commands that Roger
  has not shipped.

## Minimal workflow

1. Run `roger-help` to confirm what the harness supports.
2. Run `roger-status` to inspect current Roger state.
3. Run `roger-findings` if you need the active finding set.
4. Run `roger-return` when the in-harness work is complete or blocked.

## Anti-patterns

Do not:

- treat provider-local memory as Roger truth
- assume `roger-refresh`, `roger-clarify`, or `roger-open-drafts` exist
- bypass Roger validation or approval boundaries
