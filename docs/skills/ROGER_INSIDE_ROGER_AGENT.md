# Roger Inside Roger Agent

Status: reusable Roger skill.

Purpose:
Use this skill only when the agent is already inside a Roger-managed provider
session or bare-harness continuation and needs the smallest safe Roger-native
command subset.

This skill is intentionally narrow. It does not widen the in-harness command
surface, and it remains narrower than the dedicated `rr agent` worker
transport.

## When to use it

Use this skill when all of the following are true:

- Roger already launched or resumed the session
- the agent is currently operating inside that provider session
- the agent needs bounded Roger context or a safe way back to Roger

Do not use it:

- as a launch path
- as a substitute for `rr --robot`
- for approval or posting
- for raw `gh` review communication
- for mutation-capable work unless Roger explicitly elevated the mode

## Safe subset

Prefer this order:

1. `roger-help`
2. `roger-status`
3. `roger-findings`
4. `roger-return`

Rules:

- treat these as convenience adapters over Roger-owned semantics
- if a command is unsupported in the current harness, fail closed and use the
  equivalent `rr` command outside the harness
- do not invent richer in-harness commands because they feel convenient
- do not bypass Roger approval, posting, or finding-validation boundaries

## Example

Inside a Roger-managed session:

1. run `roger-help` to confirm the harness-safe subset
2. run `roger-status` to inspect current Roger state
3. run `roger-findings` to inspect the current finding set
4. when the in-harness work is complete or blocked, run `roger-return`

If any of those commands are unsupported, stop and use the equivalent `rr`
surface outside the harness instead of improvising.

## Minimal doctrine

- stay inside Roger truth, not provider-local vibes
- use Roger-native status/findings surfaces before inferring state
- keep work read-mostly unless Roger visibly elevated capability
- return to Roger rather than improvising approval/posting flows

## Anti-patterns

Do not:

- use raw `gh` from inside the harness
- treat provider-local memory or chat state as Roger truth
- assume `roger-clarify` or `roger-open-drafts` exist, or invent a manual
  refresh command instead of relying on Roger's automatic reconciliation model
- present an unsupported in-harness affordance, or parity with `rr agent`, as
  if it were shipped
