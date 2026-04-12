# Review Flow Matrix

This document turns the major Roger user flows into a reusable scenario matrix.
It is not the canonical architecture spec; the canonical architecture remains
[PLAN_FOR_ROGER_REVIEWER.md](/Users/cdilga/Documents/dev/roger-reviewer/docs/PLAN_FOR_ROGER_REVIEWER.md).
For the canonical command-surface expectations and the current user-flow
hardening priorities behind those flows, see
[`PLAN_FOR_ROGER_REVIEWER.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/PLAN_FOR_ROGER_REVIEWER.md).

Purpose:

- keep TUI, CLI, extension, and harness behavior aligned
- make happy paths, common variants, and failure/recovery paths explicit
- provide a stable source for integration-test selection and consistency checks
- use [`RELEASE_AND_TEST_MATRIX.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/RELEASE_AND_TEST_MATRIX.md)
  as the explicit provider/browser/OS coverage matrix for which flows must run
  where

## How to use this matrix

- treat each flow ID as a scenario family, not a single test
- implementation beads can reference flow IDs in acceptance criteria
- integration tests should cover the highest-risk happy path plus the most
  important degradation paths for each family
- if a new surface or feature cannot be mapped cleanly onto an existing flow,
  either add a flow or explain why it is intentionally out of scope

## Flow Families

### F00: Resolve Local Launch Profile and Terminal Topology

- Surfaces: CLI, local companion, TUI launcher
- Primary artifact: `LocalLaunchProfile`
- Happy path: Roger resolves the configured launch profile and opens the review
  in the intended local surface such as VS Code integrated terminal with NTM,
  a bare WezTerm window, or a WezTerm split
- Common variants: per-repo override, per-project override, one-off launch
  override from the browser or CLI, reuse existing pane/window vs create new
- Failure/recovery: requested terminal environment unavailable, muxer not
  installed, requested split target invalid, fallback to another local surface
  with truthful feedback
- Test intent: prove launch behavior is explicit, configurable, and not tied to
  one terminal workflow

### F01: Enter or Resume a Review Locally

- Surfaces: CLI, TUI
- Primary artifact: `ReviewSession`
- Happy path: user starts or resumes a session and lands in `Session Overview`
  or `Findings Queue`
- Common variants: multiple sessions for one PR, reopen after interruption,
  direct jump to drafts or search
- Failure/recovery: missing provider session, stale locator, raw-only state while
  structure generation is still pending
- Test intent: prove local launch and resume work without browser involvement

### F01.1: Reinvoke Roger in the Current Repo and Pick Up the Right Session

- Surfaces: CLI, TUI
- Primary artifact: repo-context-based session resolution
- Happy path: user re-runs Roger in the repo directory and Roger resumes the
  single strongest matching session for that repo/PR/branch
- Common variants: exact PR match, branch-only match, active session plus older
  archived sessions for the same repo
- Failure/recovery: several plausible matches, no strong match, stale locator,
  user is sent to the session finder instead of Roger guessing silently
- Test intent: prove repo-local re-entry is fast but honest

### F01.2: Global Session Finder and Cross-Repo Jump

- Surfaces: TUI, CLI, extension handoff target selection
- Primary artifact: global session index / session query result
- Happy path: user opens the session finder, searches by repo, PR, attention
  state, or recency, and jumps directly into the chosen session
- Common variants: filter to `awaiting_user_input`, `awaiting_approval`, active
  only, or recent only
- Failure/recovery: session no longer resumable, target repo unavailable
  locally, multiple same-PR instances still require disambiguation
- Test intent: prove Roger is navigable even when the user returns from outside
  the original working directory

### F02: Launch from GitHub PR Page

- Surfaces: extension, local companion, TUI
- Primary artifact: structured review-intake payload
- Happy path: extension attaches Roger entry controls into the GitHub PR page
  using GitHub-native button styling in stable header-action seams, or renders a
  first-class in-page Roger pane in the PR right rail above the reviewers card,
  and Roger opens the correct local session with the lowest-click 4-action set
  (`start`, `resume`, `findings`, `refresh`) available directly from the page
- Common variants: short objective or preset, open directly into a target local
  queue, header-action host versus right-rail pane, modal fallback when
  page-DOM attachment is unavailable, browser-action popup as an explicit
  manual fallback
- Failure/recovery: companion unavailable, bridge missing, multiple local
  instances, launch-only mode with no readback, GitHub DOM drift that forces a
  bounded in-page modal or browser-action fallback entrypoint, additive seam
  unavailable without displacing first-party GitHub actions
- Test intent: prove honest daemonless handoff behavior on supported browsers

### F02.1: Guided Browser Setup And Verification

