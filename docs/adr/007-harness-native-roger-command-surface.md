# ADR 007: Harness-Native Roger Command Surface

- Status: accepted
- Date: 2026-03-29

## Context

Roger already needs a durable CLI surface such as `rr resume`, `rr status`, and
`rr return`. That is the canonical control path.

But some supported or future harnesses also expose in-session command
affordances such as slash commands, subcommands, or provider-specific command
palettes. If Roger ignores that capability entirely, the dropout-to-bare-
harness story becomes clumsy: the user can inspect code in the bare harness, but
has no natural in-context way to ask Roger for status, findings, clarification,
or a clean return path.

If Roger overfits to one harness's syntax, the opposite problem appears:
provider-specific command glue leaks into app-core and the semantics drift away
from the canonical CLI.

Roger needs one contract that preserves both truths:

- `rr` remains the canonical Roger command surface
- supported harnesses may expose thin Roger-native in-session commands as an
  adapter capability

## Decision

Roger should define a stable logical command surface in core, with optional
harness-native bindings layered on top of it.

Recommended logical command IDs:

- `roger-help`
- `roger-status`
- `roger-findings`
- `roger-refresh`
- `roger-clarify`
- `roger-open-drafts`
- `roger-return`

Recommended core objects:

- `RogerCommand`
  - `command_id`
  - `review_session_id`
  - `review_run_id` when relevant
  - `args`
  - `invocation_surface` such as `cli`, `tui`, or `harness_command`
  - `provider`
- `RogerCommandResult`
  - `status`
  - `user_message`
  - `next_action`
  - `session_binding`
  - optional payload or deep-link target
- `HarnessCommandBinding`
  - `provider`
  - `command_id`
  - `provider_command_syntax`
  - `capability_requirements`

Dispatch rules:

- the core Roger operation is the source of truth; harness commands are thin
  adapters over the same routing used by the CLI
- logical command IDs stay Roger-owned even when literal syntax differs by
  harness
- unsupported harness commands must fail truthfully and point to the equivalent
  `rr` command or session-finder path
- harness command support is optional and must never be the only way to access
  a Roger capability
- mutation-capable flows such as approval and GitHub posting remain elevated in
  the TUI or CLI approval flow rather than hidden behind lightweight in-harness
  commands

`0.1.0` stance:

- no provider is required to support Roger-native in-harness commands
- OpenCode may expose a small safe subset if it can do so cleanly
- Gemini is not required to expose any Roger-native commands in `0.1.0`

Preferred first subset when implemented:

- `roger-help`
- `roger-status`
- `roger-findings`
- `roger-return`

Still optional even on capable harnesses:

- `roger-refresh`
- `roger-clarify`
- `roger-open-drafts`

## Consequences

- Roger gets natural in-harness ergonomics without making any single harness the
  architecture center
- CLI, TUI, and supported harnesses can share semantics and state transitions
  rather than drift into separate mini-products
- return-to-Roger, status, and clarification become easier to discover from
  within a dropped-out harness session
- provider-specific syntax stays isolated in adapter bindings instead of leaking
  into app-core

## Open Questions

- should `roger-findings` always reopen the TUI when available, or can some
  harnesses return a bounded textual view first?
- should Roger surface provider-specific command help automatically when
  entering bare-harness mode?

## Follow-up

- define the capability discovery shape for `supports_roger_commands`
- define the core command router and error model
- add flow coverage for in-harness command usage and truthful CLI fallback
- decide which command IDs are mandatory versus optional per harness
