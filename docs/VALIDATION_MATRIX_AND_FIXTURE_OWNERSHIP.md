# Validation Matrix And Fixture Ownership

This document closes `rr-025`.

It binds Roger's major flow families, support claims, fixture families, and
validation suite ownership into one planning-stage matrix so implementation does
not fan out on vague testing promises.

Primary references:

- [`REVIEW_FLOW_MATRIX.md`](docs/REVIEW_FLOW_MATRIX.md)
- [`RELEASE_AND_TEST_MATRIX.md`](docs/RELEASE_AND_TEST_MATRIX.md)
- [`TEST_HARNESS_GUIDELINES.md`](docs/TEST_HARNESS_GUIDELINES.md)
- [`VALIDATION_INVARIANT_MATRIX.md`](docs/VALIDATION_INVARIANT_MATRIX.md)
- [`AUTOMATED_E2E_BUDGET.json`](docs/AUTOMATED_E2E_BUDGET.json)

## Governing Rules

- Roger recognizes only three conceptual validation lanes: `unit`,
  `integration`, and `e2e`.
- The suite families below are current runner-compatible subkinds, not extra
  top-level lanes.
- Roger currently carries a six-slot budget for major automated E2E journeys in
  `0.1.x`: `E2E-01` through `E2E-06`.
- Only `E2E-01 Core review happy path` is executable today; the other five are
  approved scenario slots and do not count as functional coverage until their
  suites land and run.
- Provider acceptance is separate from end-to-end testing.
- Browser-launch, bridge recovery, malformed structured output, refresh
  invalidation, and same-PR multi-instance behavior must be defended mostly by
  targeted integration-family suites or smoke tests rather than new E2Es.
- A support claim is not real unless a named suite family or manual smoke lane
  owns it.
- Every release-critical support claim should also map to one or more invariant
  ids in `VALIDATION_INVARIANT_MATRIX.md`.
- Every implementation bead that changes behavior should cite invariant ids or
  extend the invariant matrix in the same slice.

## Invariant Linkage

This document maps flows, fixtures, and suite families. It does not by itself
prove that Roger's most critical product truths are owned. For that, use
[`VALIDATION_INVARIANT_MATRIX.md`](docs/VALIDATION_INVARIANT_MATRIX.md).

Practical rule:

- use this document to answer "which suite family and fixture family own this
  flow?"
- use the invariant matrix to answer "which critical truth is this work
  actually defending?"
- a bead that names flow coverage but no invariant ownership is still weaker
  than Roger should tolerate for release-critical behavior

## Lane Mapping And Compatibility

| Current suite family or subkind | Conceptual lane |
|---------------------------------|-----------------|
| `unit_*`, `prop_*` | `unit` |
| `int_*`, `accept_*` | `integration` |
| `e2e_*` | `e2e` |
| `smoke_*` | operator or release evidence, not a validation lane |

## Suite Families

