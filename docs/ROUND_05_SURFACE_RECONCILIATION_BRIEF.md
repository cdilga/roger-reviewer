# Round 05 Surface Reconciliation Brief

Status: Proposed  
Audience: Roger maintainers and implementers working on active `0.1.x` UX,
surface, and support-claim hardening  
Scope: TUI, CLI, browser extension, and the validation posture that defends
their current product claims

---

## Why this brief exists

Roger does not currently have a blank-slate planning problem. It has a surface
reconciliation problem.

The canonical plan already says the following:

- the TUI is the primary decision cockpit
- the CLI is the glue layer between repo context, the TUI, and automation
- the extension is a PR-page launch, status, and targeted-handoff surface
- approval and posting must remain explicit, local-first, and visibly elevated

The implementation has drifted away from that product shape in ways that are now
visible:

- the from-source developer path is currently broken by a duplicate dependency
  key in `packages/cli/Cargo.toml`
- the live CLI surface still lacks first-class draft, approval, posting,
  bootstrap, and uninstall commands that the plan already expects
- the current TUI shell is a useful structural scaffold, but it is still much
  closer to a read-mostly shell than to the dense operator cockpit the plan
  describes
- the extension has good bounded-launch instincts, but the actual placement and
  action model are still underspecified relative to the intended UX
- the bead frontier is now too narrow to represent the real product gaps

This brief does not roll the project back to a pre-polish planning state. It
narrows one active implementation problem:

1. reconcile the real product surfaces with the canonical plan
2. identify the current UX, command, and support mismatches explicitly
3. turn those mismatches into a realistic implementation and validation order

---

## Core decision

Do not move back to an earlier pre-polishing plan snapshot.

Instead:

- keep `docs/PLAN_FOR_ROGER_REVIEWER.md` as the canonical product plan
- keep
  `docs/PLAN_FOR_TRUTHFUL_PROVIDER_PARITY_AND_GITHUB_COPILOT_CLI.md` as the
  active hardening follow-on for provider and lifecycle truth
- use this brief to reconcile the user-facing surfaces and operator workflows
  with current code reality
- shape new proof beads where the current graph no longer reflects actual
  product risk

This is a mid-build reconciliation pass, not a vision reset.

---

## Current repo truth for the surface layer

### TUI truth

The canonical plan expects a dense review cockpit with:

- Review Home
- Session Overview
- Findings Queue
- Finding Inspector
- Draft Approval Queue
- Timeline and History
- Search and Recall

The current `packages/app-core/src/tui_shell.rs` shell proves useful structural
state for:

- Session
- Recent Runs
- Findings
- Finding Detail
- Draft Queue
- Activity

That shell is valuable, but it is not yet the full operator workspace described
in the canonical plan.

### CLI truth

The live help surface currently exposes:

- `rr review`
- `rr resume`
- `rr return`
- `rr sessions`
- `rr findings`
- `rr search`
- `rr refresh`
- `rr status`
- `rr update`
- `rr extension setup`
- `rr extension doctor`
- `rr bridge ...`
- `rr robot-docs`

The canonical plan already says the current command surface is still missing:

- a dense workspace entry contract such as `rr open` or `rr tui`
- a first-class draft, approval, and posting command family
- a truthful bootstrap and preflight surface centered on `rr doctor`
- a product-facing `rr extension uninstall`

### Extension truth

The current extension already has several correct instincts:

- Native Messaging only on the serious path
- popup explicitly treated as manual backup
- bounded status mirror only when readback is truthful
- contextual primary action derived from Roger attention state
- no approval or posting controls in the browser

But there are still UX mismatches:

- the current code and tests prefer right-rail placement before inline header
  placement, while the canonical extension path says inline first, right rail
  second, modal third
- the current action model maps `awaiting_outbound_approval` to `Findings`,
  which is safe but undersells the more important product state: open drafts
  locally
- the extension does not yet expose the stronger local focus actions the
  canonical plan wants for the companion tier

### Build and package truth

This reconciliation pass must start from live repo reality, not from
architectural intent alone.

Current blocker:

- the source-run path is presently broken because
  `packages/cli/Cargo.toml` contains duplicate `serde.workspace = true`
  entries, which prevents `cargo run -p roger-cli --bin rr -- --help`

That is a product-surface issue, not just a dev-only nuisance, because the repo
currently claims a truthful from-source developer path.

Planning-pass boundary note:

- this brief owns the user-facing surface consequences of install, update,
  doctor, and extension lifecycle work
- the deeper delivery/install/release contract should live in the canonical plan
  plus narrow support contracts rather than expanding this brief into another
  long-lived delivery-spec document
- command naming should settle on `rr update` and `rr extension uninstall` as
  the product-facing verbs; lower-level bridge uninstall/repair commands remain
  demoted repair surfaces only if still needed

---

## Surface authority rules

### D1. TUI remains the primary decision cockpit

If a workflow needs dense evidence comparison, batch triage, refresh lineage,
draft editing, approval review, post-failure recovery, or operator-oriented
prompt steering, it belongs in the TUI first.

### D2. CLI remains the router, finder, inspector, and mutation gate

The CLI should excel at:

- session entry and repo-local re-entry
- session finding and cross-repo navigation
- status, findings, and search inspection
- explicit draft, approval, and posting transitions
- bootstrap, doctoring, and update flows
- targeted handoff into the dense TUI workspace

### D3. Extension remains a PR-page launcher and bounded mirror

The extension should specialize in:

- start, resume, and open-locally entry from a PR page
- bounded attention and count mirroring
- safe contextual CTA inference
- targeted local focus actions such as open findings or open draft queue

The extension should not own:

- approval decisions
- GitHub posting
- dense evidence reading
- batch triage
- long-form prompt authoring

### D4. Approval and posting must become explicit product surfaces

The plan already requires local draft -> approve -> post as a first-class path.
That cannot remain implied by storage structures or buried inside the TUI shell.

Implication:

- the TUI needs a visibly elevated draft and posting workflow
- the CLI needs an explicit command family
- the extension may only indicate approval-required state and open that local
  workflow

### D5. Product help and command surfaces must be layered

Roger needs two clearly separated layers:

- product-facing commands used by ordinary operators
- dev and repair commands used by maintainers

