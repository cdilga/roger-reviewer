# Critique Round 02 for Roger Reviewer

This round focuses on the places where the current plan is still too soft for
implementation: the daemonless claim, extension realism, OpenCode durability,
storage and instance design, rollout order, GitHub write semantics, safety, and
fuzzy carry-over from the brain dump.

## Highest-Value Revisions

### 1. Make the v1 extension a launch surface, not a live local-status surface

Reasoning:

The current plan still implies that the extension can reflect local Roger state
on the PR page while also remaining daemonless. That is the weakest part of the
plan. A browser extension can realistically launch a local app from an explicit
user gesture, but reliable local status reflection usually drifts toward native
messaging queries, localhost services, or another hidden control plane.

Problem solved:

This narrows v1 to a bridge that is actually achievable without violating the
daemonless requirement. It also makes the fallback story honest instead of
pretending the extension will know local review state before that path exists.

Proposed diff:

```diff
--- a/docs/PLAN_FOR_ROGER_REVIEWER.md
+++ b/docs/PLAN_FOR_ROGER_REVIEWER.md
@@
-2. Extension injects a Roger action button and status indicator.
+2. Extension injects a Roger action button and launch menu.
@@
-6. Review progress and unresolved findings become visible both locally and, at a
-   minimum, as extension-readable status.
+6. Review progress and unresolved findings become visible locally.
+7. In v1 the extension does not promise live local-state reflection. If the
+   bridge is unavailable, it falls back to copying an equivalent `rr` command.
@@
-It should provide:
-
-- PR-aware button injection
-- launch/resume actions
-- status indicator for unresolved or unapplied findings
-- ability to add prompts or review actions from the PR page
-- GitHub-specific shortcut integration where practical
+It should provide:
+
+- PR-aware button injection
+- launch/resume actions
+- explicit fallback UI that copies an equivalent local `rr` command
+- ability to add prompts or review actions from the PR page
+- GitHub-specific shortcut integration where practical
@@
-Preferred direction:
-
-- use a native-launch or custom-protocol handoff that can invoke a local Roger
-  command with structured PR context
-- ensure there is a fallback if the bridge is unavailable, such as copying a
-  ready-to-run local command
+Preferred direction for v1:
+
+- register a custom protocol such as `roger-reviewer://` that is triggered only
+  from an explicit user gesture
+- have the protocol handler translate the launch payload into a local `rr`
+  command
+- if protocol launch is unavailable, copy a ready-to-run `rr review --repo ...`
+  command instead of attempting a live bridge
+- reject localhost HTTP servers, background polling, or a persistent native host
+  as v1 requirements
@@
-The exact bridge mechanism needs a validation spike because browser security
-models can easily force architecture drift. This should be treated as an early
-technical-risk spike, not deferred indefinitely.
+The exact platform registration flow still needs a validation spike, but v1
+should optimize for a user-gesture custom-protocol handoff rather than a status
+sync channel.
@@
-### Gate D: GitHub bridge realism
+### Gate D: GitHub bridge realism
@@
-- the extension status story is realistic for v1 rather than pretending to be
-  live-synced without infrastructure
+- the extension succeeds as a launch surface even without live local status
+- no localhost server or always-on background process is required
```

### 2. Make OpenCode fallback real with Roger-owned durability, not just mapping

Reasoning:

The current plan says every Roger session maps to an OpenCode session or
transcript anchor. That is not enough to preserve the fallback if OpenCode
compacts, changes its internal storage, or loses the original session. Roger
needs its own durable resume bundle and run-state model.

Problem solved:

This turns "fallback to plain OpenCode" from a slogan into a recovery path that
survives compaction, crashes, and interrupted runs without losing finding
lineage.

Proposed diff:

```diff
--- a/docs/PLAN_FOR_ROGER_REVIEWER.md
+++ b/docs/PLAN_FOR_ROGER_REVIEWER.md
@@
 - `ReviewRun`
+- `ReviewRunState`
@@
 - `PostedAction`
+- `SessionLocator`
+- `ResumeBundle`
@@
 Roger should wrap an OpenCode session rather than replace it.
@@
 - every Roger review session maps to an underlying OpenCode session or transcript
   anchor
+- Roger also owns a durable session ledger outside OpenCode internals
 - Roger stores additional structured metadata outside that session
 - if Roger UI state is unavailable, the user can still reopen the OpenCode
   session directly
 - compaction recovery should be able to reinsert selected artifacts, prior
   findings, and prompt-stage summaries into a resumed session
