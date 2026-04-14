# TUI Workspace And Operator Flow Contract

Status: implementation-facing support contract for the first-release Roger TUI
workspace.

Purpose:

- define the durable first-release TUI workspace shape
- keep TUI implementation grounded in canonical Roger entities and operator
  flows
- move accepted TUI workspace truth out of temporary reconciliation prose and
  into a stable support contract

Authority:

- `AGENTS.md` remains the operational authority
- `PLAN_FOR_ROGER_REVIEWER.md` remains the canonical product plan
- this document is the implementation-facing contract for first-release TUI
  workspace shape and operator flow behavior
- `ROUND_05_SURFACE_RECONCILIATION_BRIEF.md` remains the broader reconciliation
  brief for unresolved CLI, extension, and validation-surface work

## Scope

This contract covers:

- the TUI workspace layout and durable destinations
- the operator primitives that the TUI must expose
- canonical nouns and selection behavior
- the separation between browsing, prompting, and mutation-capable actions

This contract does not define:

- the full CLI command family
- extension placement or browser CTA policy
- queue internals and background execution rules already covered by
  `TUI_RUNTIME_SUPERVISOR_POLICY.md`

## Core operator questions

The Roger TUI should answer, with low navigation cost:

- what needs operator attention now
- what changed since the last pass
- what is selected right now
- what is already drafted
- what is ready to approve
- what follow-up or prompt action should happen next

## First-release primitives

The first TUI release is built from these primitives:

1. `attention queue`
2. `focusable work queue`
3. `stable selection set`
4. `inspector`
5. `composer`
6. `prompt source model`
7. `elevated mutation gate`
8. `dropout and return bridge`

If a proposed first-release TUI feature does not materially strengthen one of
those primitives, it should be treated as follow-on scope by default.

## Canonical nouns

The TUI must project Roger-owned domain objects rather than inventing new
UI-only nouns.

Rules:

- `Finding` is the primary operator object for review work
- selection sets carry canonical finding ids rather than row numbers
- draft views project `OutboundDraft` and `OutboundDraftBatch`
- session views project `ReviewSession`, `ReviewRun`, and `AttentionState`
- prompt and follow-up surfaces project `PromptPreset`, `ReviewTask`, and
  `PromptInvocation`
- help overlays, command palettes, and mouse affordances are behavior only; they
  do not create shadow domain objects

## Workspace layout

Default workspace shape:

1. top status strip
2. primary working region
3. persistent secondary inspector whenever screen size allows

Rules:

- the inspector must earn its space by showing high-signal detail for the
  current focus
- if the inspector is frequently empty, redundant, or less useful than the
  queue, that is a product bug
- wide-screen adaptation should preserve clarity rather than merely filling
  columns

## Durable operator destinations

The first-release primary workspace should expose five durable destinations:

- `Home`
- `Findings`
- `Drafts`
- `Search/History`
- `Sessions`

The following should be overlays or drawers rather than peer workspaces:

- `Composer`
- `Prompt Palette`
- `Help`

Rules:

- do not promote every useful concept to a top-level screen
- do not introduce a top-level board metaphor unless it clearly outperforms the
  findings queue for real review work

## Findings queue contract

The findings queue is a real work queue, not a preview list.

Required capabilities:

- stable focus
- single-select and multi-select
- range-select and additive select
- grouping by useful Roger dimensions such as file, severity, lineage, run, and
  draft state
- saved filters or quick scopes for common review modes
- explicit handoff of the current selection into follow-up, clarification, or
  draft creation flows

Rules:

- moving between queue and inspector must not discard the working set
- refresh should preserve valid selections where identity still exists
- queue rows should remain grounded in canonical domain objects, not in
  decorative dashboard cards

## Inspector contract

The inspector is the consistent detail region for the current focus target.

It should be able to show:

- finding detail
- code evidence preview
- draft detail
- posting failure detail
- prompt preset detail
- session summary
- selected history item

Rules:

- inspector content must explain the current focus, not merely restate list-row
  text
- derived ranking or attention signals should include concise interpretation and
  action guidance when material
- stale, invalidated, repair-needed, and posting-failed states must surface as
  bounded operator states with visible next actions

## Composer contract

The first-release TUI includes one bounded local composer.

Supported modes:

- `clarify`
- `session chat`
- `follow-up`
- draft refinement from the current selection

Rules:

- composer input must preserve canonical finding references and current
  selection-set context
- clarification remains linked to finding lineage and materializes through
  Roger-owned task/result lineage rather than a UI-only side thread
- session chat remains attached to the active review session through explicit
  task lineage
- palette-driven or freehand follow-up launches a new canonical `ReviewTask`
  and corresponding `PromptInvocation` turn history
- the TUI must not become an unconstrained general harness shell

## Mutation visibility contract

Approval and posting must remain visibly elevated relative to browsing.

Rules:

- draft materialization, approval, and posting must not look visually equivalent
  to navigation
- the TUI should expose a clear draft queue and approval path
- mutation-capable actions should be separated from triage, browsing, and
  informational views
- invalidation or drift that blocks posting must be visible in the same
  workspace

## History and search contract

History and recall are durable operator tools, not novelty views.

Rules:

- history should answer live operator questions such as what changed, why an
  item needs attention, and what happened since the last pass
- search and history should share the same queue-plus-inspector grammar where
  practical
- the first release does not need multiple unrelated history visualizations if
  one strong operator flow answers those questions well
- search rows should project canonical `RecallEnvelope` truth rather than loose
  snippets or unlabeled ranked blobs
- the TUI should make `query_mode`, `retrieval_mode`, scope bucket, lane, trust
  posture, and degraded flags visible when they materially affect operator
  judgment
- the TUI owns dense recall inspection, candidate audit, and promotion review;
  the extension does not
- promotion review actions from the TUI create or resolve
  `MemoryReviewRequest` objects rather than mutating durable memory through
  ad hoc UI state
- the active `SessionBaselineSnapshot` should be inspectable from the
  Search/History workspace so operators can see which scopes and default recall
  posture currently govern the session

## Dropout and return

The TUI must support deliberate dropout to the underlying harness or a shell and
an obvious return path back into Roger.

Rules:

- dropout should be a visible action, not a hidden escape hatch
- return should preserve operator control context truthfully
- bounded Roger-owned chat and deliberate raw-harness freedom are both valid,
  but they should remain visibly distinct modes

## Interaction quality rules

These are first-release product requirements, not optional polish:

- fixed headings where scrolling would otherwise destroy orientation
- visible paging or position state for long queues
- consistent `help` and `escape/back` behavior
- low-confusion focus movement
- clear indication of current selection
- no hidden prompt stacking
- no ambiguous mutation affordances

## Non-goals for first release

Do not treat these as first-release defaults:

- a top-level board or kanban workspace
- decorative analytics panes with weak operator value
- view proliferation without a stable operator grammar
- unconstrained free-form harness UI embedded inside Roger

## Relationship to Round 05

`ROUND_05_SURFACE_RECONCILIATION_BRIEF.md` should continue to own:

- unresolved CLI surface reconciliation
- unresolved extension UX and action-model reconciliation
- broader surface-proof sequencing

This contract owns the accepted TUI workspace and operator-flow shape that no
longer needs to remain only in a reconciliation brief.