That means:

- `rr --help` should foreground product surfaces
- `rr bridge ...` should remain available but visibly demoted
- extension setup should never require normal users to know extension IDs or
  host-binary paths

---

## Deep TUI reconciliation

## Product role

The TUI should answer, with almost no navigation cost:

- what requires a decision right now
- what changed since the last pass
- what is already drafted
- what is ready to approve or unsafe to post
- what prompt or follow-up action should be run next

## First-release primitive model

The first TUI release should not be designed as a list of screens or as a
grab-bag of features. It should be designed around a small number of strong
primitives that compose into most operator workflows.

Recommended first-release primitives:

1. `attention queue`
   - one durable place that answers "what needs me now?"
   - powered by canonical Roger attention state, not ad hoc UI heuristics
2. `focusable work queue`
   - findings, drafts, sessions, search results, and history entries should all
     behave like focusable queue items rather than unrelated widgets
3. `stable selection set`
   - the operator must be able to select one or many items and carry that
     working set across views and actions
4. `inspector`
   - one consistent detail region for understanding the currently focused item
5. `composer`
   - one bounded place to clarify, follow up, draft, or otherwise act on the
     current selection without opening a second product
6. `prompt source model`
   - a clear split between what Roger auto-injects per session and what the
     operator injects intentionally for a run
7. `elevated mutation gate`
   - draft approval and posting stay visibly separate from triage and browsing
8. `dropout and return bridge`
   - the operator can leave the cockpit deliberately and return without losing
     control context

If a proposed TUI feature does not strengthen one of those primitives, it is
probably not first-release material.

## Schema alignment and canonical nouns

The TUI, CLI, and extension should project canonical Roger entities rather than
inventing fuzzy UI-only nouns.

Rules:

- a `Finding` is the core operator primitive, not a generic "issue card" or
  anonymous row
- the findings queue operates on canonical `Finding` objects and selection sets
  carry canonical finding identities
- the inspector projects `Finding`, `CodeEvidenceLocation`, related
  clarification history, and any linked outbound-draft state
- session-management views project `ReviewSession`, recent `ReviewRun` history,
  and canonical `AttentionState`
- draft approval views operate on `OutboundDraft` items grouped by
  `OutboundDraftBatch`, because approval and posting happen at the batch level
- prompt palette and prompt reuse operate on `PromptPreset` and
  `PromptInvocation`, not on a second ad hoc prompt-store invented by the UI
- the composer launches `PromptInvocation` records; finding-bound clarification
  should remain on Roger's clarification lineage, while session-wide chat should
  still persist through ordinary invocation and artifact history rather than a
  second uncontrolled chat model
- stable selection sets are controller state over canonical ids, not a new
  durable aggregate
- help overlays, mouse affordances, and command palettes are UI behavior only;
  they should not create shadow domain objects

If a proposed surface cannot say which canonical object it reads, mutates, or
links, it is underspecified.

## What an amazing first TUI should feel like

The first release does not need to feel huge. It needs to feel sharp.

The target feeling is:

- high signal at a glance
- no confusion about what is selected
- no confusion about what will mutate state
- no confusion about what prompt context is active
- very low friction to go from reading to acting
- no need to remember hidden state to understand why Roger is behaving a
  certain way

The anti-patterns to avoid are:

- too many peer views with no stable operator grammar
- a queue that loses selection or context every time focus changes
- hidden prompt stacking that makes runs hard to explain
- posting actions that feel visually equivalent to harmless navigation
- trying to ship "everything the TUI might someday do" in the first release

## Information architecture

The default active Roger workspace should have three simultaneous regions:

1. a top status strip
2. a primary working region
3. a secondary inspector region

The primary working region should switch between these durable operator views:

- `Review Home`
- `Session Management`
- `Session Overview`
- `Findings Queue`
- `Draft Approval Queue`
- `Timeline and History`
- `Search and Recall`
- `Prompt Palette`
- `Help / Command Overview`

The secondary inspector should be able to show:

- finding detail
- evidence preview
- draft detail
- posting failure detail
- prompt preset detail
- session summary
- selected history item

Recommended first-release shape:

- keep the primary workspace to five durable operator destinations:
  - `Home`
  - `Findings`
  - `Drafts`
  - `Search/History`
  - `Sessions`
- make `Prompt Palette`, `Help`, and the `Composer` overlays or drawers rather
  than full peer workspaces
- keep the inspector persistent whenever screen size allows

This is a better first-release tradeoff than giving every concept a top-level
screen.

## Roger-specific TUI reconciliation rules

Recent TUI analysis reinforced several design tradeoffs, but the important move
here is to phrase them as Roger rules rather than as another project's lineage.

Use these as reconciliation rules for Roger:

- Roger must stay grounded in canonical `Finding`, `OutboundDraftBatch`,
  `ReviewSession`, `AttentionState`, and prompt lineage rather than generic
  cards, issues, or dashboard widgets
- adaptive split-view is worth keeping, but the inspector must earn its screen
  cost; if the right-hand region is frequently empty, redundant, or less useful
  than the queue, that is a product bug rather than an acceptable placeholder
- top-level view count must stay low; rich TUIs degrade quickly when every
  useful concept becomes a peer workspace, so overlays and drawers should
  absorb help, prompt, and composer affordances wherever possible
- any Roger-owned ranking or attention signal must be explained in-product;
  opaque scores create operator distrust, so the TUI should expose concise
  "what this means", "why Roger surfaced it", and "how to act on it" guidance
  wherever usefulness or attention ordering is material
- history is worth a durable operator surface, but only when it answers live
  questions such as "what changed since my last pass?" and "why is this session
  asking for attention now?"; history should share the same queue-plus-inspector
  grammar rather than becoming a separate novelty screen
- do not cargo-cult a kanban or board metaphor into Roger; workflow boards
  often consume lots of space while adding little beyond a strong queue, so
  Roger should only ship board-like surfaces if they clearly outperform the
  findings queue for real review work
- expensive analysis should be computed once in shared core state and projected
  into each surface; if a calculation is skipped, stale, or unavailable, Roger
  should say so explicitly rather than pretending the result is always live