- Surfaces: CLI, extension packaging, local host registration
- Primary artifact: browser setup state plus doctor result envelope
- Happy path: user runs `rr extension setup`, Roger prepares the unpacked
  extension artifact, guides the one required manual browser load step, learns
  the extension identity without requiring the user to type it, registers the
  installed `rr` binary as the Native Messaging host, and `rr extension doctor`
  confirms the setup truthfully
- Common variants: browser-specific setup for Edge, Chrome, or Brave;
  extension self-registration versus Roger-owned discovery; repair/dev-only use
  of lower-level bridge commands
- Failure/recovery: extension not yet loaded, extension identity missing, host
  registration drift, unsupported browser path, `rr extension doctor` green but
  runtime host execution still broken, doctor reports blocked guidance instead
  of claiming browser launch support
- Test intent: prove the normal browser setup path is guided, truthful, and
  does not require manual extension-id entry or a user-facing separate host
  binary, and that support is only claimed after the registered `rr` host
  binary answers a real Native Messaging round trip

### F02.2: Extension Shortcuts, Settings, And Help

- Surfaces: extension popup, in-page PR entry, options/settings, help surface
- Primary artifact: extension-local operator ergonomics and guidance state
- Happy path: user can discover and use safe non-conflicting shortcuts for core
  Roger actions, adjust extension settings explicitly, and open an in-extension
  help surface that explains action meanings, setup state, and fallback paths
- Common variants: popup-driven help, page-driven help, settings that tune
  page-entry behavior, shortcut enable or disable controls, browser-managed
  shortcut overrides
- Failure/recovery: unavailable shortcut chord, conflicting browser binding,
  missing setup state, help surface points user to `rr extension doctor` or
  local Roger commands rather than pretending health
- Test intent: prove extension ergonomics are discoverable, configurable, and
  truthful without adding hidden state ownership

### F02.3: Inferred Safe Actions And Reduced Extension Friction

- Surfaces: extension PR entry, guided setup, session re-entry
- Primary artifact: inferred-primary-action and reduced-friction interaction model
- Happy path: Roger infers the most likely safe next action from session and
  attention state, hides unnecessary secondary actions until they are relevant,
  continues setup when extension registration arrives, and avoids extra
  disambiguation prompts when a single strongest target is already known
- Common variants: contextual `refresh`, emphasized `resume`, setup completes
  after observed registration, picker still appears for real ambiguity
- Failure/recovery: ambiguity remains explicit, setup still fails closed when
  registration never appears, mutation-sensitive actions remain manual and
  elevated
- Test intent: prove Roger reduces avoidable clicks without inferring across
  safety boundaries

### F03: Structured Findings Pack Intake

- Surfaces: harness, app-core, TUI
- Primary artifact: `StructuredFindingsPack` plus raw output
- Happy path: the LLM emits a valid structured findings pack and Roger
  normalizes it into findings rows, code-evidence locations, and linked
  artifacts
- Common variants: partial pack with valid findings and late-arriving artifacts,
  findings with primary plus supporting code locations, TOON pack versus compact
  JSON pack
- Failure/recovery: malformed pack, missing pack, schema drift, invalid anchors,
  repair loop, raw-only fallback
- Test intent: prove Roger salvages valid data and preserves raw output

### F04: Triage Findings

- Surfaces: TUI
- Primary artifact: normalized `Finding`
- Happy path: user scans the queue, opens the inspector, reviews evidence and
  attached code locations, and changes triage state
- Common variants: batch triage, filter/group by file or severity, compare with
  prior findings, jump from a finding to its primary code anchor
- Failure/recovery: duplicate/superseded finding, invalid anchor, insufficient
  context, user chooses follow-up instead of immediate triage
- Test intent: prove the TUI is the dense decision workspace

### F04.1: Open Finding Evidence in the Local Editor

- Surfaces: TUI, CLI, local editor launcher
- Primary artifact: selected `Finding` plus code-evidence set
- Happy path: user opens the selected finding's primary code location or full
  evidence set in the configured local editor such as VS Code
- Common variants: one primary anchor only, multiple supporting anchors opened
  as additional tabs, no explicit columns present, worktree-specific path
  resolution
- Failure/recovery: editor launcher unavailable, file no longer present
  locally, anchor stale after refresh, Roger falls back to explicit path/range
  references in the TUI
- Test intent: prove editor handoff is thin, truthful, and derived from
  Roger-owned finding state

### F05: Request Follow-Up or Provide Input

- Surfaces: TUI, CLI, extension handoff
- Primary artifact: follow-up review intent linked to existing findings
- Happy path: user requests targeted follow-up and Roger runs another bounded
  stage without losing prior lineage
- Common variants: rerun on one file, one finding class, or one subsystem
- Failure/recovery: user input required, prior context unavailable, follow-up
  yields partial or contradictory findings
- Test intent: prove recursive review remains durable and inspectable

