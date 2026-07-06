# Self-Exploration And Review Workbench Recovery Plan

Status: active recovery plan
Class: implementation support plan / bead creation packet
Audience: maintainers and agents restoring Roger's lived review workflow

Authority:

- `AGENTS.md` remains the operational authority.
- `docs/PLAN_FOR_ROGER_REVIEWER.md` remains the canonical product plan.
- `docs/TUI_WORKSPACE_AND_OPERATOR_FLOW_CONTRACT.md` remains the TUI contract.
- `docs/HARNESS_SESSION_LINKAGE_CONTRACT.md` remains the resume and dropout contract.
- `docs/SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md` remains the memory/search contract.
- This document translates the current user-visible recovery gap into implementation tracks and beads.

## Purpose

Roger currently has many of the right local primitives, but the operator
experience still feels broken when used end to end. This plan defines the work
required for a user to self-explore Roger, launch from the browser extension,
resume cleanly, inspect findings and memory posture, author review feedback in
GitHub context without bypassing Roger approval, and preserve final submissions
for later indexing and improvement.

This is a recovery plan for lived usability and proof, not a new product
direction.

## Current Evidence

The current repo state shows these gaps.

- The TUI cockpit exists, but it is still a text-first shell over sessions,
  findings, drafts, timeline, and prior-review search. It does not yet feel like
  the dense memory-aware operator workbench described by the TUI contract.
- The richer DTOs in `packages/app-core/src/tui_shell.rs` describe memory,
  active sessions, jobs, supervisor state, draft review, and search history, but
  the live `rr tui` path is driven by `packages/tui` projections and does not
  surface the full operational machinery.
- `rr search --repo cdilga/roger-reviewer --query "resume session findings"
  --robot` currently degrades to recovery scan with no hits and reports missing
  or unverified semantic assets. That is honest, but the UI does not make the
  memory/search posture visible enough.
- Existing extension setup can pass `rr extension doctor --browser edge`, but
  the PR detail panel still dispatches headless `rr review/resume --robot`
  launches and does not drop the operator into an obvious live provider session.
- The current open-terminal concept is under-specified: Roger needs a configured
  default terminal target, platform-specific launch support truth, and an exact
  session handoff rather than a broad repo/PR resume that may pick the wrong
  session.
- A live extension-launched session can become hard to inspect from the repo:
  `rr status --session <id> --robot` and `rr resume --session <id> --robot`
  can block because the session launch binding was recorded with a cwd under
  `NativeMessagingHosts`. That means the stale-binding safety invariant is
  being fed the wrong launch-surface context.
- When several Roger sessions exist for the same PR, the browser and TUI flows
  do not yet provide a first-class session picker. Any implicit "latest session"
  behavior is unsafe for terminal handoff, findings display, draft authoring, and
  resume.
- The CLI has local `draft`, `approve`, and `post` primitives, but there is no
  comfortable authoring surface for reviewing and editing proposed final review
  comments in GitHub context.
- Posted actions and outcome events exist, but the product does not yet expose a
  coherent final-submission artifact lane that captures edited drafts, approval
  decisions, posting results, usefulness feedback, and later memory-extraction
  candidates as one inspectable history.
- The open bead graph is mostly extension PR-listing work plus one live-unproven
  in-session memory-flow bead. It does not currently represent the full recovery
  work needed for the above user journeys.

## Target User Journeys

### Journey 1: Self-explore locally

The operator can run `rr tui` or open it from a browser-launched flow and see:

- all relevant sessions for the repo and PR
- whether each session is launched, running, waiting for findings, failed,
  awaiting approval, or stale
- provider tier, continuity state, latest launch attempt, and resume/open
  terminal commands
- a session picker when more than one session exists for the current repo/PR,
  showing enough status to choose intentionally
- worker task status and whether an in-session agent has actually submitted
  stage results
- memory/search status including semantic asset posture, retrieval mode,
  degraded flags, lane counts, and baseline snapshot
- findings, draft batches, posted actions, and final submission history

An empty findings list must explain why it is empty and what to do next.

### Journey 2: Launch or resume from GitHub and land somewhere real

From a GitHub PR page or PR listing page, the operator can:

