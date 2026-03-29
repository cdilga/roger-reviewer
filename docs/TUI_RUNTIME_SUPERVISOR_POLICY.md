# Roger TUI Runtime Supervisor Policy

This document is the implementation-facing contract for `rr-006.2`. It narrows
the architectural decisions in ADR-008 into concrete defaults for queue classes,
queue limits, cancellation rules, time budgets, refresh cadence, and
serialization boundaries.

Authority:

- `docs/adr/008-tui-runtime-and-concurrency-boundary.md` is the parent ADR
- This document narrows the open questions explicitly left in ADR-008
- `AGENTS.md` and `docs/PLAN_FOR_ROGER_REVIEWER.md` remain canonical

---

## Runtime Shape (From ADR-008, Fixed)

The settled decisions that this document builds on:

- TUI and app-core run in-process in `0.1.x`
- One FrankenTUI foreground thread owns UI input, reducers, and rendering
- One dedicated async executor thread handles I/O-bound work
- One bounded CPU-worker pool handles indexing and heavier compute
- Roger-owned channels between the foreground loop and the background supervisor
- No resident broker or daemon in steady state

---

## Queue Classes

Roger should define three named background queue classes. Each class has a
separate queue, limits, and cancellation semantics.

### Class A: Harness Work

Jobs: harness session start/reopen, prompt stage execution, `rr return` session
rebind, ResumeBundle writes.

Default limits:

- **Queue depth**: 1 active job per `ReviewSession`; no concurrent harness jobs
  for the same session
- **Pending queue depth**: 1; a second enqueue for the same session replaces
  the pending job rather than queuing behind it
- **Concurrency across sessions**: allowed; separate sessions may run harness
  jobs concurrently
- **Timeout**: 5 minutes per harness job; configurable down to 1 minute, up to
  15 minutes
- **Cancellation**: explicit only; harness jobs are not cancelled by background
  idling, only by explicit user cancel, session close, or supervisor shutdown
- **On timeout**: job moves to `timed_out` state; `ReviewRunState` is updated to
  `interrupted`; raw output captured so far is preserved

### Class B: Bridge and GitHub Adapter Work

Jobs: Native Messaging bridge traffic, GitHub read requests, extension status
callbacks, launch intake normalization.

Default limits:

- **Queue depth per session**: 5 active jobs; new enqueue is rejected with
  `queue_full` error when at capacity
- **Timeout**: 15 seconds per job
- **Cancellation**: jobs are cancelled on session close or when a newer
  superseding request arrives (for status callbacks)
- **On timeout**: job fails; calling surface receives `timed_out` error; no
  retry without explicit re-request

### Class C: Indexing and Search Maintenance Work

Jobs: Tantivy index updates, embedding generation, sidecar rebuild, artifact
digest computation.

Default limits:

- **Queue depth**: 10 pending jobs; oldest jobs are dropped when at capacity
  (indexing is rebuildable so dropping is safe)
- **CPU-worker pool size**: 2 workers by default; configurable 1–4
- **Timeout**: 60 seconds per indexing job
- **Cancellation**: jobs are interruptible; cancellation is cooperative and
  completes at the next checkpoint
- **Priority**: Class C work yields to Class A and Class B work; the supervisor
  must not starve Class A or Block the foreground loop via Class C
- **On failure or timeout**: log the failure; next review cycle will re-queue if
  needed; never block a review for indexing

---

## Cancellation Rules

These cancellation rules apply across all classes:

1. **Explicit cancel** (user action or `rr cancel`): allowed for any queued or
   active job; harness jobs must honour cancellation within 500ms of receiving
   the cancel signal
2. **Session close**: all jobs bound to a closing session are cancelled; ongoing
   Class A jobs write `interrupted` state before stopping
3. **Process shutdown (SIGTERM)**: all active jobs receive a cancel signal; the
   supervisor waits up to 2 seconds for graceful completion before forcing exit;
   in-flight harness I/O is interrupted; partial output is preserved if already
   written to the canonical store
4. **Approval invalidation** (instance retarget, branch change): does not
   cancel harness jobs; it only invalidates outbound approval tokens; the
   supervisor must emit an `attention_state_changed` event with reason
   `approval_invalidated` and must not silently continue an outbound action