| Suite family | Purpose | Primary owning beads |
|--------------|---------|----------------------|
| `unit_*` | pure domain, schema, reducer, serializer, and render logic | implementation beads plus `rr-011.3` |
| `prop_*` | state-machine and rule-matrix coverage | `rr-011.2`, `rr-011.4`, `rr-011.6` |
| `int_storage_*` | storage, migrations, artifact budgets, canonical-row invariants | `rr-014` |
| `int_harness_*` | adapter boundaries with doubles and resumability fixtures | `rr-003.1`, `rr-003.2`, `rr-011.5` |
| `int_cli_*` | launch resolution, session finder, robot outputs, resume routing | `rr-018`, `rr-011.6` |
| `unit_worker_*` | worker envelope, task binding, and task-result materialization logic | worker-boundary beads |
| `int_worker_*` | active-agent task binding, scope envelope, and result-submission boundaries | worker-boundary beads, later agent-access beads |
| `int_tui_*` | findings workflow, approval surfaces, inspector, editor handoff | `rr-019`, `rr-011.5` |
| `int_bridge_*` | Native Messaging envelopes, launch-only mode, install recovery, read-safe status | `rr-021`, `rr-011.4` |
| `int_github_*` | draft invalidation, payload rendering, partial post handling | `rr-020`, `rr-008.1`, `rr-011.4` |
| `int_search_*` | prior-review lookup, query-mode planning, recall envelopes, lexical-only degrade, provenance-safe search | `rr-024` |
| `accept_opencode_*` | OpenCode provider-claim acceptance | `rr-011.1`, `rr-011.5` |
| `accept_bounded_provider_*` | bounded live-CLI provider-claim acceptance (`codex`, `claude`, `gemini`; later `copilot`) | `rr-011.1` |
| `e2e_*` | heavyweight multi-boundary product journeys from the six-slot catalog (`E2E-01` through `E2E-06`) | `rr-011.7` plus later implementation beads |
| `smoke_*` | manual or release-lane smoke on real targets | `rr-011.4`, `rr-011.5`, `rr-011.6` |

## Fixture Families

Each fixture family must be small, named by purpose, and reusable across suites.

| Fixture family | Purpose | Primary consumers |
|----------------|---------|-------------------|
| `fixture_repo_compact_review` | compact single-repo happy-path review target | `int_cli_*`, `int_tui_*`, `e2e_core_review_happy_path` |
| `fixture_repo_monorepo_review` | larger repo with cross-file findings and search | `int_search_*`, `int_tui_*` |
| `fixture_same_pr_multi_instance` | two or more valid local targets for the same PR | `int_cli_*`, `int_bridge_*`, `smoke_same_pr_instances_*` |
| `fixture_findings_valid_minimal` | valid structured pack with primary and supporting code evidence | `unit_*`, `int_harness_*` |
| `fixture_findings_partial_mixed` | partially valid pack with salvageable findings | `unit_*`, `int_harness_*`, `rr-011.3` |
| `fixture_findings_raw_only` | no structured pack, raw output only | `int_harness_*`, `rr-011.3` |
| `fixture_findings_invalid_anchor` | structurally valid finding with stale or bad anchors | `prop_*`, `rr-011.2`, `rr-011.3` |
| `fixture_worker_task_binding` | valid agent task context with stable session/run/task/nonce tuple | `unit_worker_*`, `int_worker_*`, `accept_opencode_*` |
| `fixture_worker_nonce_mismatch` | stale or mismatched task/nonce binding that must fail closed | `unit_worker_*`, `int_worker_*` |
| `fixture_worker_scope_boundary` | active-agent read/query request that attempts to exceed allowed task scope | `int_worker_*`, `int_search_*` |
| `fixture_memory_recall_envelope` | retrieval corpus proving lane, scope, trust, and degraded explanation fields | `int_search_*`, `int_cli_*` |
| `fixture_memory_candidate_vs_promoted` | retrieval corpus with candidate and promoted memory for the same anchors | `int_search_*`, `int_cli_*`, `int_worker_*` |
| `fixture_memory_review_requests` | candidate memory plus promotion/demotion review requests and expected non-mutating resolutions | `int_search_*`, `int_worker_*`, `int_tui_*` |
| `fixture_memory_scope_denial` | overlay recall request that must fail closed without explicit enablement | `int_search_*`, `int_worker_*` |
| `fixture_search_auto_resolution` | search requests where omitted or `auto` intent must resolve to concrete planner intent before execution | `unit_*`, `int_search_*`, `int_cli_*`, `int_worker_*` |
| `fixture_search_recovery_scan_degraded` | missing/corrupt lexical sidecar forcing explicit `recovery_scan` mode with visible degradation | `int_search_*`, `int_cli_*` |
| `fixture_session_baseline_search_defaults` | session baseline snapshots proving default query mode, candidate visibility, and allowed-scope posture across dropout/return/reseed | `int_cli_*`, `int_worker_*`, `accept_opencode_*` |
| `fixture_resumebundle_stale_locator` | stale `SessionLocator` plus valid `ResumeBundle` reseed path | `accept_opencode_*`, `accept_bounded_provider_*`, `rr-011.5` |
| `fixture_opencode_dropout_return` | bare-harness dropout and `rr return` control flow | `accept_opencode_*`, `rr-011.5` |
| `fixture_bridge_launch_only_no_status` | bridge present, truthful launch-only/no-status mode | `int_bridge_*`, `rr-011.4` |
| `fixture_bridge_transcripts` | browser launch-intent and Native Messaging transcript corpus for supported browsers | `int_bridge_*`, `smoke_browser_launch_chrome`, `smoke_browser_launch_brave`, `smoke_browser_launch_edge` |
| `fixture_bridge_install_recovery` | missing host manifest, version drift, install repair path | `int_bridge_*`, `smoke_bridge_install_*` |
| `fixture_github_draft_batch` | local outbound drafts, approval snapshot, payload rendering | `int_github_*`, `e2e_core_review_happy_path` |
| `fixture_partial_post_recovery` | one posted action succeeds while another fails | `int_github_*`, `rr-011.4` |
| `fixture_refresh_rebase_target_drift` | rebased target with moved anchors and invalidation pressure | `prop_*`, `int_github_*`, `rr-011.2`, `rr-011.4` |