- start a new Roger review or resume an existing one
- choose the intended Roger session when several sessions exist for the same PR
- open the exact selected session in the operator's configured terminal when
  interactive work is needed
- see bounded local status without the extension pretending to be the source of
  truth
- recover cleanly if Native Messaging is unavailable, forbidden, stale, or
  bound to the wrong local context
- return to the local TUI or CLI with the same session id and target identity

Browser-launched sessions must not record `NativeMessagingHosts` as a repo-local
binding that later blocks explicit session inspection from the real repo.

Terminal handoff must be session-specific. A browser action for session
`session-abc` must open `session-abc`, not a best-effort resume for the newest
session on the same PR. If there is no selected session and more than one
candidate exists, Roger must ask the operator to choose.

### Journey 3: Follow findings and author feedback in GitHub context

The extension can show Roger findings and draft comments in the GitHub PR page
where the operator is already reading the code review context.

Rules:

- the extension may display local findings, draft batches, and draft revisions
- the extension may offer a local editor for Roger-owned draft bodies
- every edit is saved to the local Roger store as a draft revision or equivalent
  canonical object
- the extension must not post to GitHub directly, click GitHub submit controls,
  or use raw `gh` write operations
- approval and posting remain Roger-mediated and visibly elevated through the
  TUI or CLI approval/posting model

### Journey 4: Preserve final submissions for later improvement

After a review loop, Roger can answer:

- which findings were accepted, ignored, resolved, or superseded
- which draft text was first generated
- which text the operator edited
- which exact payload was approved
- what Roger attempted to post
- what GitHub accepted or rejected
- which final bodies and outcomes should become searchable evidence
- which repeated patterns should become candidate memory for review

The canonical store remains the source of truth. Search sidecars and semantic
assets are rebuildable projections.

## Required Delivery Tracks

### Track A: Bridge launch context and resume recovery

Fix the bridge/CLI launch boundary so browser-originated launches are recorded
with a truthful launch surface and do not poison repo-local session bindings.

Required outcomes:

- Native Messaging launches are distinguishable from ordinary repo-local CLI
  launches in the stored `LaunchSurface`.
- Bridge-dispatched `rr review/resume` does not bind the browser host cwd as the
  repo/worktree root.
- Explicit `rr status --session <id>` and `rr findings --session <id>` remain
  usable for read-only inspection even when the launch binding is stale, while
  mutation-capable resume/return paths still fail closed or require rebind.
- `Open in terminal` is implemented as a first-class bridge mode with a bounded
  ack, exact session id, and platform-specific support truth.
- Roger has an operator-visible terminal preference, with a default/fallback
  order that can be inspected by `rr doctor` and the extension doctor path.
- Opening a session in terminal uses the configured terminal target and fails
  loudly if the selected terminal is unavailable rather than silently falling
  back to a different terminal.
- A repo/PR resume request that resolves to multiple sessions returns a
  disambiguation envelope instead of guessing.
- Existing stale binding safeguards for real cross-worktree reuse remain intact.

### Track B: Real session progress and in-session worker proof

Close the gap between "session launched" and "Roger review actually happened".

Required outcomes:

- the TUI and CLI can show whether a launched session has no worker task, a
  pending task, a running task, a completed task, or a failed task
- the open live-unproven in-session memory-flow bead is completed with replayable
  evidence from a real provider session
- a review with zero findings is distinguishable from a review that never ran
- the first-run flow creates enough task/context/status evidence that an
  operator can tell what is missing without reading the database

### Track C: TUI operator workbench completion

Upgrade the live TUI from "thin text cockpit" to the practical workbench
promised by the support contracts.

Required outcomes:

- home/session screens include status strips for provider, continuity, launch
  attempts, worker tasks, drafts, posting, memory, and semantic assets
- repo/PR views include a session picker when multiple sessions exist, with
  provider, creation/update time, launch surface, worker progress, findings,
  drafts, posting state, and stale-binding indicators
- findings and draft screens support efficient selection, review, editing
  handoff, and elevated approval with no hidden mutation
- Search/History shows recall envelopes with query mode, retrieval mode, scope,
  lane, trust posture, degraded reasons, and baseline snapshot