@@
+### Durability and recovery
+
+- every durable stage boundary writes a `ResumeBundle` containing the current
+  review target, prompt-stage summaries, surviving findings, and artifact
+  references
+- `ReviewRun` instances must distinguish `running`, `interrupted`, `completed`,
+  `failed`, and `cancelled`
+- resume first attempts to reopen the recorded OpenCode session; if that fails,
+  Roger creates a new OpenCode session and seeds it from the latest
+  `ResumeBundle`
+- no Roger feature may depend on undocumented OpenCode internals when the same
+  outcome can be reached through a CLI or file-level contract
@@
 Minimum expectations:
@@
 - create or link to a session
- capture enough identifiers to reopen the same session later
- reinsert compact context bundles when resuming
+- capture a `SessionLocator` with enough information to reopen the same session
+  later
+- write a Roger-owned `ResumeBundle` after each durable stage boundary
+- support recovery by seeding a fresh OpenCode session when the original one
+  cannot be reopened
 - avoid depending on fragile internal implementation details if a stable CLI or
   file-level boundary exists
@@
 - plain OpenCode fallback and resume
+- resume after an interrupted run or after the original OpenCode session has
+  been compacted away
@@
 - findings survive restart
+- interrupted or compacted sessions can be recovered without losing finding
+  lineage
```

### 3. Simplify v1 storage to one canonical local store with WAL and locks

Reasoning:

The current plan still carries the brain-dump idea that named instances should
copy DB state from a primary instance with fast diffing. That is high-complexity
state sync before the base product exists. It also conflicts with local-first
search because the best search surface is one canonical store.

Problem solved:

This removes a likely source of corruption, migration pain, and multi-instance
confusion. It also makes search, resume, and audit simpler because every surface
looks at the same durable store.

Proposed diff:

```diff
--- a/docs/PLAN_FOR_ROGER_REVIEWER.md
+++ b/docs/PLAN_FOR_ROGER_REVIEWER.md
@@
 ### Source of truth
@@
 Use a local SQLite-family database as the canonical store for review sessions,
 findings, artifacts, status, and index metadata.
+
+### Store layout and locking
+
+- use one canonical Roger store per user profile under the platform app-data
+  directory rather than copying DB state per worktree or per app instance
+- place large raw artifacts in a sibling content-addressed artifact store keyed
+  by digest
+- enable SQLite WAL mode and use an advisory per-session lock so only one
+  writer mutates a review session at a time
+- treat export/import as an explicit portability workflow rather than implicit
+  background synchronization
@@
-Use worktrees as the default isolation unit for active reviews or review-driven
-follow-up work.
+Use the canonical Roger store as the default state location. Worktrees and local
+installs may point at that store, but they are not separate sources of truth.
@@
- multiple local copies of the app can coexist as named instances
- instances should be able to copy relevant DB state from a primary instance
-  efficiently rather than cloning everything blindly
+- multiple local installs or binary channels may coexist, but they should point
+  at the same canonical store unless the user explicitly creates a separate
+  profile
+- v1 should not attempt fast-diff DB copy between instances
+- explicit export/import is acceptable for later portability workflows
@@
-Open question:
-
-The fast-diffing DB copy strategy needs concrete design work. For v1, it may be
-enough to start with conservative snapshot export/import semantics before
-optimizing incremental transfer.
+Deferred for later:
+
+- profile-to-profile export/import may be added after the single-store model is
+  proven
```

### 4. Make worktrees opt-in for elevated flows, not the default review path

Reasoning:

The plan currently treats worktrees as the default isolation unit. That is too
heavy for a read-mostly review product. Most sessions only need a stable repo
snapshot and local findings. Worktrees should exist for isolated execution,
code-changing workflows, or conflicting local state, not as the default tax on
every review.

Problem solved:

This improves local-first UX, reduces repo churn, and aligns worktree creation
with the safety model instead of with ordinary review.

Proposed diff:

```diff
--- a/docs/PLAN_FOR_ROGER_REVIEWER.md
+++ b/docs/PLAN_FOR_ROGER_REVIEWER.md
@@
-5. Roger creates or reuses a local review instance, prepares a worktree if
-   needed, and opens the TUI or CLI flow.
+5. Roger creates or reuses a local review session, records the current repo
+   snapshot, and opens the TUI or CLI flow.
+6. Roger creates a dedicated worktree only when the user requests isolated
+   execution or enters an elevated mutation-capable mode.
@@
-Use worktrees as the default isolation unit for active reviews or review-driven
-follow-up work.
+Review flows default to the current checkout plus recorded base/head commits.
+Worktrees are created only for isolated execution, code changes, or conflicting
+local repo states.
@@
 ### Local environment protection
@@
 - runtime should distinguish read-only review flows from mutation-capable flows
 - mutation-capable flows should be visibly elevated, not implicit