- terminal mechanics are first-release product work, not polish debt: fixed
  headings, visible paging state, consistent escape/help behavior, predictable
  wide-screen adaptation, and low-confusion focus movement all materially affect
  whether a dense cockpit feels trustworthy

Net implication:

- Roger should copy the strong parts of that TUI lineage:
  - persistent inspector on wide screens
  - one shared computation engine across TUI, CLI, and extension
  - explicit explanation for derived metrics and ranked work
  - durable history as an operator tool
- Roger should reject the weak parts:
  - view proliferation without a stable operator grammar
  - decorative secondary panes
  - board metaphors that do not beat the queue

## Session management window

This should be a first-class TUI window, not an incidental list hidden inside
another panel.

Required capabilities:

- list active and recent sessions across repos and PRs
- filter by attention state, provider, recency, repo, and PR
- switch the active cockpit session without losing context
- show bounded health signals such as continuity state, pending approval, and
  refresh recommendation
- allow bulk operator actions where safe, such as archive, hide, pin, or mark
  as recently handled
- expose repo-local and global session-finder semantics through the same model

Desired follow-on capabilities:

- show stale or blocked sessions separately from healthy active sessions
- surface sessions waiting on operator action as a dedicated attention slice

First-release scope note:

- a dedicated `session compare` view is not required for the first TUI release
- the likely value of session compare is diagnosing multiple same-PR sessions,
  different worktrees, or provider divergence, but that is not a core daily
  operator primitive
- first release should instead support:
  - strong session summaries
  - clear active-session switching
  - visible recency and health
  - history comparison within one session across runs

If later evidence shows that operators regularly have two live same-PR sessions
that need side-by-side reasoning, session compare can return as a follow-on
surface.

## Findings queue and multi-selection

The findings queue should be a real work queue, not just a preview list.

Required capabilities:

- multi-select findings
- range-select and additive select
- batch triage transitions
- batch move to follow-up
- batch draft materialization when appropriate
- grouping by file, severity, lineage, run, draft state, and usefulness signals
- saved filters or quick scopes for common review modes

Selection should be durable enough that the operator can:

- move between queue and inspector without losing the working set
- pass selected findings into a chat or follow-up composer
- draft or approve against the current selection explicitly

## Chat and finding-reference composer

Roger needs an explicit local chat and follow-up composer inside the TUI.

First-release recommendation:

- support two bounded chat modes inside one composer:
  - `clarify`: tied to one or more currently selected findings
  - `session chat`: tied to the active review session with optional finding
    references and current working-set context
- keep both modes Roger-owned and auditable rather than turning the TUI into an
  unconstrained general chat shell
- if the operator wants a freer harness experience or the bounded chat lane hits
  its limit, the default escape hatch should be immediate dropout to the
  underlying harness

Required capabilities:

- compose a bounded clarification, follow-up, or session-local chat turn from
  the TUI
- reference one or more selected findings without manual copy and paste
- autocomplete finding references from current selection, recent findings, and
  visible queue rows
- preserve the exact selected finding set in the invocation snapshot
- route the result either to clarify-in-place or to a new bounded follow-up run
- allow an immediate handoff to the underlying harness from the same context
  when the operator chooses to leave Roger's bounded lane

Canonical reference model:

- support stable finding reference insertion by Roger-owned IDs, not fragile row
  numbers
- render those references with a compact bespoke syntax that remains readable in
  the TUI and durable in invocation history

First-release syntax decision:

- use `@finding(<id>)`

Storage and lineage rules:

- references are easy to type or autofill
- the syntax is unambiguous
- the parser is Roger-owned
- the resulting prompt snapshot stores both the literal text and the resolved
  finding set
- finding-bound clarification remains linked to the selected `Finding` set and
  should flow through Roger's clarification lineage
- session-local chat stays attached to the current `ReviewSession` plus any
  optional finding references; for `0.1.x`, it should persist as prompt
  invocation and artifact history rather than a second canonical chat object

Recommended first-release constraint:

- keep the composer bounded to Roger-owned actions and session-local reasoning:
  - clarify selected finding(s)
  - run a follow-up pass against selected finding(s), file(s), or subsystem
  - create or refine local draft text from selected finding(s)
  - hold a short session-local chat while Roger remains the control surface
- do not turn the first release into a second full harness UI with ambient
  arbitrary tool usage and hidden context mutation

That gives Roger a real chat lane without weakening the case for deliberate
dropout when the operator wants raw harness freedom.

## Drop out to CLI or harness from the TUI

The canonical plan already supports deliberate dropout into the underlying
harness. This brief tightens the surface rule:

- the default dropout target from the TUI should be the underlying harness for
  the active Roger session
- Roger should also support explicit secondary dropout paths into the local CLI
  command surface or a shell in the same session context when those are the
  clearest operator moves
- the TUI should expose a visible `drop out` action with target variants such
  as:
  - open underlying harness
  - print or copy equivalent `rr` command
  - open shell in current session context
- the return path back into Roger must stay obvious and durable

This is especially useful when the operator wants:

- raw command-line search or inspection
- quick scripting against the same session
- a direct harness experience for a while

## Help overview and discoverability

The TUI should expose a first-class help layer, conventionally via `?`, with:

- global keybindings
- current view actions
- selection grammar
- draft and posting actions
- clarification and follow-up actions
- prompt palette entry
- dropout and return actions
- mouse affordances

This should be an overlay or panel, not an external doc dependency.

## Mouse support

Roger should remain keyboard-first, but mouse support should exist anywhere it
meaningfully reduces friction.

Required mouse support:

- select queue rows
- multi-select with platform-appropriate modifier semantics
- click to inspect
- click tabs, filters, and prompt palette entries
- scroll evidence, history, and draft views
- resize panes if the TUI layout supports resizable regions
- click explicit elevated actions such as approve, reject, and post

Mouse support must not weaken clarity about mutation-sensitive actions. Posting
still needs elevation and confirmation semantics.

## Prompt palette

Roger should have a real prompt palette in the TUI, modeled on the operational
strength of the NTM palette rather than on a thin dropdown.

There is already strong groundwork in the repo:

- Roger has a `PromptPreset` contract with stable preset IDs, recent, frequent,
  last-used, and optional favorites
- the repo already keeps a battle-tested NTM palette in
  `docs/swarm/command_palette.md`