- memory review and promotion requests are visible as operator work, not hidden
  metadata
- dropout/open-terminal/return actions are visible and explain the current
  session binding truth
- empty and degraded states explain the next useful action

### Track D: Extension launch, view, and local authoring

Keep the extension thin and non-authoritative, but make it useful in the GitHub
web UI.

Required outcomes:

- existing PR-detail launch/status remains reliable
- PR-listing start-review beads land without regressing PR detail
- the extension exposes a session picker when more than one local Roger session
  matches the current PR
- `Open in terminal` works on macOS first, uses the selected/default configured
  terminal target, opens the exact selected session, and fails honestly elsewhere
- the PR page can show bounded local findings/draft summaries from Roger
- the PR page can edit/save local draft revisions through the bridge without
  posting
- approval/posting controls are not added to the extension unless they are
  explicit local handoffs to TUI/CLI, never direct GitHub mutation

### Track E: Final submission and memory capture

Make review outputs useful for later search and improvement.

Required outcomes:

- draft creation, draft edits, approval, post attempts, post results, and final
  accepted bodies are captured as canonical outcome history
- final submission snapshots are linked to findings, draft batches, posted
  action items, remote identifiers, and payload digests
- `rr search`, TUI Search/History, and worker memory search can retrieve final
  submission evidence with provenance
- memory extraction creates candidate review requests rather than silently
  promoting claims
- usefulness and harmfulness feedback can be recorded without rewriting history

### Track F: User-journey validation

Prove the product behavior in one stitched self-exploration journey rather than
only unit or contract slices.

Required outcomes:

- a deterministic fixture can start from extension setup/doctor, open a browser
  PR page, launch or resume a session, open terminal/TUI, inspect status and
  memory posture, view findings, author a draft, save an edit, approve locally,
  and preserve final-submission evidence
- the test must preserve artifacts: browser transcript/screenshots, bridge
  envelopes, session id, launch attempts, store diff, and search/memory result
  envelopes
- live-provider proof is separately ticketed when deterministic tests use a
  stub or fixture provider

## Bead And Swarm Execution Model

This recovery should be implemented as self-contained beads, not as one large
"make Roger good" task.

Suggested swarm tracks:

- Track A can run first and should be treated as a blocker for extension UX.
- Track B can run in parallel after Track A design is stable, but its live proof
  must use Agent Mail to coordinate any terminal/provider reservations.
- Terminal preference and exact-session handoff should run with Track A before
  the visible web/TUI open-terminal affordances claim reliability.
- Session selection/disambiguation should land before Track C and the
  GitHub-context parts of Track D claim that findings, drafts, or terminal
  handoff refer to the intended review.
- Track C should wait for Track A's session-binding truth, Track B's status
  vocabulary, and session-picker semantics so the TUI does not invent fake
  state.
- Track D can continue PR-listing work immediately, but GitHub-context authoring
  should depend on Track E's local draft revision contract.
- Track E should land before claiming indexing/improvement behavior.
- Track F closes the recovery packet and is the release-quality proof.

Agents should use Agent Mail with bead ids as thread ids, reserve files before
editing, and use NTM only after this bead packet is in place. The initial swarm
should be small enough to avoid overlapping `packages/cli/src/lib.rs`,
`packages/storage/src/lib.rs`, and extension content-script work without clear
reservations.

## Definition Of Recovered

Roger reaches this recovery bar when:

- a browser-launched review can be inspected and resumed from the repo without a
  stale NativeMessagingHosts binding blocking explicit session reads
- the extension can reliably launch or open terminal from PR detail and PR list
  pages
- the extension and TUI ask the operator to choose when multiple sessions match
  a PR, and all findings/draft/terminal actions are tied to the selected session
- the operator can configure the default terminal Roger opens, and doctor/status
  surfaces show whether that terminal handoff is supported on the current machine
- `rr tui` shows enough session, worker, memory, finding, draft, and posting
  state for a user to self-explore without reading implementation docs
- GitHub-context viewing and local draft authoring work without bypassing Roger
  approval/posting
- final edited submissions and outcomes are durable and searchable evidence
- a stitched proof artifact demonstrates the above end to end
