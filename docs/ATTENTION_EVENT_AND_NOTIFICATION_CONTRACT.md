# Attention Event And Notification Contract

## Purpose

This document freezes the `0.1.0` Roger-owned attention-state contract for
session status, local attention queues, and bounded mirror surfaces.

It narrows the canonical plan's trigger and notification section into an
implementation-facing support contract so later CLI, TUI, bridge, and extension
work do not each invent their own status vocabulary.

## Authority And Scope

- `AGENTS.md` remains the operating authority for repo work.
- `docs/PLAN_FOR_ROGER_REVIEWER.md` remains the canonical product plan.
- This document is an implementation-facing support contract for bead
  `rr-006.3`.
- If this document conflicts with the canonical plan, the canonical plan wins
  until the plan is deliberately updated.

## Non-Goals

- a general-purpose notification framework
- a polling-based workflow engine
- raw harness progress events exposed as Roger truth
- browser-defined status semantics
- mandatory desktop notifications in `0.1.0`

## Design Rules

- Roger owns canonical attention state for a review session.
- Harness, bridge, and browser signals may inform state changes, but they do
  not define Roger state directly.
- The TUI and CLI are required first-class readers of attention state.
- The browser extension may mirror Roger attention state only when the chosen
  bridge can read it back truthfully.
- If a surface cannot read Roger state truthfully, it must degrade to launch,
  resume, or handoff affordances instead of inventing stale or guessed status.
- Attention state is small, explicit, and normalized. Roger does not need a
  large event taxonomy in `0.1.0`.
- Notification delivery is a mirror of Roger state, not a second source of
  truth.

## Core Model

Roger should store two related but distinct objects:

- `AttentionState`: the current canonical session-level attention status used
  by TUI queues, CLI status, and session-finder views
- `AttentionEvent`: append-only history recording when Roger changed the
  canonical attention state and why

Rules:

- one `ReviewSession` has at most one current `AttentionState` at a time
- an `AttentionEvent` may change the state or record a no-op reaffirmation, but
  the current state remains the hot-path lookup
- Roger may derive local notifications or badge counts from the current state
  plus recent events
- Roger should not require every surface to subscribe to a live event stream in
  `0.1.0`; durable reads from the canonical store remain sufficient

## Canonical `0.1.0` Attention States

The required canonical state set is:

| State | Meaning | Attention-bearing |
| --- | --- | --- |
| `review_started` | Roger has created or resumed the review session and the session is now active locally | No |
| `awaiting_user_input` | Roger needs a human decision, clarification, or triage action before the next meaningful step | Yes |
| `awaiting_outbound_approval` | Local outbound drafts exist and explicit approval is required before any GitHub write path can proceed | Yes |
| `findings_ready` | A review pass completed and produced findings or updated review output ready for human inspection | Yes |
| `refresh_recommended` | The review target changed enough that a fresh pass is recommended before relying on older findings or drafts | Yes |
| `review_failed` | Roger could not continue automatically and needs operator recovery or explicit retry | Yes |

Rules:

- These names are the stable Roger-facing vocabulary for `0.1.0`.
- `review_started` is a real canonical state but is not by itself a queueable
  "needs action now" state.
- Roger may materialize a separate boolean or derived priority for
  "attention-requiring now" without inventing new state names.
- If a future phase needs a neutral steady-state such as `idle` or
  `completed_without_findings`, it should be added deliberately rather than
  inferred by surfaces ad hoc.

## `review_attached` Clarification

The canonical plan's minimum event set also mentioned `review_attached`.

For `0.1.0`, Roger should treat `review_attached` as a lifecycle event that may
appear in launch/resume history, not as a separate durable attention state.
That keeps the attention queue small while still allowing launch surfaces to
record that a PR, repo context, or harness session was bound successfully.

## Attention Event Envelope

Roger should expose attention history through one append-only event type:
`attention_state_changed`.

Required fields:

- `id`
- `review_session_id`
- `occurred_at`
- `from_state` nullable for first-state creation
- `to_state`
- `reason_code`
- `source_surface`
- `actor_kind`
- `payload`

Recommended payload fields:

- `review_run_id` when the transition is tied to one review pass
- `trigger_kind` such as `launch`, `prompt_run`, `user_action`, `bridge_read`,
  `refresh_probe`, or `error`