## Prompt management: autoinjected vs operator-injected

Roger should explicitly separate two prompt layers in the TUI.

### Layer A: session-baseline auto-injection

This is the prompt context Roger applies automatically for the active review
session. It should be visible, inspectable, and stable enough that the operator
can explain why a run behaved the way it did.

Recommended session-baseline fields:

- review target identity
- safety posture and review mode
- resolved repo or project instruction layer
- default prompt preset for this session, if one is set
- bounded continuity bundle or prior-run carry-forward context
- selected provider and any provider-safe control text Roger owns

Rules:

- session-baseline injection is Roger-owned, not an invisible stack of ad hoc
  prompt fragments
- the TUI should let the operator inspect the active session baseline
- the baseline should be sticky for the session unless the operator changes it
  intentionally

### Active session baseline changes

An active session baseline change means changing the sticky Roger-owned defaults
that future runs in the current session will inherit automatically.

Examples:

- changing the session's default prompt preset
- changing the preferred provider or model within an already-allowed capability
  tier
- changing how much prior-run carry-forward context Roger injects by default

For `0.1.x`, baseline changes should be:

- explicit, not ambient
- visible in session overview and prompt palette
- recorded as a forward-only run-mode boundary for future prompt invocations
- never retroactive over prior invocations or prior findings history

For `0.1.x`, baseline changes should not be used for:

- changing review-target identity
- changing approval or posting authority
- changing safety posture or trust-floor policy
- silently rewriting prior session behavior after the fact

If one of those stronger changes is needed, Roger should require a new session
or a more explicit elevated local flow rather than pretending the current
session is still semantically the same.

### Layer B: operator-injected run modifiers

This is the explicit operator action for one clarification, follow-up, or draft
generation run.

Recommended run-level injectables:

- selected prompt preset
- short explicit objective
- selected finding references
- scoped modifiers such as changed-files-only, security-only, or explain-why

Rules:

- operator-injected modifiers must be visible before execution
- Roger should snapshot the exact resolved prompt and resolved finding set
- the operator should be able to remove or edit these injectables before launch

### UI contract for prompt management

The TUI should render prompt state as:

- `session baseline`: a stable, inspectable summary visible from the session
  overview and prompt palette
- `run modifiers`: chips, tokens, or a compact summary in the composer before
  execution

This gives Roger three valuable properties:

- clear operator mental model
- reproducible audit trail
- low-friction run setup without hidden magic

### First-release recommendation

For `0.1.x`, keep prompt management to:

- one active session baseline
- one selected preset per run
- one optional short explicit objective
- one optional resolved selection set

Do not add:

- arbitrary prompt-stack builders
- nested preset composition UI
- prompt marketplace behaviors
- unconstrained freeform browser-driven prompt injection

Required TUI prompt-palette capabilities:

- open from anywhere in the cockpit
- show favorite, recent, frequent, and scope-valid presets first
- show preset categories, labels, and short summaries
- preview the resolved prompt before execution
- allow a short explicit objective to be layered onto a preset
- allow operator-pinned shortcuts for the most valuable presets
- launch clarify or follow-up actions directly from the palette
- record preset choice and resolved prompt snapshot exactly

Recommended implementation rule:

- reuse Roger's `PromptPreset` model and storage as canonical
- lift proven palette interaction ideas from the NTM operator palette, but do
  not make Roger depend on NTM-specific file formats as its canonical runtime
  model

This should feel like a real operator tool, not a settings menu.

## Proposed first-release interaction grammar

The TUI should have a small, memorable control language.

Recommended baseline grammar:

- `j/k` or arrow keys: move focus within the current queue
- `tab` / `shift-tab`: move between primary regions
- `enter`: inspect the focused item
- `space`: add or remove focused item from selection
- `x`: open batch actions for current selection
- `/`: filter or search within the current queue
- `c`: open composer for clarify or follow-up on current selection
- `p`: open prompt palette
- `d`: open draft queue
- `s`: open sessions
- `h`: go to review home
- `o`: open evidence or local handoff
- `?`: help overlay
- `:` optional command palette or command line for power users

The exact keys can change. The stronger point is that navigation, selection,
compose, inspect, and mutate should each have a stable verb.

## Other TUI features that still need explicit UI ownership

The following user-facing capabilities are either clearly needed or strongly
implied by the canonical plan, and should be mapped explicitly into UI work:

- review-home attention queue
- queue badges and counts for `awaiting_outbound_approval`, `refresh_recommended`,
  and `review_failed`
- diff-aware refresh comparison view
- approval invalidation banner after refresh or retarget
- posting failure recovery view with retry and audit detail
- evidence open-in-editor affordances
- search result jump-to-session and jump-to-finding actions
- quick-copy actions for finding IDs, evidence anchors, and local handoff
  commands
- timeline drilldown into stage outputs and prompt invocations
- usefulness labeling UI for prompts or findings when the operator wants to
  capture that signal
- launch-target picker when `rr open` or extension handoff needs to focus a
  specific workspace queue

## TUI validation implications

The TUI should be defended with:

- structural state snapshot tests
- controller tests for session switching, selection, multi-select, chat
  references, palette actions, approval invalidation, and posting recovery
- keybinding and help-overlay tests
- mouse-interaction tests where the framework permits them cheaply
- prompt-baseline versus run-modifier resolution tests

---

## Deep CLI reconciliation

## Product role

The CLI should be the product router between:

- repo context
- global session discovery
- the TUI cockpit
- bounded automation
- explicit approval and posting transitions

## Recommended product-facing command families

### Entry and navigation

- `rr review`
- `rr resume`
- `rr open` or `rr tui`
- `rr return`
- `rr sessions`

Recommended role of `rr open`:

- open the dense workspace for the strongest matching session
- accept `--session`, `--pr`, and `--focus`
- support safe focus targets such as `overview`, `findings`, `drafts`, `history`,
  `search`, `prompt-palette`, and `session-management`

### Inspection

- `rr status`
- `rr findings`
- `rr search`
- optional `rr history` if timeline and history remain large enough to deserve a
  separate command

### Explicit outbound flow

- `rr draft`
- `rr approve`
- `rr post`

### Health and bootstrap

- `rr init`
- `rr doctor`
- `rr update`