## Flow-To-Coverage Map

| Flow family | Required coverage shape | Primary owner |
|-------------|-------------------------|---------------|
| `F00` launch profile and terminal topology | `unit_*`, `prop_*`, `int_cli_*`, targeted `smoke_*` | `rr-018` |
| `F01`, `F01.1`, `F01.2` local entry, repo re-entry, global session finder | `int_cli_*`, `accept_opencode_*`, targeted `smoke_*` | `rr-011.6` |
| `F02`, `F02.1`, `F02.3` browser launch, setup, and reduced-friction re-entry | `int_bridge_*`, `smoke_browser_launch_chrome`, `smoke_browser_launch_brave`, `smoke_browser_launch_edge`, supported-browser `smoke_*`, and approved `E2E-05` when implemented | `rr-021` |
| `F03`, `F11` structured findings intake and degraded parse fallback | `unit_*`, `int_harness_*`, provider acceptance where needed | `rr-011.3` |
| `F04`, `F04.1`, `F05`, `F05.1` triage, editor handoff, follow-up, clarification | `int_tui_*`, `int_harness_*`, targeted `smoke_*` | `rr-011.5` |
| `F06`, `F13` refresh and draft invalidation | `prop_*`, `int_github_*`, `int_cli_*`, and approved `E2E-04` when implemented | `rr-011.2`, `rr-011.4` |
| `F07` draft review, approval, posting | `unit_*`, `int_github_*`, and `E2E-01` | `rr-008.1`, `rr-011.4` |
| `F08` history, original pack, and raw output inspection | `int_tui_*`, `int_storage_*` | `rr-019` |
| `F09` search and recall during review | `unit_*`, `int_search_*`, `int_cli_*`, targeted manual smoke, and approved memory-assisted E2Es (`E2E-02`, `E2E-03`) when implemented | `rr-024` |
| `F09.1` active agent memory access during review | `unit_worker_*`, `int_worker_*`, `int_cli_*`, targeted `accept_*`, and targeted manual smoke | later agent-access beads |
| `F09.2` candidate audit and memory review during review | `unit_*`, `int_search_*`, `int_worker_*`, `int_tui_*`, and targeted manual smoke | `rr-024`, later agent-access beads |
| `F10`, `F14` bridge recovery and honest no-status mode | `int_bridge_*`, `smoke_browser_launch_chrome`, `smoke_browser_launch_brave`, `smoke_browser_launch_edge`, supported-browser `smoke_*` | `rr-011.4` |
| `F12` same-PR multi-instance selection and routing | `prop_*`, `int_cli_*`, `int_bridge_*`, `smoke_*` | `rr-011.6` |
| `F17`, `F17.1` harness dropout and return | `accept_opencode_*`, targeted `smoke_*`, and approved `E2E-06` when implemented | `rr-011.5` |