### F05.1: Clarify a Finding Without Mutating It

- Surfaces: TUI, harness
- Primary artifact: clarification thread linked to an existing `Finding`
- Happy path: user asks a bounded explanatory question about a finding and gets
  local clarification without changing triage or outbound state
- Common variants: request more code context, ask why the finding is plausible,
  ask what evidence is weak or missing, compare with related prior findings,
  clarify why one code location is primary and another is only supporting
- Failure/recovery: clarification is inconclusive, requires more context, or
  escalates into `awaiting_user_input` or a deeper follow-up pass
- Test intent: prove users can interrogate findings without being forced into a
  state change

### F06: Refresh After New Commits

- Surfaces: TUI, CLI, extension handoff
- Primary artifact: refresh run plus finding reconciliation
- Happy path: refresh clearly marks findings as `new`, `carried forward`,
  `resolved`, or `stale`
- Common variants: delta-only view, filter to changed findings only, compare
  current vs prior run
- Failure/recovery: rebase/force-push remapping, invalidated anchors, pending
  draft invalidation or reconfirmation
- Test intent: prove refresh does not explode duplicates or lose lineage

### F07: Draft Review, Approval, and Posting

- Surfaces: TUI, GitHub adapter
- Primary artifact: `OutboundDraft` and `PostedAction`
- Happy path: user reviews local drafts, edits or batches them, explicitly
  approves, and Roger posts via the adapter
- Common variants: split one draft into several comments, merge several findings
  into one comment, reject a draft while keeping the finding
- Failure/recovery: partial post failure, remote thread invalid, approval
  revoked before post, retry after adapter failure
- Test intent: prove approval is explicit and audit lineage survives posting

### F08: Inspect History, Original Pack, and Raw Output

- Surfaces: TUI
- Primary artifact: prior runs, stage outputs, original pack, raw output
- Happy path: user inspects the timeline, opens the original structured pack,
  and compares normalized findings with raw output
- Common variants: inspect only one stage, review a posted-action lineage chain,
  open artifacts from an older run
- Failure/recovery: partially structured historical run, missing sidecar index,
  cold artifact fetch
- Test intent: prove degraded and audit paths are first-class, not hidden

### F09: Search and Recall During Review

- Surfaces: TUI, CLI
- Primary artifact: scoped retrieval results
- Happy path: user searches prior findings, summaries, artifacts, or promoted
  memory without leaving the current review context
- Common variants: search from inspector, compare with related prior finding,
  repo-only vs explicit broader overlay
- Failure/recovery: lexical-only degraded mode, conflicting history, no safe
  result, stale-memory suppression
- Test intent: prove retrieval is useful without causing silent scope bleed

### F10: Companion, Bridge, and Setup Recovery

- Surfaces: extension, local companion, TUI/CLI
- Primary artifact: bridge health and local-target resolution
- Happy path: bridge is healthy and local handoff works transparently
- Common variants: browser restart, companion upgrade, version mismatch warning
- Failure/recovery: missing host manifest, missing URL handler, registration
  drift, unsupported browser mode
- Test intent: prove setup and recovery states are honest and actionable

### F11: Structured-Pack Parse Failure and Raw-Output Fallback

- Surfaces: harness, app-core, TUI, extension status
- Primary artifact: malformed or partial `StructuredFindingsPack` plus raw output
- Happy path: Roger salvages the valid subset, records a degraded stage state,
  and keeps raw output available for inspection
- Common variants: missing pack, malformed syntax, schema drift, invalid field,
  invalid anchor, repair succeeds on bounded retry
- Failure/recovery: retry budget exhausted, repair loop exits to `repair_needed`,
  extension shows bounded degraded status only
- Test intent: prove structured-output failures are survivable and auditable

### F12: Multiple Roger Instances or Worktrees for the Same PR

- Surfaces: CLI, TUI, local companion, extension handoff
- Primary artifact: instance/worktree selection plus launch profile resolution
- Happy path: Roger clearly shows multiple eligible local targets for the same
  PR and the user chooses the intended one
- Common variants: same PR open in two named instances, one bare checkout plus
  one worktree, different launch profiles by instance
- Failure/recovery: ambiguous auto-selection, stale instance target, requested
  launch profile unavailable for one instance
- Test intent: prove same-PR concurrency stays explicit and safe

### F13: Draft Invalidation After Refresh or Rebase

- Surfaces: TUI, GitHub adapter
- Primary artifact: `OutboundDraft` linked to changed findings and anchors
- Happy path: refresh or rebase revalidates a draft and either keeps it valid or
  marks it as needing reconfirmation before post
- Common variants: one draft remains valid while another becomes stale, merged
  finding set changes comment grouping
- Failure/recovery: anchor no longer resolvable, thread target changed, prior
  approval revoked automatically because the underlying context changed