### Browser lane

- `rr extension setup`
- `rr extension doctor`
- `rr extension uninstall`

### Dev and repair

- `rr bridge ...`
- contract export and verify flows
- lower-level host registration repair

These should remain available but clearly demoted below product help.

## CLI behavior standards

- repo-local re-entry should be fast when there is one strong match
- ambiguity should open a session finder or produce a ranked explicit selection
- `rr status` should be a concise attention-and-health summary, not a dump
- `rr findings` should be good for inspection and scripting, not for replacing
  the TUI
- `rr sessions` should become the durable global session finder surface
- `rr --help` should present product commands before low-level commands
- CLI output should make it easy to drop into the TUI with an explicit focus
  target
- product-facing verbs should represent operator intent, not bridge plumbing;
  low-level bridge refresh, host probing, or repair semantics belong in
  `doctor`, setup, or explicitly demoted dev flows

## CLI features that still need explicit UI ownership

- `rr open --focus <queue>`
- `rr draft`, `rr approve`, and `rr post`
- `rr doctor`
- `rr extension uninstall`
- compact session-picker and session-management flows
- command aliases or short-hands that align with the TUI help model

## CLI validation implications

- command-surface truth tests for `rr --help`
- session-finder and ambiguity handling tests
- outbound command-family tests
- doctor/init/update gating tests
- regression test that the from-source path can at least load manifest/help

---

## Deep extension reconciliation

## Product role

The extension should be Roger's:

- PR-page launch surface
- bounded local-status mirror
- targeted local-handoff surface

It should not become a second review cockpit.

## Placement order

The canonical plan's placement order is the right one and should win:

1. additive inline or header seam first
2. bounded right-rail Roger host second
3. in-page modal fallback third
4. toolbar popup as explicit manual backup only

The current implementation and tests prefer the right rail first. That should be
treated as a reconciliation bug, not as an alternate product direction.

## Inline versus right-rail behavior

Inline header placement should stay compact.

Recommended rule:

- inline or header seam: one primary CTA plus a compact Roger entry or overflow
  affordance
- right-rail host: full safe action set plus bounded status and counts
- modal fallback: same safe action set as the right-rail host when no coherent
  page seam exists

## Action model

Current safe action family:

- `Start`
- `Resume`
- `Findings`
- `Refresh`

Recommended companion-tier additions:

- `Open in Roger`
- `Open Draft Queue`
- `Open Session Overview`

Recommended mapping:

- no matching local session: `Start`
- `review_started` or `awaiting_user_input`: `Resume`
- `findings_ready`: `Findings`
- `refresh_recommended`: `Refresh`
- `awaiting_outbound_approval`: `Open Draft Queue`
- `review_failed`: `Open in Roger` with failure guidance

## User-intent actions only

The extension should surface operator-intent actions, not transport mechanics.

Rules:

- ordinary PR-page controls should represent review intent such as `Start`,
  `Resume`, `Findings`, `Refresh`, `Open Draft Queue`, or `Open in Roger`
- `Refresh` in product UI must always mean "refresh the review against changed
  PR or repo state", never "re-read Native Messaging state" or "retry bridge
  transport"
- if Roger can attempt bridge readback, state refresh, or staleness checks
  automatically on mount, focus, or bounded retry, it should do so automatically
- manual controls such as `refresh bridge`, `ping host`, `reload status`, or
  similar plumbing should stay out of the normal PR surface
- bridge-maintenance actions belong only in setup, doctor, or explicit recovery
  views when something is actually broken
- when readback is unavailable, the extension should degrade to truthful launch
  and open-local actions rather than surfacing a nervous cluster of maintenance
  buttons

## Status and counts

The extension should mirror only bounded and truthful state:

- current attention label
- freshness indicator
- bounded counts such as findings needing attention, drafts awaiting approval,
  or refresh-needed state

If readback is stale or unavailable:

- hide mirrored state
- keep launch and open-locally actions
- point the operator back to local Roger as authoritative

## Settings, help, and shortcuts

The extension should not force product ergonomics into repo docs alone.

Desired in-extension ergonomics:

- explicit help surface
- setup and doctor state summary
- safe keyboard shortcuts for core actions
- explicit settings for seam preference and fallback behavior only when those
  settings are truthful and bounded

## What must stay local

The browser extension must not become:

- the approval surface
- the posting surface
- the evidence inspection cockpit
- the batch triage workspace

The correct browser behavior for approval-required state is:

- show that approval is required
- offer `Open Draft Queue`
- do not offer approve or post in-browser

## Extension validation implications

- DOM seam placement tests that enforce inline-first, rail-second, modal-third
- action-model tests for each canonical attention state
- snapshot or DOM-structure tests for compact inline host versus right-rail host
- theme and readability smoke
- one browser-path smoke for Native Messaging request and response on the real
  installed `rr` host path

---

## Validation implications across surfaces

This surface work should not be defended by one generic E2E.

Each surface needs the cheapest truthful proof layer.

Cross-surface validation should cover:

- review-flow matrix coverage for `F01`, `F01.2`, `F02`, `F02.3`, `F04`,
  `F05.1`, `F06`, `F07`, and `F11`
- attention-state parity across TUI, CLI, and bounded extension mirror
- approval-state invalidation after refresh or retarget
- prompt-palette selection and invocation snapshot truth
- dropout and return behavior across TUI, CLI, and harness

---

## Recommended bead shaping

The current frontier is too small for the real product state. The graph should
be widened with proof-oriented child beads rather than treated as nearly done.

Recommended bead groups:

1. build and package truth
   - restore green manifest/build/help path
   - prove source-run onboarding again
2. CLI surface reconciliation
   - product help ordering
   - `rr open` or `rr tui`
   - draft, approve, and post family
   - `rr doctor`
   - `rr extension uninstall`
3. TUI cockpit completion
   - review home
   - session management window
   - findings queue and inspector ergonomics
   - multi-select and batch operations
   - chat and finding-reference composer
   - prompt baseline and run-modifier model
   - prompt palette
   - help overlay and mouse support
   - draft approval queue completion
   - history/search/session-finder views
4. extension UX reconciliation
   - placement-order fix
   - inline versus rail host contracts
   - local focus actions
   - help, settings, and shortcuts