## Support-Claim Ownership

| Claim | Minimum defending coverage |
|-------|----------------------------|
| OpenCode direct resume, stale-locator reseed, dropout, and `rr return` | `accept_opencode_*` plus release-lane `smoke_*` |
| Bounded live-CLI provider Tier A support | `accept_bounded_provider_*`; no deeper continuity claim without new acceptance |
| Native Messaging is the serious v1 bridge | `int_bridge_*` plus supported-browser `smoke_*` |
| Chrome/Brave/Edge browser launch support is explicit and bounded | `int_bridge_*`, `smoke_browser_launch_chrome`, `smoke_browser_launch_brave`, `smoke_browser_launch_edge`, `fixture_bridge_transcripts` |
| Launch-only bridge mode is truthful and does not fake status | `fixture_bridge_launch_only_no_status`, `int_bridge_*`, `rr-011.4` |
| Approval invalidation and partial post recovery are safe | `prop_*`, `int_github_*`, `fixture_partial_post_recovery`, `rr-011.4` |
| Same-PR multi-instance routing is explicit and safe | `fixture_same_pr_multi_instance`, `int_cli_*`, `int_bridge_*`, `rr-011.6` |
| Structured findings degraded modes are survivable and auditable | `fixture_findings_partial_mixed`, `fixture_findings_raw_only`, `fixture_findings_invalid_anchor`, `rr-011.3` |
| Memory and recall stay truthful across review continuation and triage | `int_search_*`, `int_cli_*`, targeted smoke, and any approved memory-assisted E2E catalog entries (`E2E-02`, `E2E-03`) | `rr-024` |
| Active-agent read/query and memory access remain scoped, provenance-rich, and truthfully degraded | `unit_worker_*`, `int_worker_*`, `int_cli_*`, targeted `accept_*`, `fixture_worker_scope_boundary`, `fixture_memory_recall_envelope` | later agent-access beads |
| Memory promotion/demotion review stays explicit and non-mutating until accepted | `int_search_*`, `int_worker_*`, `int_tui_*`, `fixture_memory_review_requests` | `rr-024`, later agent-access beads |

## Artifact Obligations

The suites above must preserve or consume these artifact classes where relevant:

- normalized `StructuredFindingsPack` snapshots
- raw provider outputs
- `ResumeBundle` examples
- bridge request or response transcripts
- outbound-draft and posted-action snapshots
- reducer or controller structural states for TUI and CLI flows
- fixture provenance metadata tying a fixture to its allowed suite families

Failure artifacts should be kept by default for acceptance, E2E, and bridge or
provider integration failures.

## `0.1.x` E2E Budget Rule

- The blessed heavyweight E2E catalog contains exactly six approved major
  journeys: `E2E-01` through `E2E-06`.
- Only the entries that have executable suites and real runs count as
  functional coverage.
- If that count increases, Roger should emit the explicit "could this be
  smaller, or are you taking the lazy route to another heavyweight E2E?"
  feedback required by the plan and `AGENTS.md`.
- New expensive scenarios should default to `unit_*`, `prop_*`, `int_*`,
  `accept_*`, or `smoke_*` unless a written justification proves they cannot.

## Acceptance Summary For `rr-025`

This matrix now names:

- the major flow families Roger must defend
- the fixture families that support them
- the suite families that own the work
- the support claims Roger is allowed to make
- the six-slot major-E2E rule and its lower-level substitutes

That is enough for the `rr-011.x` validation beads to proceed without inventing
their own validation scope.