5. **Class C preemption**: Class C jobs are preempted when a Class A or Class B
   job needs the CPU-worker pool; preemption is cooperative; the preempted job
   resumes when capacity frees

---

## Serialization Rules

The following operations must serialize (one-at-a-time per session):

| Operation | Reason |
|-----------|--------|
| Outbound draft posting (`PostedAction` write) | Post-time revalidation; approval token consumed exactly once |
| Approval token creation or revocation | Prevents concurrent approval/invalidation races |
| `ReviewRunState` transitions | Prevents split-brain run state |
| Finding triage state transitions on the same finding | Prevents last-write-wins conflicts |

The following operations may run concurrently:

| Operation | Notes |
|-----------|-------|
| Harness jobs across different `ReviewSession`s | Independent sessions |
| Class C indexing across different repos or sessions | Derived state, rebuildable |
| Read-only queries (findings list, draft list, status) | No write contention |
| Bridge/GitHub reads | Idempotent; no local mutation |

Serialization mechanism: `row_version` optimistic checks for finding and
draft mutations; Roger-owned session lease (a single async-mutex or database
advisory lock per session) for state-transition paths.

---

## Refresh Cadence

### Same-Process Wake (Class A and B completions)

Background job completions send a direct wake signal over a Roger-owned channel
to the FrankenTUI foreground loop. No polling delay.

Rule:

- wake signals are non-blocking; if the foreground loop is busy, the signal is
  queued in the Roger-owned channel buffer (suggested buffer: 16 events)
- the foreground loop drains the channel at the start of each tick

### Cross-Process Event Polling

The active TUI uses a `last_seen_event_id` cursor against the canonical event
stream for updates from other Roger processes (CLI, bridge host, agent runs).

Default polling cadences:

| TUI State | Poll Interval |
|-----------|--------------|
| Active (user input in last 30s) | 500ms |
| Idle (no user input, review running) | 2s |
| Background (TUI in background / minimized) | 10s |
| Session closed | No polling |

Rules:

- polling must be bounded; each poll reads at most 50 new events from the cursor
- if a poll returns 50 events (at capacity), the next poll fires immediately to
  drain remaining events rather than waiting the full interval
- cross-process wake using OS signals or a simple in-process notification pipe
  may supplement polling in `0.1.x` but must not become a daemon dependency
- polling state survives TUI state transitions; the cursor is durable across
  foreground/background switches

---

## Daemonless Requirement

Background work must stay behind the in-process supervisor or short-lived helper
processes. Roger must not require a resident broker, event fabric, or
long-running background service for normal operation.

Specific prohibitions:

- no always-on indexing daemon
- no always-on bridge listener process
- no always-on inter-session event broker
- the bridge host is launched on-demand per invocation (Native Messaging host
  model), not kept alive between invocations

The supervisor itself lives inside the `rr` process. When `rr` exits, all
supervised work is cancelled.

---

## Which Operations Route to TUI vs. Bounded Machine-Readable Output

This resolves the third open question from ADR-008.

**Always route to TUI:**

- approval of outbound drafts
- posting to GitHub
- finding triage that changes `outbound_state`
- explicit `rr return` (completion routes back to TUI)
- session conflict or approval invalidation requiring human decision

**May return bounded machine-readable output without TUI:**

- `rr status` (`--robot` mode): structured status summary
- `rr findings` (`--robot` mode): structured findings list
- `rr refresh` (`--robot` mode): trigger a refresh, return job id and initial
  status
- `rr resume` (`--robot` mode): start or resume a session, return session id
- health-check and diagnostic commands

**Never allowed in harness commands (always route to TUI or `rr`):**

- approval flows
- posting flows
- finding state changes that affect `outbound_state`
- any mutation that changes the approval-safety posture

---

## Implementation Notes

- The supervisor is not a general-purpose work queue; it owns three named
  classes with explicit semantics. Do not extend it with ad hoc fourth classes
  without updating this policy.
- Cancellation tokens must flow from the supervisor to harness job handles;
  harness adapters are responsible for polling the token at safe checkpoints.
- Cross-process write conflicts resolved by `row_version` must surface as
  explicit `conflict` errors to the calling surface, not silently swallowed.
- The event cursor for cross-process polling must be persisted so a TUI restart
  does not re-process stale events.