5. validation hardening
   - CLI surface truth
   - TUI structural and interaction tests
   - extension seam and attention tests
   - one real browser or host smoke lane

These should be treated as independently provable slices, not one giant UX epic.

---

## Recommended implementation order

1. restore build/help truth
2. lock the CLI product surface and help hierarchy
3. complete the TUI operator information architecture
4. add the prompt baseline, prompt palette, session management, and selection grammar
5. reconcile extension placement and action model to the accepted UX
6. add the defending validation layers before widening support wording

Do not invert this order by polishing browser affordances ahead of local
approval, draft, and cockpit behavior.

---

## Feature-by-feature bundling audit

Use this test for `0.1.x`:

- `keep`: this needs to exist as a distinct durable destination, command, or
  contextual action because bundling it away would make Roger less truthful or
  less usable
- `bundle`: this is real product functionality, but it should live inside a
  stronger primitive rather than becoming its own screen or command
- `defer`: this is valid, but it is not needed for the first release contract

### TUI audit

| Feature | Schema anchor | Decision | Bundle into | Reason |
| --- | --- | --- | --- | --- |
| Review Home / attention queue | `ReviewSession`, `AttentionState` | keep | — | Roger needs one durable "what needs me now?" cockpit entrypoint. |
| Session management window | `ReviewSession`, `ReviewRun`, `AttentionState` | keep | — | Cross-repo session finding and active-session switching need a first-class destination. |
| Session overview | `ReviewSession`, `ReviewRun`, `AttentionState` | bundle | Home + Sessions inspector | It is required information, but not a separate first-release destination. |
| Findings queue | `Finding`, `FindingFingerprint` | keep | — | `Finding` is the primary operator primitive and needs its own dense queue. |
| Finding inspector | `Finding`, `CodeEvidenceLocation`, clarification lineage | bundle | persistent inspector region | It is critical, but it should be the shared detail region, not a route. |
| Draft approval queue | `OutboundDraft`, `OutboundDraftBatch`, `PostedAction` | keep | — | Approval and posting are elevated enough to deserve a distinct workspace. |
| Timeline and history | `ReviewRun`, `PromptInvocation`, `PostedAction` | bundle | combined Search/History destination | Useful, but first release should combine it with recall/search instead of adding another top-level tab. |
| Search and recall | scoped searchables over sessions/findings/artifacts | keep | combined Search/History destination | Search is a core continuity promise and needs a durable destination. |
| Focusable work-queue model | controller state over canonical ids | bundle | all queue views | Foundational primitive, not a user-facing top-level feature. |
| Stable selection set | controller state over `Finding` / `OutboundDraft` ids | bundle | Findings, Drafts, Search | Mandatory capability, but it should live inside queue interactions. |
| Multi-select and batch actions | `Finding`, `FindingDecisionEvent` | bundle | Findings queue | Essential workflow power, but not a separate surface. |
| Composer | `PromptInvocation` plus selection context | keep | overlay/drawer | Roger needs one bounded action surface for clarify/chat/follow-up. |
| Finding reference syntax `@finding(<id>)` | `Finding` identity | bundle | Composer | Required affordance, but only as part of the composer. |
| Finding-bound clarification | clarification lineage + `PromptInvocation` | bundle | Composer | Required behavior, but it should share one action surface with chat/follow-up. |
| Session-local chat | `ReviewSession`, `PromptInvocation` | bundle | Composer | Needed, but bounded and not a separate chat product. |
| Prompt palette | `PromptPreset`, `PromptInvocation` | bundle | overlay/drawer | Real operator tool, but not a full peer workspace. |
| Session baseline prompt model | `PromptPreset`, `PromptInvocation`, `ReviewSession` | bundle | Session overview + Prompt Palette | Required for truth and auditability, but not its own screen. |
| Run modifiers | `PromptInvocation` | bundle | Composer + Prompt Palette | Per-run controls belong where the run is launched. |
| Active session baseline changes | `ReviewSession`, `PromptInvocation` | bundle | Session overview + Prompt Palette | Needed, but as explicit bounded controls rather than a separate feature area. |
| Elevated mutation gate | `OutboundDraftBatch`, `PostedAction` | bundle | Draft queue + confirmations | Critical rule, but it should shape the draft surface rather than add another destination. |
| Dropout and return bridge | `ReviewSession`, harness linkage | bundle | Composer / inspector / `rr return` actions | Required escape hatch, but fundamentally an action, not a screen. |
| Help overview | UI only | bundle | `?` overlay | Necessary discoverability, but not a route. |
| Mouse support | UI only | bundle | cross-cutting | Important convenience layer, not its own feature slice. |
| Diff-aware refresh comparison | `Finding`, `FindingFingerprint`, `ReviewRun` | bundle | Search/History + Findings inspector | Important, but best expressed as a mode within history/inspection. |
| Approval invalidation banner | `OutboundDraftBatch`, attention state | bundle | Drafts + Session overview | Required warning state, not a destination. |
| Posting failure recovery | `OutboundDraftBatch`, `PostedAction` | bundle | Draft queue inspector | Necessary recovery path, but part of the draft lane. |
| Open primary / all evidence | `CodeEvidenceLocation` | bundle | Finding inspector actions | Important handoff, but it is an inspector action. |
| Search jump-to-session / jump-to-finding | `ReviewSession`, `Finding` | bundle | Search results | Pure navigation affordance. |
| Quick-copy ids / anchors / handoff commands | canonical ids + UI only | bundle | inspector / context menus | Helpful, but should not become dedicated UI. |
| Timeline drilldown into prompt invocations and stage outputs | `PromptInvocation`, run history | bundle | Search/History inspector | Important observability, but within history. |
| Usefulness labeling UI | `OutcomeEvent` / optional human labels | defer | later in finding/prompt detail | Valuable for analytics, but not first-release critical. |
| Launch-target picker | launch-intake target + UI focus target | bundle | `rr open` / extension handoff | Real need, but should stay inside existing launch flows. |
| Session compare view | `ReviewSession`, `ReviewRun` | defer | later, if needed | Too niche for first release; strong summaries and per-session history are enough. |

### CLI audit

