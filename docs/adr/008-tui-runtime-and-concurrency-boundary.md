# ADR 008: TUI Runtime and Concurrency Boundary

- Status: accepted
- Date: 2026-03-29

## Context

Roger needs:

- a Rust TUI because Roger's accepted local runtime is Rust-first and the TUI
  must share local ownership with app-core
- multiple concurrent entrypoints such as TUI, CLI, bridge host, and
  agent-owned invocations
- truthful daemonless behavior in steady state
- same-session conflict safety and cross-session concurrency

That creates an architectural fork:

- split the TUI and app-core into separate local processes with a general IPC
  boundary
- keep the TUI and app-core in-process and solve the real concurrency problem at
  the canonical store and worker-execution layers

The foreground UI loop also matters here because `FrankenTUI`, Roger's current
TUI dependency, is synchronous rather than tokio-native.

## Decision

Roger should keep the TUI and app-core in-process in `0.1.x`.

Recommended runtime shape:

- the TUI process hosts the Roger command router, domain access, and view-model
  logic in-process
- the Roger TUI runtime owns the foreground synchronous event loop for that
  process; in `0.1.x`, that loop is implemented via FrankenTUI
- long-running or latency-tolerant work runs behind a Roger-owned background
  supervisor rather than on the TUI foreground loop
- CLI, bridge-host, and agent-owned entrypoints may run as separate Roger
  processes against the same canonical store
- cross-process safety is enforced through session leases or optimistic
  `row_version` checks plus append-only events, not through a resident broker
  service

### Primary executable topology

Roger should ship as one primary `rr` binary with internal mode boundaries in
`0.1.x`.

Recommended rule:

- `rr` is the canonical executable
- TUI, CLI, bridge-host, robot-facing commands, and helper flows such as return
  are modes or subcommands of that same primary binary
- a separate helper executable is allowed later only if a platform-specific
  packaging or host-registration constraint clearly justifies it
- Roger should not assume a small fleet of cooperating local binaries as its
  baseline architecture

### In-process routing rule

The first implementation should use typed in-process routing on the hot path.

Recommended rule:

- TUI actions call Roger-owned Rust router/domain interfaces directly
- do not introduce a mandatory local IPC boundary between the TUI and core in
  `0.1.x`
- do not introduce an internal message-bus abstraction merely because a future
  cross-process split is imaginable
- keep the logical operation names aligned with the stable external envelope
  family so later extraction remains possible

### Background execution model

Roger should use a small supervised execution model rather than ad hoc
per-request thread spawning.

Recommended model:

- one foreground UI thread for input, reducers, and rendering
- one dedicated async executor thread for I/O-bound work such as harness I/O,
  bridge traffic, GitHub adapter requests, and other async-capable jobs
- one bounded CPU-worker pool or dedicated indexing/search-maintenance workers
  for embeddings, indexing, and heavier compute tasks
- Roger-owned channels between the foreground loop and the supervisor
- append-only event writes plus canonical-row updates as the cross-process truth

### Wake and refresh policy

Roger should use immediate local wake plus bounded store-backed refresh in
`0.1.x`.

Recommended rule:

- same-process background completions send a direct local wake signal back to
  the TUI process over a Roger-owned channel
- important completions and cross-process updates also append durable event rows
  to the canonical store
- the active TUI keeps a `last_seen_event_id` or equivalent cursor and performs
  bounded polling against the event stream for cross-process updates
- do not make filesystem notification, a resident broker, or a richer event
  fabric a `0.1.x` requirement

### Minimum external envelope

Roger should freeze one small versioned envelope family for real external
boundaries in `0.1.0`.

Recommended fields:

- `protocol_version`
- `kind`: `request` | `response` | `event`
- `name`
- `correlation_id`
- `source_surface`: `tui` | `cli` | `extension` | `external_link` |
  `harness_command` | `agent` | `system`
- `session_id` when bound
- `run_id` when bound
- `instance_id` when relevant
- `ts`
- `payload`
- `ok` for responses when relevant
- `error` for failure cases

Recommended initial logical names:

- requests: `resume_session`, `refresh_review`, `show_findings`,
  `ask_clarification`, `open_drafts`, `return_to_roger`
- events: `session_updated`, `findings_updated`, `drafts_updated`,
  `attention_state_changed`, `background_job_changed`

Boundary rule:

- this envelope is mandatory at real external edges such as browser bridge,
  robot-facing outputs where committed, and later cross-process adapters
- it is not a required internal serialized transport between the TUI and
  app-core in `0.1.x`
- TOON, protobuf, and a local IPC transport are all optional later tools, not
  required runtime boundaries

### Boundary rule

The first cross-process boundary Roger should invest in is the external edge:

- browser bridge envelopes
- robot CLI outputs
- harness command bindings
- stored events and canonical rows

It should **not** start by turning the TUI into a thin remote client of a
separate app-core service.

## Why

- the hard problem is concurrent processes against shared durable state, not
  remoting button clicks or keypresses from the TUI to a sibling process
- in-process TUI keeps the hot path lower-latency and easier to debug
- it avoids inventing a second failure mode where the local UI and local core
  can desynchronize even inside one operator session
- it preserves the daemonless architecture more honestly
- it still leaves room for later editor or client integrations because those can
  target Roger-owned external contracts rather than an assumed local daemon API
- it avoids overfitting to richer event-fabric exploration that is not required
  for `0.1.x`

## Consequences

- Roger now commits to a dedicated async executor thread plus bounded CPU-worker
  execution as the default background model
- Roger now commits to local wake signals plus bounded event-stream polling as
  the `0.1.x` refresh mechanism
- store-backed conflict detection becomes more important than local IPC design
- a future extracted app-core service remains possible, but only after a
  specific product pressure justifies it

## Open Questions

- what queue limits, cancellation rules, and time budgets should the background
  supervisor enforce by default?
- what polling cadence should Roger use for cross-process event refresh in
  active versus idle TUI states?
- which logical operations should always bounce the user into the TUI versus
  returning a bounded machine-readable or harness-local result?

## Follow-up

- define the concrete worker/task-supervision contract
- define the concrete TUI refresh cadence and wake policy for external session
  updates
- publish the v1 external envelope schemas and examples
- add tests for same-session conflict handling across multiple Roger processes
