# UX Surface Audit For Memory And Agent Access

Status: Proposed
Class: bounded side-plan / UX audit
Audience: maintainers reconciling TUI, CLI/harness, and extension surfaces against Roger’s memory and active-agent architecture

Primary references:

- [`ROUND_05_SURFACE_RECONCILIATION_BRIEF.md`](./ROUND_05_SURFACE_RECONCILIATION_BRIEF.md)
- [`TUI_WORKSPACE_AND_OPERATOR_FLOW_CONTRACT.md`](./TUI_WORKSPACE_AND_OPERATOR_FLOW_CONTRACT.md)
- [`ATTENTION_EVENT_AND_NOTIFICATION_CONTRACT.md`](./ATTENTION_EVENT_AND_NOTIFICATION_CONTRACT.md)
- [`HARNESS_SESSION_LINKAGE_CONTRACT.md`](./HARNESS_SESSION_LINKAGE_CONTRACT.md)
- [`ROBOT_CLI_CONTRACT.md`](./ROBOT_CLI_CONTRACT.md)
- [`AGENT_ACCESS_AND_IN_SESSION_OPERATION_CONTRACT.md`](./AGENT_ACCESS_AND_IN_SESSION_OPERATION_CONTRACT.md)

---

## Purpose

This audit captures how the three main UX surfaces currently line up against the
new memory and active-agent architecture:

- CLI/harness/robot
- TUI
- browser extension

It focuses on:

- memory/search access
- active-agent operation
- truthful degraded behavior
- approval and posting boundaries

---

## Short answer

All three surfaces are directionally aligned with Roger’s architecture, but none
is fully reconciled yet.

- CLI/harness is the closest thing Roger currently has to an active-agent
  surface, but it is narrower than some docs imply.
- TUI is still more read-mostly shell than full operator workbench.
- Extension is correctly narrow, but still has minor drift and one blurred
  action model around approval.

---

## CLI, harness, and robot surface

### What is already right

- `rr --robot` is already the correct default machine-readable surface.
- repo-first search is the correct sensible default.
- degraded lexical-only search is surfaced explicitly.
- unsupported provider capabilities block truthfully instead of pretending
  parity.
- `rr return` is bounded correctly to the provider(s) that actually support it.

### Confirmed drift

- the robot envelope timestamp does not match the frozen contract
- `drafts.awaiting_approval` currently appears to count approved findings rather
  than truly awaiting-approval draft batches
- the broader in-session command story in docs is ahead of the live safe subset
- `rr search` is structurally correct, but still lacks explicit control knobs
  for overlays, tentative candidates, and richer memory-lane selection
- the robot-docs inventory is slightly ahead of the frozen shortlist in one
  spot

### Sensible defaults

- keep `rr --robot` read/query and dry-run only
- keep search repo-scoped by default
- keep clarification and open-drafts as follow-on capabilities until the core
  contract is implemented
- keep unsupported harness commands failing closed to the `rr` equivalent

### Default reconciliation

- CLI/harness should become the canonical active-agent read/query plane
- do not widen it into approval or posting authority

---

## TUI surface

### What is already right

- read-mostly posture is preserved
- findings, lineage, degraded markers, and draft queue state already exist in
  the shell
- attention and continuity state are carried in session chrome
- no automatic posting or hidden mutation bypass is present

### Confirmed drift

- the TUI workspace is still narrower than its support contract
- memory recall, search/history, composer, prompt palette, and sessions are not
  yet durable first-class destinations
- promotion review is effectively missing
- approval and posting are modeled in data, but not yet elevated enough as a
  first-class operator surface
- canonical attention states are not yet surfaced with the full product
  semantics the contract expects
- degraded-mode visibility exists, but not yet as a complete operational
  workspace state

### Sensible defaults

- keep TUI as the authoritative operator workbench
- make TUI the place for:
  - recall inspection
  - promotion review
  - clarification review
  - draft review
  - approval handoff
  - recovery

### Default reconciliation

- do not try to solve active-agent mutation from the CLI or extension first
- complete the TUI workbench before adding richer external facades

---

## Extension surface

### What is already right

- thin PR-page launcher and status mirror is the correct role
- launch-only/no-status mode is the correct degraded fallback
- the extension is not acting as a memory/search surface
- the extension is not acting as an approval or posting surface

### Confirmed drift

- README placement language has drifted from the current rail-first
  implementation
- `awaiting_outbound_approval` still maps to a generic findings action instead
  of a more distinct draft-facing handoff
- the extension remains intentionally too small to expose more nuanced
  draft/recovery states, which is defensible for `0.1.0` but should be explicit

### Sensible defaults

- keep only the current four safe actions:
  - `start`
  - `resume`
  - `findings`
  - `refresh`
- keep memory/search out of the extension entirely
- keep no-status mode whenever authoritative readback is unavailable

### Default reconciliation

- the extension should remain a truthful mirror and launcher only
- do not grow it into a recall, promotion, approval, or posting surface

---

## Cross-surface UX inconsistencies

### U1. Approval-state semantics still blur across surfaces

- the TUI contract treats draft review and approval as first-class
- the extension collapses `awaiting_outbound_approval` into `show_findings`
- CLI status naming appears to blur approved state and awaiting-approval state

Default reconciliation:

- make “drafts need operator approval” a distinct user-facing concept across all
  surfaces, even when the extension still only deep-links into the richer local
  surface

### U2. Active-agent memory access exists architecturally, but not yet as one UX story

- CLI/robot has partial read/query access
- TUI has partial local shell state
- harness-native commands are still tiny
- extension has none by design

Default reconciliation:

- make the CLI/robot surface the canonical active-agent read/query plane
- make the TUI the canonical review/mutation plane
- keep harness commands as ergonomic mirrors only

### U3. Memory integration is present in policy, not yet visible enough in UX

The current spec now knows about:

- evidence versus promoted memory
- candidate memory
- overlays
- promotion and demotion
- degraded semantic mode

But the UX surfaces do not yet show that as a coherent operator/agent
experience.

Default reconciliation:

- prioritize recall envelopes, lane labels, and promotion review in TUI/CLI
- defer any richer browser or MCP projections until that core UX is truthful

---

## Recommended next surface work

1. Reconcile CLI/harness command truth with the current tiny safe subset.
2. Make `rr search` and `rr status` carry the richer recall and approval truth
   the contracts now imply.
3. Turn the TUI into the real memory-aware operator workbench.
4. Keep the extension thin and explicitly non-authoritative.
5. Only add a thin MCP facade after the above surfaces already align.

---

## Result

The correct UX architecture is now fairly clear:

- CLI/robot: machine-readable read/query plane
- harness commands: bounded ergonomic continuity layer
- TUI: authoritative memory-aware operator workbench
- extension: truthful mirror and launcher

That should be treated as the default until a stronger reason appears to do
otherwise.