| Feature | Schema anchor | Decision | Bundle into | Reason |
| --- | --- | --- | --- | --- |
| `rr open` or `rr tui` | `ReviewSession` launch / focus | keep | — | Roger needs one explicit dense-workspace entry command. |
| `rr sessions` | `ReviewSession`, `AttentionState` | keep | — | Global session finding is a first-class continuity need. |
| `rr status` | `AttentionState`, session health | keep | — | Canonical local status must be visible directly. |
| `rr findings` | `Finding` | keep | — | Scriptable/local inspection needs a stable command. |
| `rr search` | scoped search over Roger memory | keep | — | Search is a product promise, not a hidden subcommand. |
| `rr draft` | `OutboundDraft`, `OutboundDraftBatch` | keep | — | Outbound transition needs a first-class CLI surface. |
| `rr approve` | `OutboundDraftBatch` | keep | — | Approval is a core explicit mutation gate. |
| `rr post` | `PostedAction` | keep | — | Posting must remain explicit and auditably separate. |
| `rr doctor` | setup/bridge/store health | keep | — | Truthful recovery and preflight need one canonical command. |
| `rr extension uninstall` | extension install state | keep | `rr extension ...` family | It belongs in the product-facing browser lane. |
| compact session picker | `ReviewSession` | bundle | `rr open` + `rr sessions` | Needed behavior, but not a separate command family. |
| `rr history` | `ReviewRun`, `PromptInvocation`, `PostedAction` | bundle | `rr search` or later optional command | Useful, but can stay bundled unless the CLI history lane grows large enough. |
| bridge / repair commands | bridge envelopes only | bundle | demoted dev/repair lane | Necessary for maintainers, but should stay out of product help. |

### Extension audit

| Feature | Schema anchor | Decision | Bundle into | Reason |
| --- | --- | --- | --- | --- |
| inline/header seam | launch + attention mirror | bundle | default PR-page placement rule | Needed behavior, but not a separate user feature. |
| right-rail host | launch + attention mirror | bundle | fallback/companion placement | Real need, but only as a placement fallback. |
| modal fallback | launch + attention mirror | bundle | degraded placement fallback | Necessary only when normal seams fail. |
| toolbar popup backup | launch only | bundle | manual backup path | Important fallback, not the main surface. |
| `Start` | launch intent | keep | contextual PR-page action | Needed when no local session exists. |
| `Resume` | `ReviewSession`, `AttentionState` | keep | contextual PR-page action | Needed when a local session already exists. |
| `Findings` | `Finding`, `AttentionState` | keep | contextual PR-page action | Needed when findings are the next truthful local focus. |
| `Refresh review` | refresh recommendation + target revision | keep | contextual PR-page action only | Needed, but only when Roger recommends refresh. |
| `Open Draft Queue` | `OutboundDraftBatch`, `AttentionState` | keep | contextual PR-page action | Needed for approval-required state. |
| `Open in Roger` | `ReviewSession` | keep | contextual PR-page action | Needed as the broad recovery/open-local affordance. |
| `Open Session Overview` | `ReviewSession` | bundle | `Open in Roger` with focus | Not worth a separate named action. |
| bounded status + freshness mirror | `AttentionState` | bundle | inline/rail host | Required, but as mirrored UI inside the existing host. |
| extension help surface | UI only | bundle | settings/help surface | Needed, but not a PR-page primary action. |
| extension settings | UI only | bundle | settings/help surface | Real need, but should stay bounded. |
| keyboard shortcuts | UI only | bundle | help/settings | Helpful, but not a separate feature track. |
| Native Messaging refresh / ping-host buttons | bridge transport only | defer / reject | automatic checks + `doctor` | This is plumbing, not operator intent, and should not be ordinary UI. |

---

## Supported user stories on top of this narrowed feature set

The point of bundling is not to make Roger smaller in capability. It is to make
the first release sharper and more coherent.

The following user stories should still be supported truthfully by the narrowed
`0.1.x` cut.

### Core review loop

| User story | Primary surfaces | Backing primitives |
| --- | --- | --- |
| As a reviewer, I can open Roger and immediately see which local review session needs my attention now. | TUI Home, `rr status`, `rr sessions` | `ReviewSession`, `AttentionState` |
| As a reviewer, I can jump into the right session for the current repo or PR without manually hunting for ids. | `rr open`, TUI Sessions, extension Start/Resume | `ReviewSession`, launch resolution |
| As a reviewer, I can scan findings quickly, open one, and inspect its evidence without losing my place in the queue. | Findings queue + inspector | `Finding`, `CodeEvidenceLocation` |
| As a reviewer, I can triage many similar findings together instead of repeating the same action one by one. | Findings queue batch actions | `Finding`, `FindingDecisionEvent` |
| As a reviewer, I can move from findings to local drafts and explicitly approve or reject outbound comments before anything is posted. | Draft queue, `rr draft`, `rr approve`, `rr post` | `OutboundDraft`, `OutboundDraftBatch`, `PostedAction` |
| As a reviewer, I can recover locally when posting fails and still see exactly what payload Roger tried to send. | Draft queue inspector | `OutboundDraftBatch`, `PostedAction` |

### Memory, continuity, and recall

| User story | Primary surfaces | Backing primitives |
| --- | --- | --- |
| As a reviewer, I can resume prior review work instead of losing continuity when I leave and come back later. | TUI Sessions, `rr resume`, extension Resume | `ReviewSession`, `ReviewRun` |
| As a reviewer, I can search prior findings, summaries, artifacts, and session history without leaving the current review context. | Search/History, `rr search` | scoped search index + Roger durable objects |
| As a reviewer, I can tell what changed since the last run and which findings are new, carried forward, stale, or resolved. | Search/History, findings inspector, refresh comparison | `Finding`, `FindingFingerprint`, `ReviewRun` |
| As a reviewer, I can reopen the exact local context for a prior session even if I started it from the browser earlier. | Sessions, `rr open`, extension Open in Roger | `ReviewSession`, launch history |
| As a reviewer, I can keep one durable review cockpit while still dropping out to the underlying harness temporarily and returning later without losing session continuity. | TUI dropout, underlying harness, `rr return` | harness linkage, `ReviewSession` |
| As a reviewer, I can inspect the active session baseline so I understand what Roger is carrying forward from prior runs or repo-local context. | Session overview, Prompt Palette | `ReviewSession`, `PromptInvocation`, resolved context |

