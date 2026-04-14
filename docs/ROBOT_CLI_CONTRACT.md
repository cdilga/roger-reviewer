# Robot CLI Contract

This document freezes the minimum `0.1.0` robot-facing CLI contract for Roger
Reviewer. It narrows the canonical plan's robot-mode conventions into a
planning-stage support contract that later CLI implementation beads can follow
without inventing new machine-facing behavior ad hoc.

Authority order for this topic:

1. `AGENTS.md`
2. `docs/PLAN_FOR_ROGER_REVIEWER.md`
3. this document

If implementation pressure exposes a conflict, the canonical plan wins unless a
plan update is made explicitly.

## Scope

This contract defines:

- the minimum `0.1.0` command shortlist that must support `--robot`
- which robot output formats are guaranteed versus optional
- stdout/stderr and exit-code behavior
- the common response envelope for machine-readable outputs
- stable payload commitments for the first command set

This contract does not define:

- the internal storage schema for every Roger entity
- full TOON enablement for every command
- future harness-native command surfaces beyond ordinary `rr` semantics
- the in-session `rr agent ...` worker transport

## CLI-Wide Rules

- `--robot` enables machine-facing mode. It does not create a separate workflow.
- `--robot` applies to operator-facing Roger commands only.
- In robot mode, stdout carries only the requested machine-readable payload.
- In robot mode, stderr carries diagnostics, warnings, and progress text meant
  for humans operating the command.
- `--robot-format json` is the safe default and is guaranteed for every command
  in the `0.1.0` shortlist below.
- `--robot-format compact` is a stable optional summary format only for
  read-mostly commands that return compact lists or status summaries.
- `--robot-format toon` is not part of the guaranteed `0.1.0` baseline. A later
  bead may enable it for specific commands only after Roger-owned smoke tests
  prove the payload shape and fallback behavior.
- Robot mode must expose degraded, partial, blocked, or repair-needed states
  explicitly rather than flattening them into plain success prose.

Boundary rule:

- the review-worker transport lives under `rr agent ...`
- `rr agent` is not part of the `--robot` contract and must remain a separate
  machine-facing surface with its own binding and schema rules

## `0.1.0` Command Shortlist

These commands must support `--robot` in `0.1.0`.

| Command | Required robot formats | Optional robot formats | Why it is in scope |
| --- | --- | --- | --- |
| `rr status` | `json` | `compact` | Canonical machine-readable session/attention probe |
| `rr sessions` | `json` | `compact` | Global session-finder surface for automation |
| `rr findings` | `json` | `compact` | Primary structured findings inspection surface |
| `rr search` | `json` | `compact` | Search and retrieval surface for agent workflows |
| `rr review --dry-run` | `json` | none | Safe launch-intent planning without starting mutation-capable work |
| `rr resume --dry-run` | `json` | none | Safe resume planning and ambiguity reporting |
| `rr robot-docs guide` | `json` | `compact` | Stable machine-facing usage overview |
| `rr robot-docs commands` | `json` | `compact` | Stable command inventory |
| `rr robot-docs schemas` | `json` | `compact` | Stable schema-discovery surface |
| `rr robot-docs workflows` | `json` | `compact` | Stable workflow and sequencing hints |

Commands outside this list may add robot mode later, but they must not claim
stable `0.1.0` machine-contract support unless a follow-up planning or ADR step
freezes their payloads explicitly.

Clarification for current `0.1.0` scope:

- `rr memory rebuild` is deferred and is not part of the shipped CLI surface in
  this slice; automation should treat `rr search` as the only supported
  memory/search retrieval command until a later command-surface bead lands.

## Format Commitments

### Guaranteed in `0.1.0`

- `json` for every shortlisted command
- deterministic top-level envelope fields
- deterministic exit-code mapping

### Optional in `0.1.0`

- `compact` for the shortlist above where marked optional
- command-specific summary condensation, as long as it is still structured and
  versioned