- Test intent: prove refresh never lets stale approved drafts post silently

### F14: Honest No-Status Mode for Launch-Only Bridge

- Surfaces: extension, local companion
- Primary artifact: bounded bridge capability state
- Happy path: extension knows it is operating in launch-only mode and offers
  start/resume/refresh without pretending it has live local status, whether the
  entrypoint is inline on the PR page or via the browser-action fallback
- Common variants: install page explains limited mode, user upgrades later to a
  richer companion/readback bridge
- Failure/recovery: stale cached status is suppressed rather than shown as live,
  user is directed to open Roger locally for truth
- Test intent: prove the extension remains honest when readback is unavailable

### F15: Post-Failure Recovery with Partial Success

- Surfaces: TUI, GitHub adapter
- Primary artifact: partially posted `OutboundDraft` batch and `PostedAction`
  records
- Happy path: Roger records which comments posted successfully, which failed,
  and offers targeted retry or manual resolution
- Common variants: one thread succeeds and another fails, duplicate-detection on
  retry, network failure after some remote writes
- Failure/recovery: remote identifiers missing for a subset, ambiguous adapter
  outcome, safe retry without double-posting
- Test intent: prove posting failures preserve audit truth and operator control

### F16: Awaiting User Input as a First-Class Review State

- Surfaces: TUI, CLI, extension bounded status
- Primary artifact: `ReviewRunState` / Roger attention state
- Happy path: Roger pauses in an explicit `awaiting_user_input` state and shows
  exactly what is needed to continue
- Common variants: targeted follow-up question, missing repo context, approval
  choice needed before proceeding, ambiguous launch target
- Failure/recovery: user ignores the request, resumes later, or changes the
  requested action entirely
- Test intent: prove user input is a durable review state, not an incidental
  prompt interruption

### F17: Intentional Dropout to Bare Harness with Roger Control Context

- Surfaces: TUI, CLI, harness
- Primary artifact: `ResumeBundle` / Roger control bundle plus `SessionLocator`
- Happy path: user intentionally drops out of Roger into the underlying
  supported harness session, with the review target, safety posture, and
  Roger-control skills/instructions reloaded
- Common variants: continue questioning the codebase directly, inspect raw
  artifacts in the bare harness, return later to the same Roger session, or use
  an explicit `rr return` helper from inside the bare harness
- Failure/recovery: original harness session cannot reopen, Roger starts a fresh
  harness session seeded from the latest control bundle, or the user returns to
  Roger after partial bare-harness exploration
- Test intent: prove bare-harness fallback is a real operator path rather than a
  fake emergency story; OpenCode is the primary `0.1.0` path and the bounded
  live-CLI providers are secondary paths

### F17.1: Return from Bare Harness Back to Roger

- Surfaces: harness, CLI, TUI
- Primary artifact: Roger session binding in the current bare-harness context
- Happy path: user exits or explicitly runs `rr return` or the harness-native
  `roger-return` command from the bare harness and Roger reopens or refocuses
  the TUI on the same session
- Common variants: Roger owns the parent process and auto-returns on harness
  exit, the harness exposes a provider-native Roger return command, or the user
  returns manually later through `rr return`
- Failure/recovery: no bound Roger session in the current context, target TUI is
  no longer running, or Roger falls back to ordinary `rr resume` / session-finder
  behavior
- Test intent: prove dropout has a natural way back into Roger without depending
  on perfect process continuity

### F17.2: Invoke Roger Commands from Within a Supported Harness

- Surfaces: harness, CLI
- Primary artifact: `RogerCommand`, `RogerCommandResult`, and
  `HarnessCommandBinding`
- Happy path: user invokes a provider-native Roger command such as
  `roger-status`, `roger-findings`, `roger-clarify`, or `roger-return`, and the
  harness adapter routes it through the same Roger-owned core operation as the
  equivalent `rr` command
- Common variants: provider syntax differs, the command opens the TUI instead of
  rendering a bounded inline response, or only a subset of Roger command IDs is
  supported by the current harness
- Failure/recovery: harness has no Roger command surface, the specific command
  is unsupported, or Roger falls back to the equivalent `rr` guidance and
  session-finder path without faking support
- Test intent: prove in-harness commands are convenience adapters with semantic
  parity, not a second command model

## Minimum Cross-Surface Consistency Rules

- every extension action must map to a real local Roger flow, not an
  extension-only branch
- every TUI state that matters operationally should have a stable underlying
  session/run/finding state in app-core
- every degraded mode shown to the user must preserve raw output and audit
  lineage
- every approval-related flow must remain local-first even when launched from the
  browser
- every harness-native Roger command must map to the same Roger-owned core
  operation as its CLI equivalent, or fail truthfully with the CLI fallback path