### Prompt inclusion, steering, and bounded chat

| User story | Primary surfaces | Backing primitives |
| --- | --- | --- |
| As a reviewer, I can choose a prompt preset for the next run instead of relying on hidden prompt magic. | Prompt Palette, composer, bounded web ingress | `PromptPreset` |
| As a reviewer, I can add a short explicit objective to sharpen one run without changing Roger's whole configuration. | Prompt Palette, composer, bounded intake | `PromptInvocation`, `explicit_objective` |
| As a reviewer, I can reference one or more findings directly in chat or follow-up using `@finding(<id>)` rather than copying ids manually. | Composer | `Finding`, `PromptInvocation` |
| As a reviewer, I can ask Roger to explain or clarify a selected finding without mutating its triage state. | Composer clarify mode | clarification lineage, `PromptInvocation` |
| As a reviewer, I can have a short session-local chat inside the TUI while Roger remains the control surface. | Composer session-chat mode | `ReviewSession`, `PromptInvocation` |
| As a reviewer, I can see exactly which preset, objective, and selected findings were included in a run after the fact. | Prompt/history inspector | `PromptInvocation` |
| As a reviewer, I can change the session baseline for future runs only, with a visible boundary, rather than silently rewriting earlier prompt history. | Session overview, Prompt Palette | `ReviewSession`, `PromptInvocation` |
| As a reviewer, I can reuse recent, frequent, or favorite prompt presets without needing a second prompt-management product. | Prompt Palette | `PromptPreset`, `PromptInvocation` reuse projections |

### Evidence handoff and bounded escape hatches

| User story | Primary surfaces | Backing primitives |
| --- | --- | --- |
| As a reviewer, I can open the primary code evidence for a finding directly in my editor. | Finding inspector | `CodeEvidenceLocation` |
| As a reviewer, I can open all relevant evidence for a finding when one location is not enough. | Finding inspector | `CodeEvidenceLocation` set |
| As a reviewer, I can drop out to the underlying harness when the bounded Roger chat lane is no longer enough. | TUI dropout action | harness linkage |
| As a reviewer, I can still return to the same Roger session after working directly in the harness. | `rr return`, resume flows | `ReviewSession` continuity |

### Browser entry and local handoff

| User story | Primary surfaces | Backing primitives |
| --- | --- | --- |
| As a browser-first reviewer, I can start a local Roger review from a PR page with one clear action. | extension Start | launch intake |
| As a browser-first reviewer, I can resume an existing local session from the PR page instead of starting over. | extension Resume | `ReviewSession`, `AttentionState` |
| As a browser-first reviewer, I can open the local findings view when findings are already ready. | extension Findings | `Finding`, `AttentionState` |
| As a browser-first reviewer, I can open the local draft queue when approval is the next real task. | extension Open Draft Queue | `OutboundDraftBatch`, `AttentionState` |
| As a browser-first reviewer, I can trigger a real review refresh only when Roger says refresh is the right operator move. | extension Refresh review | `refresh_recommended`, target revision |
| As a browser-first reviewer, I do not have to understand Native Messaging plumbing to use Roger successfully. | extension inline/rail host + `rr doctor` when needed | user-intent action model |

### Setup, install, update, and recovery

| User story | Primary surfaces | Backing primitives |
| --- | --- | --- |
| As a new operator, I can install Roger and get a truthful local CLI/TUI product without first learning bridge internals. | install path, `rr doctor`, docs | product install contract |
| As a browser user, I can run extension setup through a product-facing flow instead of manually wiring extension ids and host paths. | `rr extension setup` | extension setup contract |
| As an operator, I can verify whether Roger, the local store, and the browser bridge are healthy through one canonical doctor path. | `rr doctor`, `rr extension doctor` | health and recovery checks |
| As an operator, I can update the installed Roger binary through one explicit command with confirmation and fail-closed behavior. | `rr update` | update contract |
| As an operator, I can safely tell the difference between product usage and repair actions because bridge plumbing is demoted out of the normal help path. | `rr --help`, extension help, `doctor` | command-surface layering |
| As an operator, I can uninstall the browser companion through a real product-facing path if I want to remove it cleanly. | `rr extension uninstall` | extension lifecycle |

### Explicit non-stories for the first cut

These are intentionally not part of the narrowed first-release contract:

- a dedicated session-compare workspace
- in-browser approval or posting
- a second unconstrained prompt-authoring product inside the extension
- ordinary PR-page buttons for Native Messaging refresh, ping-host, or other
  bridge-plumbing verbs
- a general-purpose harness replacement UI inside the TUI

---

## Resolved decisions from this pass

The following surface choices are now the recommended `0.1.x` default:

1. TUI finding references should use `@finding(<id>)`.
2. The TUI should support bounded session-local chat as well as finding-bound
   clarification.
3. The default dropout target from the TUI should be the underlying harness.
4. Session-baseline changes, when allowed, should create a visible forward-only
   run-mode boundary rather than silently mutating prior history.
5. Operator-facing browser actions should stay user-intent centered rather than
   exposing Native Messaging plumbing or similar bridge-maintenance verbs.

## Open questions that need explicit decisions

These are the most important remaining UX ambiguities from this pass:

1. What bulk actions should the session-management window allow beyond safe
   navigation, archival, and pinning, if any?
2. Should prompt-palette categories be purely Roger-preset driven, or should
   Roger also ingest battle-tested prompt groups from repo-local palette files
   as import sources?
3. How far should mouse support go in `0.1.x`: additive convenience only, or
   full click-first parity for most ordinary cockpit operations?
4. Should the extension inline seam show a single primary action plus overflow,
   or a compact two-action model such as primary plus open-locally?

These should be resolved deliberately, not ad hoc in whichever slice lands
first.

---

## Bottom line

Roger's current problem is not that the project needs a pre-polish rollback.

The real problem is that:

- the TUI is still thinner than the intended cockpit
- the CLI still lacks explicit product-complete operator flows
- the extension still needs a stronger targeted-handoff UX contract
- the graph and validation posture no longer reflect those realities

This brief is the recommended bridge between the canonical plan and the next
honest implementation wave.