### Not guaranteed in `0.1.0`

- `toon`
- robot mode on mutating post-approval commands
- robot mode on every future subcommand added to `rr`

## Exit Semantics

Robot commands must use deterministic exit codes:

| Exit code | Meaning | Expected payload behavior |
| --- | --- | --- |
| `0` | command completed successfully, including an empty-but-valid result | emit a complete `json` or `compact` payload on stdout |
| `2` | invalid arguments or invalid format request | emit a structured error payload on stdout and concise diagnostics on stderr |
| `3` | blocked precondition | emit a structured blocked payload describing the missing prerequisite |
| `4` | repair-needed state | emit a structured repair-needed payload with next-step guidance |
| `5` | degraded or partial success | emit a structured payload with `outcome` showing `degraded` or `partial` |
| `1` | unexpected internal failure | emit a structured error payload if possible; otherwise stderr is authoritative |

`0` must not mean "some text was printed". It means the command reached a valid,
inspectable result state.

## Common Envelope

Every `json` robot payload in `0.1.0` must expose this top-level envelope:

```json
{
  "schema_id": "rr.robot.status.v1",
  "command": "rr status",
  "robot_format": "json",
  "outcome": "complete",
  "generated_at": "2026-03-30T00:00:00Z",
  "exit_code": 0,
  "warnings": [],
  "repair_actions": [],
  "data": {}
}
```

Field rules:

- `schema_id` is required and versioned per command payload.
- `command` is required and reflects the invoked command shape.
- `robot_format` is required and records the actual emitted format.
- `outcome` is required and must be one of `complete`, `empty`, `partial`,
  `degraded`, `blocked`, `repair_needed`, or `error`.
- `generated_at` is required and must be UTC RFC 3339.
- `exit_code` is required and must match the process exit status.
- `warnings` is required and lists non-fatal issues or degraded-mode notes.
- `repair_actions` is required for `repair_needed` and optional-but-present as
  an empty list otherwise.
- `data` is required and contains the command-specific payload.

## Command Payload Commitments

### `rr status`

Purpose: machine-readable summary of the current Roger state in the working
directory or explicitly targeted session.

Stable `data` fields:

- `repo`
- `session`
- `target`
- `attention`
- `findings`
- `drafts`
- `continuity`

Example:

```json
{
  "schema_id": "rr.robot.status.v1",
  "command": "rr status",
  "robot_format": "json",
  "outcome": "complete",
  "generated_at": "2026-03-30T00:00:00Z",
  "exit_code": 0,
  "warnings": [],
  "repair_actions": [],
  "data": {
    "repo": {
      "root": "/path/to/repo",
      "branch": "feature/example"
    },
    "session": {
      "id": "rs_123",
      "resume_mode": "opencode_bound"
    },
    "target": {
      "provider": "github",
      "pull_request": 42
    },
    "attention": {
      "state": "awaiting_user_input",
      "updated_at": "2026-03-30T00:00:00Z"
    },
    "findings": {
      "total": 6,
      "needs_follow_up": 2
    },
    "drafts": {
      "awaiting_approval": 1
    },
    "continuity": {
      "tier": "tier_b",
      "resume_locator_present": true
    }
  }
}
```

### `rr sessions`

Purpose: machine-readable global session-finder output.

Stable `data` fields:

- `items`
- `count`
- `truncated`

Each session item must include:

- `session_id`
- `repo`
- `target`
- `attention_state`
- `updated_at`

### `rr findings`

Purpose: structured access to current findings without scraping human TUI or CLI
output.

Stable `data` fields:

- `session_id`
- `items`
- `count`
- `filters_applied`

Each finding item must include:

- `finding_id`
- `fingerprint`
- `title`
- `triage_state`
- `outbound_state`
- `evidence_count`

### `rr search`

Purpose: structured retrieval results over Roger's local-first search surface.

Stable `data` fields:

- `query`
- `requested_query_mode`
- `resolved_query_mode`
- `retrieval_mode`
- `mode`
- `scope_key`
- `candidate_included`
- `allow_project_scope`
- `allow_org_scope`
- `items`
- `count`
- `truncated`
- `degraded_reasons`
- `scope_bucket`
- `lane_counts`

Search contract notes:

- `requested_query_mode` is the ingress intent supplied by the operator,
  baseline, or worker
- `resolved_query_mode` is the concrete planner posture Roger actually
  executed after resolving `auto` or omitted intent
- `retrieval_mode` is the engine path Roger actually executed
- `mode` remains a backwards-compatible alias for `retrieval_mode` during the
  `0.1.x` transition, but machine consumers should migrate to the explicit
  fields above
- robot search output is required to preserve planner truth, not just final hit
  ranking

Each search item must include:

- `kind`
- `id`
- `title`
- `score`
- `memory_lane`
- `scope_bucket`
- `citation_posture`
- `surface_posture`
- `locator`
- `snippet`

Search items are the stable robot projection of the canonical `RecallEnvelope`
contract. They may be thinner than the full envelope, but they must preserve
lane, scope, degraded truth, and provenance semantics rather than inventing a
separate robot-only meaning.

Optional search item fields when available:

- `memory_lane`
- `trust_state`
- `citation_posture`
- `surface_posture`
- `explain_summary`

When semantic retrieval is unavailable, `mode` must report a degraded lexical
path explicitly rather than implying full hybrid retrieval.

### `rr review --dry-run`

Purpose: report what Roger would do to start a review without actually starting
the review flow.

Stable `data` fields:

- `resolved_target`
- `session_action`
- `instance_plan`
- `preflight`

`session_action` must distinguish at least `create`, `reuse`, `ambiguous`, and
`blocked`.

### `rr resume --dry-run`

Purpose: report what Roger would do to resume a review without mutating session
state.

Stable `data` fields:

- `resume_candidates`
- `selection_rule`
- `instance_plan`
- `preflight`

If multiple plausible matches exist, the payload must report ambiguity rather
than silently selecting one.

### `rr robot-docs *`

Purpose: machine-readable discovery surface so agents do not depend on prose
scraping or out-of-band examples.

Stable `data` fields:

- `topic`
- `version`
- `items`

`rr robot-docs schemas` must expose the current `schema_id` inventory for every
shortlisted command in this contract.

## Degraded, Partial, and Repair-Needed Handling

- `partial` means Roger produced a usable subset of the requested result but had
  to omit part of it, such as unresolved malformed provider output.
- `degraded` means Roger completed the command using a weaker path that remains
  truthful, such as lexical-only search when semantic assets are unavailable.
- `repair_needed` means automation must not assume the state is safe to ignore;
  the payload must include at least one concrete `repair_action`.
- `blocked` means the request could not proceed because a precondition was not
  satisfied, such as an ambiguous resume target that needs explicit selection.

## Compact Format Rules

Where `compact` is supported, it must remain structured and versioned. In
`0.1.0`, that means:

- one object per line or one compact JSON document
- no ANSI, no banners, no conversational text
- the same `schema_id`, `outcome`, and `exit_code` semantics as full `json`

If a command cannot preserve these guarantees in `compact`, it must reject the
format request with exit code `2` rather than silently drifting.

## TOON Policy

- `toon` is reserved as a future optional format selector.
- No command is required to support `toon` in `0.1.0`.
- A future bead may promote `toon` to supported status for individual commands
  only after smoke tests prove shape stability and fallback behavior.

## Acceptance Mapping

This contract satisfies `rr-005.4` by freezing:

- the `0.1.0` robot-command shortlist
- guaranteed versus optional output formats
- stdout/stderr and deterministic exit semantics
- explicit degraded, partial, blocked, and repair-needed handling
- stable schema commitments and examples for the first machine-readable outputs