- `summary` as a short operator-facing explanation
- `draft_batch_id` when moving into `awaiting_outbound_approval`
- `finding_counts` when moving into `findings_ready`
- `target_revision` when moving into `refresh_recommended`
- `error_code` and `recovery_hint` when moving into `review_failed`

Rules:

- `to_state` must always be one of the canonical states above.
- `reason_code` must be Roger-owned and stable enough for later filtering.
- `source_surface` records where the transition was observed or initiated; it
  does not transfer state ownership away from Roger.
- A harness-specific wait flag or browser callback should be normalized into
  Roger's state vocabulary before persistence.

## Normalization Rules

Roger should normalize incoming signals using these rules:

- harness progress, wait, or failure signals may contribute evidence for a
  Roger transition, but Roger persists only the normalized state name and
  Roger-owned reason code
- browser launch signals may create `review_started` or emit a lifecycle
  `review_attached` event, but the browser does not set richer attention states
  on its own
- user actions in the TUI or CLI may clear or replace an existing attention
  state only through Roger-owned state-transition logic
- GitHub or PR metadata changes may trigger `refresh_recommended`, but the
  stored state is Roger's interpretation of that change rather than a raw
  webhook-style event

## Surface Exposure Contract

### TUI

The TUI must expose attention state directly.

Minimum `0.1.0` responsibilities:

- review-home attention queue
- session overview and current-state label
- visible distinction between `awaiting_user_input`,
  `awaiting_outbound_approval`, `findings_ready`, `refresh_recommended`, and
  `review_failed`
- actionable entrypoints that return the user to the correct local workflow for
  the current state

### CLI

The CLI must expose attention state directly.

Minimum `0.1.0` responsibilities:

- `rr status` or equivalent session-status output includes current
  `AttentionState`
- session-finder or resume flows can filter or surface sessions by attention
  state
- machine-readable CLI output uses the same canonical state names rather than a
  second CLI-only vocabulary

### Browser Extension

The extension is a mirror surface, not the source of truth.

Allowed `0.1.0` behaviors:

- show entrypoints such as start, resume, or open locally
- show lightweight mirrored Roger status when the bridge can read it back
  truthfully
- surface bounded counts or badges derived from the canonical attention state

Not allowed:

- defining browser-only attention labels
- pretending cached or guessed state is current Roger truth
- requiring browser readback for the core local CLI/TUI workflow

### Optional Local Notification Mirrors

Desktop notifications or other local mirrors are optional in `0.1.0`.

If implemented, they must:

- render canonical Roger state names or Roger-owned human labels derived from
  them
- deep-link back to a Roger-owned local surface
- avoid becoming the only place where a state transition is visible

## Degraded Readback Rules

When a surface cannot read Roger state back truthfully, degraded behavior must
be explicit.

### Required degraded behavior for the extension

- allow thin launch, resume, or "open in Roger" actions when possible
- omit mirrored status labels and counts rather than showing stale values
- if needed, show a bounded "status unavailable locally" affordance that points
  the user to the TUI or CLI for authoritative state

### Required degraded behavior for CLI/TUI

- if the canonical store is readable but a live worker or harness is not, show
  the last persisted attention state with a truthful freshness indicator where
  available
- if the canonical store itself is unavailable or repair-needed, do not invent
  a last-known good state; report a blocked or repair-needed condition instead

## Session-Finder And Queue Semantics

Roger should treat attention state as shared product infrastructure rather than
as one-screen decoration.

Minimum `0.1.0` uses:

- TUI review-home attention queue
- CLI status summaries
- global session-finder filters for attention-requiring sessions
- lightweight extension mirroring when bridge readback is available

Recommended queue priority order:

1. `review_failed`
2. `awaiting_outbound_approval`
3. `awaiting_user_input`
4. `refresh_recommended`
5. `findings_ready`
6. `review_started`

This ordering is a presentation recommendation, not a new canonical state field.

## Relationship To Outcome Events

This contract does not replace the broader outcome-event model.

Rules:

- attention transitions should be persisted as `attention_state_changed`
  history
- prompt, finding, draft, approval, and posting history still belong to their
  own typed event families
- surfaces may compose those histories together, but attention remains the
  small cross-surface status layer

## Acceptance Mapping For Later Beads

This contract is sufficient for later implementation beads if they preserve all
of the following:

- the stable state names frozen above
- direct exposure in TUI and CLI
- mirror-only semantics for extension and optional notification surfaces
- truthful degraded behavior when the bridge cannot read status back
- Roger-owned normalization instead of raw harness or browser-defined states