+- mutation-capable flows should run in a dedicated worktree or similar isolated
+  environment by default
```

### 5. Split GitHub read, GitHub write, and extension work into separate phases

Reasoning:

The current rollout still bundles GitHub integration and the extension together,
even though read-only GitHub ingestion is a much earlier dependency than either
posting or the browser bridge. The plan should prove the core loop locally
first, then add read-only remote context, then safe write-back, and only then
the extension.

Problem solved:

This defers the riskiest integrations until after the core review loop works,
while still acknowledging that realistic PR review eventually needs read-only
GitHub data before extension work.

Proposed diff:

```diff
--- a/docs/PLAN_FOR_ROGER_REVIEWER.md
+++ b/docs/PLAN_FOR_ROGER_REVIEWER.md
@@
 ### Contract 3: Outbound posting boundary
@@
 - local state records success, failure, and remote identifiers
+
+### Contract 4: GitHub read boundary
+
+Roger must separate read-only GitHub ingestion from GitHub mutation and from
+the browser extension.
+
+Minimum expectations:
+
+- resolve PR metadata and diff context through `gh` CLI or another explicit
+  adapter
+- cache imported snapshots locally so review can continue offline
+- allow the core review loop to run against local git state even when GitHub is
+  unavailable
@@
 Capabilities for v1:
@@
-- resolve PR metadata
-- fetch diff and review context
-- draft comments/questions/suggestions locally
-- post approved outputs back through `gh` CLI or another explicit adapter
+- read path: resolve PR metadata and fetch diff/review context
+- read path: cache imported snapshots locally for offline resume
+- write path: draft comments/questions/suggestions locally
+- write path: post approved outputs back through `gh` CLI or another explicit
+  adapter
@@
 ### Phase 2: CLI and prompt engine
@@
 - implement session-aware CLI
 - implement review-stage orchestration
 - persist structured findings and artifacts
-- prove that a local review loop works without the extension
+- prove that a local git-driven review loop works without GitHub or the
+  extension
@@
-### Phase 3: TUI
+### Phase 3: TUI
@@
 - implement TUI shell
 - add findings list/detail/action flows
 - add outbound draft approval UX
 - validate refresh and resume behavior
@@
-### Phase 4: GitHub integration and extension
+### Phase 4: Read-only GitHub integration
+
+- resolve PR metadata and diff snapshots through `gh`
+- validate refresh against real PR updates without enabling posting
+- prove the review loop survives with cached local snapshots
+
+### Phase 5: Outbound posting
+
+- finalize GitHub write adapter behavior
+- implement draft approval and posting
+- prove idempotent retry and audit behavior
+
+### Phase 6: Extension bridge
@@
-- finalize GitHub adapter behavior
 - validate the daemonless bridge approach
-- implement extension injection and status surfaces
+- implement extension injection and launch/fallback UI
 - prove launch/resume from PR pages
-
-### Phase 5: Search, memory, and polish
+
+### Phase 7: Search, memory, and polish
```

### 6. Model GitHub outbound actions as review batches, not only per-finding drafts

Reasoning:

GitHub review semantics are not purely one finding to one comment. Roger will
need grouped review drafts, rendered previews, thread targeting, and partial
failure handling. The current `OutboundDraft` plus `PostedAction` model is too
thin if the product is serious about approval, audit, and retries.

Problem solved:

This makes the GitHub adapter realistic before it is built. It also prevents the
approval model from collapsing once multiple findings need to become one GitHub
review submission.

Proposed diff:

```diff
--- a/docs/PLAN_FOR_ROGER_REVIEWER.md
+++ b/docs/PLAN_FOR_ROGER_REVIEWER.md
@@
 - `ConfigLayer`
 - `OutboundDraft`
+- `OutboundDraftBatch`
 - `PostedAction`
@@
 ### Contract 3: Outbound posting boundary
@@
 - outbound drafts are materialized locally first
 - approval is explicit and reviewable
 - the exact payload posted to GitHub is snapshotted for audit
 - local state records success, failure, and remote identifiers
+- multiple finding-derived drafts may be grouped into a single review batch with
+  per-item lineage
@@
 ### GitHub write path
@@
- drafts are prepared locally first
- user reviews or edits them
- Roger posts via adapter only after confirmation
- posted state is persisted locally and linked back to the finding
+- drafts are prepared locally first and grouped into an explicit review batch
+- user reviews or edits the rendered batch before approval
+- Roger posts via adapter only after confirmation
+- posted state is persisted locally with per-item remote identifiers and failure
+  states
```

### 7. Add artifact classification, redaction, and credential boundaries

Reasoning:

The plan already takes GitHub posting safety seriously, but it still treats local
storage and indexing as if every artifact were equally safe. That is not true
for prompts, cached diffs, environment-derived data, or credentials. The
Keychain note should become an explicit boundary: credentials stay outside the
primary Roger store.

Problem solved:

This reduces the risk that local-first storage becomes a local secret dump, and
it makes outbound posting safer because the same artifact classification can
block or redact sensitive content before review comments are rendered.

Proposed diff:

```diff
--- a/docs/PLAN_FOR_ROGER_REVIEWER.md
+++ b/docs/PLAN_FOR_ROGER_REVIEWER.md
@@
 ### Artifact strategy
