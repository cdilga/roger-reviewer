# TUI Runtime Supervisor Policy

This document closes `rr-006.2`.

It turns the accepted in-process runtime direction from
[`ADR 008`](/Users/cdilga/Documents/dev/roger-reviewer/docs/adr/008-tui-runtime-and-concurrency-boundary.md)
and the harness constraints from
[`HARNESS_SESSION_LINKAGE_CONTRACT.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/HARNESS_SESSION_LINKAGE_CONTRACT.md)
into a concrete `0.1.0` supervision policy.

## Scope

This policy fixes:

- the default queue classes Roger uses inside the in-process TUI or app-core runtime
- what may run concurrently and what must serialize
- cancellation and replace-latest behavior
- same-process wake behavior versus cross-process refresh
- bounded refresh cadence for active versus idle TUI states

This policy does not introduce a daemon, a resident broker, or a general local
IPC service.

## Core Rules

- Roger remains daemonless in steady state.
- The active TUI foreground loop stays synchronous and must not block on
  provider, bridge, indexing, or GitHub adapter I/O. In `0.1.x`, that means
  the Roger `FrankenTUI`-based foreground loop.
- Background work runs behind a Roger-owned supervisor and communicates back to
  the foreground loop through typed local channels plus durable event rows.
- Cross-process truth comes from the canonical Roger store, not in-memory task
  state.
- No bridge, browser, or agent entrypoint may bypass the same queueing and
  approval rules that apply to the TUI.

## Queue Classes

Roger should use five queue classes in `0.1.0`.

| Queue class | Examples | Default concurrency | Cancellation policy |
|-------------|----------|---------------------|---------------------|
| `session_control` | start review, resume, refresh, follow-up pass, clarification pass, draft regeneration | max 1 active per `ReviewSession`; max 4 active globally | explicit cancellation only once work has started; queued superseded refresh-like jobs may be dropped before start |
| `session_query` | load session overview, findings list, artifact preview, search in current context | max 8 active globally; soft cap 4 per session | replace-latest for navigational reads; stale queued jobs may be discarded |
| `outbound_post` | materialize final GitHub adapter payloads, perform approved post, record posted-action audit | max 1 active per `ReviewSession`; max 1 active globally by default | no implicit cancellation once the adapter call begins; preflight mismatch blocks instead of auto-retrying |
| `index_maintenance` | lexical rebuild, semantic generation, dirty-range compaction | max 1 active globally | cancel or defer freely when newer dirty generations supersede older queued jobs |
| `bridge_io` | Native Messaging request handling, launch-intent intake, read-safe status probes | max 4 active globally | replace-latest for identical status probes; launch intents are not dropped silently once admitted |

Rules:

- `session_control` is the highest-risk queue because it changes review state.
- `outbound_post` is separate from `session_control` so approval-gated writes
  stay visibly elevated and independently serializable.
- `session_query` stays cheap and preemptible; it must not accumulate stale UI
  work.
- `bridge_io` stays short-lived and read-safe unless it hands off a launch or
  refresh request into `session_control`.
- `index_maintenance` is derived-state work only. It must never own canonical
  review truth.

## Serialization Rules

- Only one `session_control` job may run for a given `ReviewSession` at a time.
- Only one `outbound_post` job may run for a given `ReviewSession` at a time.
- `session_control` and `outbound_post` may not run concurrently against the
  same session.
- Posting requires an approved local draft batch whose approval snapshot still
  matches the current session revision, finding revision, and target revision.
  If any of those drift, Roger must block and require renewed approval.
- Bridge-triggered start, resume, refresh, or open-locally actions must enqueue
  through the same supervisor path as TUI or CLI actions. There is no browser
  fast path.
- Index rebuilds may run concurrently with read-only queries, but Roger should
  defer or pause them when foreground `session_control` work would otherwise
  suffer noticeable contention.

## Wake and Refresh Model

### Same-process wake

- Background completions inside the same Roger process send an immediate local
  wake signal to the active TUI foreground loop.
- The wake signal is advisory. The canonical store and durable event rows remain
  the source of truth.

### Cross-process refresh

- Each TUI window keeps a durable event cursor such as `last_seen_event_id`.
- Cross-process changes are discovered by bounded polling against the canonical
  event stream.
- No resident broker, WebSocket server, or always-on helper is required for
  normal operation.

### Default poll cadence

| TUI state | Poll cadence |
|-----------|--------------|
| focused on an active session with background work or recent external activity | every 2 seconds |
| TUI open on a session, but no active local job | every 5 seconds |
| session finder, review home, or other non-session view | every 15 seconds |
| no active TUI process | no polling |

Rules:

- Polling should fetch only the next bounded event page, not rescan the whole
  store.
- Roger should cap a single refresh pass to a small bounded batch such as 100
  events before yielding back to the UI loop.
- If the store is unavailable, Roger must report degraded or blocked state
  truthfully rather than inventing liveness.

## Replace-Latest and Cancellation Rules

Replace-latest is allowed only for work that is fundamentally navigational or
supersedable.

Replace-latest by default:

- findings-list reload
- session-overview reload
- search query while the user is still typing
- artifact preview for a no-longer-focused selection
- bridge status probe for the same target

Explicit cancellation required once started:

- start review
- resume review
- refresh review
- follow-up pass
- clarification pass
- post approved drafts

Rules:

- Roger should never silently cancel or restart a post once the adapter write
  has begun.
- Roger should never treat a newer refresh request as permission to abandon an
  in-flight approval-sensitive transition. It must finish or fail the first job
  explicitly.
- If a user requests cancellation for an in-flight session-control task, the
  UI must surface that as a real state change, not as a hidden drop.

## Safety Constraints

- Review mode remains read-mostly by default.
- Provider-backed `session_control` work may read local files and run approved
  local commands within Roger's declared review posture, but it must not imply
  GitHub posting or arbitrary environment mutation.
- `outbound_post` is the only queue class allowed to cross the GitHub write
  boundary, and only after explicit human approval.
- Bridge and extension code may trigger launch or open flows, but they do not
  own mutation authority.
- Agent-owned entrypoints must use the same queue classes and approval gates as
  human-triggered entrypoints.

## Minimum Observability

Roger should make the supervisor inspectable enough that later TUI work does
not invent its own ad hoc status model.

Required observable fields:

- queue class
- job id
- session id when bound
- started or queued timestamp
- state: `queued`, `running`, `succeeded`, `failed`, `cancel_requested`,
  `cancelled`, or `blocked`
- reason summary for `failed` or `blocked`

These fields may surface through TUI status, event rows, or robot-readable
status output, but the vocabulary should stay Roger-owned and consistent.

## Acceptance Summary For `rr-006.2`

This policy fixes:

- the default queue classes Roger uses in `0.1.0`
- concurrency limits and serialization boundaries
- replace-latest versus explicit-cancel behavior
- same-process wake and cross-process refresh rules
- a bounded refresh cadence that preserves the daemonless architecture

That is enough to let `rr-006.4`, `rr-019`, and bridge-facing work proceed
without inventing their own runtime behavior.