@@
 - Store metadata and normalized excerpts in the database.
 - Store larger raw artifacts in a local content-addressed artifact directory if
   they become too large for comfortable inline DB storage.
 - Keep database rows small enough that the TUI remains responsive.
 - Define artifact budget classes early so prompt transcripts, diff chunks, and
   large reference payloads do not bloat the primary tables accidentally.
+- classify artifacts before indexing or reuse as `indexable`,
+  `sensitive-local`, `credential-derived`, or `outbound-blocked`
+- never store raw credentials or secret material in the primary database or FTS
+  tables; store only redacted excerpts or external references
@@
 ### GitHub write path
@@
 - drafts are prepared locally first and grouped into an explicit review batch
 - user reviews or edits the rendered batch before approval
 - Roger posts via adapter only after confirmation
 - posted state is persisted locally with per-item remote identifiers and failure
   states
+- outbound content passes a final redaction and sensitivity check before posting
@@
 ### Manual validation
@@
 - refresh after new commits
 - explicit approval before posting
 - plain OpenCode fallback and resume
+- secret-like content is redacted before indexing or posting
```

### 8. Do not let FrankenTUI decide the whole runtime architecture

Reasoning:

The current technology section says the repo should follow FrankenTUI if it
strongly favors a specific runtime. That is upside down. The TUI is one adapter.
The durable review core, storage, session boundary, and GitHub adapter are the
product. If FrankenTUI cannot consume those cleanly, the TUI library should be
replaceable.

Problem solved:

This protects the architecture from being distorted by a UI choice and turns a
fuzzy open question into a rule.

Proposed diff:

```diff
--- a/docs/PLAN_FOR_ROGER_REVIEWER.md
+++ b/docs/PLAN_FOR_ROGER_REVIEWER.md
@@
-### Open question
-
-The exact TUI/runtime choice depends on how FrankenTUI wants to be consumed. If
-FrankenTUI strongly favors a specific runtime or framework, the repo should
-follow that rather than forcing a mismatched stack.
+### TUI runtime rule
+
+FrankenTUI is a candidate adapter, not an architectural driver.
+
+- `app-core`, `storage`, `session-opencode`, and Git/GitHub adapters define the
+  stable contracts
+- the TUI must consume those contracts from the outside rather than forcing the
+  repo to reorganize around a TUI-specific runtime
+- if FrankenTUI cannot fit that boundary cleanly, Roger should replace the TUI
+  library rather than distort the core architecture
```

### 9. Remove undefined brain-dump shorthand from the canonical plan

Reasoning:

Phase 0 already says to isolate undefined terms from the brain dump. The
canonical plan should go one step further: undefined shorthand should not remain
as a bare open question unless it is concrete enough to assign an owner, define
data shape, and attach an acceptance test.

Problem solved:

This keeps the plan from smuggling scope creep and vague memory-system ideas
into implementation.

Proposed diff:

```diff
--- a/docs/PLAN_FOR_ROGER_REVIEWER.md
+++ b/docs/PLAN_FOR_ROGER_REVIEWER.md
@@
 - lock the canonical name as Roger Reviewer
 - define minimum v1 surface area
-- isolate undefined terms from the brain dump as open questions
+- remove undefined brain-dump shorthand from the canonical plan unless it is
+  concrete enough to define a data shape, owner, and acceptance test
 - decide what absolutely must ship before extension work starts
@@
-- What exactly do `FPs` and `SA` mean in the brain dump, and what data model do
-- they require?
+- If `FPs` and `SA` matter for v1, define them in a short glossary with size
+  limits, retrieval workflow, and data shape before implementation starts
```

## Integrated Direction

If these revisions are applied, the plan becomes materially stronger in four
ways:

- the daemonless requirement becomes testable rather than aspirational
- the OpenCode fallback becomes durable under compaction and crashes
- the local-first storage model becomes simpler and safer
- rollout sequencing separates safe read paths from risky write and browser work

## Remaining Open Questions After These Revisions

These are the questions I would still allow to remain open after revising the
plan:

- what exact platform installation steps are needed to register the custom
  protocol handler cleanly
- what thin adapter is required if FrankenTUI remains the chosen TUI library
- what is the smallest useful semantic-search layer after FTS proves its value
