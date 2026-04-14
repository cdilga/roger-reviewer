# Validation Invariant Matrix

This document is Roger's registry of release-critical product truths.

Use it when:

- shaping or splitting implementation beads
- deciding whether a suite is defending a real claim or just exercising code
- deciding whether a support claim may be widened
- checking whether proof is machine-derivable from current artifacts

This document complements, but does not replace:

- [`TESTING.md`](TESTING.md)
- [`TEST_HARNESS_GUIDELINES.md`](TEST_HARNESS_GUIDELINES.md)
- [`VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md`](VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md)
- [`VALIDATION_HARNESS_SCAFFOLD_CONTRACT.md`](VALIDATION_HARNESS_SCAFFOLD_CONTRACT.md)
- [`RELEASE_AND_TEST_MATRIX.md`](RELEASE_AND_TEST_MATRIX.md)

## Governing Rules

- every release-critical support claim should map to one or more invariant ids
- every new implementation bead that changes behavior should cite one or more
  invariant ids or add a new row here
- every invariant should have at least one owning suite family and one owning
  bead
- if an invariant is release-critical, its evidence should be mechanically
  discoverable from suite metadata and artifact outputs rather than only from
  prose closeout notes
- invariants may be defended by `unit`, `integration`, or `e2e`, but the
  cheapest truthful lane wins unless the claim truly spans several real
  boundaries

## Matrix

| Invariant id | Product truth | Primary lane | Minimum defending suites | Fixtures or corpora | Required proof artifacts | Owning beads |
|--------------|---------------|--------------|---------------------------|---------------------|--------------------------|--------------|
| `INV-POST-001` | Approval binds to the exact rendered draft payload and target tuple | `unit` + `integration` | `unit_*`, `int_github_*` | `fixture_github_draft_batch` | reducer snapshots, payload hash evidence, adapter request snapshot | `rr-008.1`, `rr-020`, later posting beads |
| `INV-POST-002` | Target drift, rebase, rerun, or anchor drift invalidates prior approval automatically | `unit` + `integration` | `prop_*`, `int_github_*` | `fixture_refresh_rebase_target_drift`, `fixture_github_draft_batch` | invalidation summary, refreshed draft snapshot, approval-state transition evidence | `rr-011.2`, `rr-011.4`, `rr-020` |
| `INV-POST-003` | Posting from the wrong repo, wrong PR, or stale session binding fails closed and preserves a local repair path | `integration` | `int_github_*` | `fixture_partial_post_recovery`, target-drift fixtures | adapter response snapshot, posted-action or failure snapshot, retry-path evidence | `rr-020`, `rr-011.4` |
| `INV-SESSION-001` | Session resolution never guesses across ambiguous same-PR or same-repo candidates | `unit` + `integration` | `prop_*`, `int_cli_*`, `int_bridge_*` | `fixture_same_pr_multi_instance` | routing decision snapshot, candidate set, explicit ambiguity result | `rr-011.6`, `rr-018`, `rr-021` |
| `INV-SESSION-002` | `resume`, `refresh`, and `return` operate only on valid resolvable session identity or fail closed truthfully | `integration` | `int_cli_*`, `accept_opencode_*`, `accept_bounded_provider_*` | `fixture_resumebundle_stale_locator`, `fixture_opencode_dropout_return` | resume bundle snapshot, session locator evidence, continuity result summary | `rr-015`, `rr-011.5`, `rr-018` |
| `INV-HARNESS-001` | Partially valid findings packs salvage valid findings rather than collapsing to total failure | `unit` + `integration` | `unit_*`, `int_harness_*` | `fixture_findings_partial_mixed` | normalized findings snapshot, repair classification, salvage summary | `rr-011.3`, harness beads |
| `INV-HARNESS-002` | Raw-only, malformed, or repair-needed findings outcomes remain auditable and truthfully classified | `unit` + `integration` | `unit_*`, `int_harness_*`, provider acceptance when needed | `fixture_findings_raw_only`, malformed corpora | raw output artifact, normalized outcome summary, repair-needed marker | `rr-011.3`, prompt/harness beads |
| `INV-HARNESS-003` | Stale or invalid code anchors do not destroy surviving finding context | `unit` + `integration` | `prop_*`, `int_harness_*`, `int_tui_*` | `fixture_findings_invalid_anchor`, `fixture_refresh_rebase_target_drift` | anchor-status summary, surviving finding snapshot, inspector-state evidence | `rr-011.2`, `rr-011.3`, `rr-019` |
| `INV-AGENT-001` | `rr agent` calls and worker result submission only succeed for the currently bound session/run/task/nonce tuple and fail closed on stale or mismatched bindings | `unit` + `integration` | `unit_worker_*`, `int_worker_*` | `fixture_worker_task_binding`, `fixture_worker_nonce_mismatch` | worker envelope snapshot, binding decision summary, explicit denial evidence | worker-boundary beads |
| `INV-AGENT-002` | Worker memory, finding, and artifact reads never widen scope or policy beyond the allowed session/task envelope | `integration` | `int_worker_*`, `int_search_*` | `fixture_worker_scope_boundary`, `fixture_memory_scope_denial` | scope decision summary, provenance-tagged retrieval snapshot, denial artifact | worker-boundary beads |
| `INV-AGENT-003` | Multi-turn worker tasks preserve exact prompt-turn history and materialize only the validated terminal result into canonical findings or clarification lineage | `unit` + `integration` | `unit_worker_*`, `int_worker_*`, `int_harness_*` | `fixture_worker_multi_turn_program`, `fixture_findings_partial_mixed` | prompt-turn ledger snapshot, worker-stage-result artifact, materialization summary | worker-boundary beads, prompt-behavior beads |
| `INV-BRIDGE-001` | Browser or Native Messaging launch never reports fake Roger success | `integration` | `int_bridge_*` | `fixture_bridge_launch_only_no_status`, `fixture_bridge_transcripts` | bridge envelope transcript, host result snapshot, truthful no-status evidence | `rr-021`, `rr-011.4` |
| `INV-BRIDGE-002` | Missing host manifest, version drift, or install damage fails closed and presents a bounded repair path | `integration` + `smoke` | `int_bridge_*`, `smoke_bridge_install_*` | `fixture_bridge_install_recovery` | install diagnosis summary, manifest-state snapshot, repair guidance artifact | `rr-021`, `rr-011.4` |
| `INV-SEARCH-001` | Search and recall never widen scope silently beyond the declared repo, project, or broader overlay | `unit` + `integration` | `unit_*`, `int_search_*` | search corpora and scope fixtures | search result summary with scope bucket, provenance labels, query context | `rr-024` |
| `INV-SEARCH-002` | Degraded lexical-only search remains truthful about mode, provenance, and limits | `unit` + `integration` | `unit_*`, `int_search_*` | lexical-only degrade fixtures | retrieval-mode evidence, result provenance snapshot, degraded-mode marker | `rr-024` |
| `INV-SEARCH-003` | Recall results expose memory lane, scope bucket, trust/degraded truth, and bounded explanation rather than an opaque ranked blob | `unit` + `integration` | `unit_*`, `int_search_*`, `int_cli_*` | `fixture_memory_recall_envelope`, `fixture_memory_candidate_vs_promoted` | recall-envelope snapshot, explain summary, robot search envelope, degraded-flag evidence | `rr-024`, later agent-access beads |
| `INV-SEARCH-004` | Candidate memory never silently behaves like promoted memory, and overlay-only memory never surfaces without explicit enablement | `integration` | `int_search_*`, `int_cli_*`, `int_worker_*` | `fixture_memory_candidate_vs_promoted`, `fixture_memory_scope_denial` | candidate-versus-promoted result snapshot, overlay-denial evidence, provenance-tagged retrieval summary | `rr-024`, later agent-access beads |
| `INV-SEARCH-005` | Memory review requests are explicit, auditable, and non-mutating until Roger accepts a resolution | `unit` + `integration` | `unit_*`, `int_search_*`, `int_worker_*`, `int_tui_*` | `fixture_memory_review_requests`, `fixture_memory_candidate_vs_promoted` | memory-review request ledger snapshot, resolution summary, non-mutation proof before acceptance | `rr-024`, later agent-access beads |
| `INV-SEARCH-006` | `query_mode=auto` never survives as an opaque executed planner state; Roger resolves it to a concrete intent before retrieval runs and surfaces both requested and resolved intent truthfully | `unit` + `integration` | `unit_*`, `int_search_*`, `int_cli_*`, `int_worker_*` | `fixture_search_auto_resolution`, `fixture_memory_recall_envelope` | search-plan snapshot, recall-envelope intent fields, robot search envelope, resolved-intent summary | `rr-024`, later agent-access beads |
| `INV-SEARCH-007` | `recovery_scan` is recovery-only, explicitly degraded, and never masquerades as healthy planned retrieval | `integration` | `int_search_*`, `int_cli_*` | `fixture_search_recovery_scan_degraded`, `fixture_memory_scope_denial` | retrieval-mode snapshot, degraded reason summary, blocked healthy-path claim evidence | `rr-024`, later search hardening beads |
| `INV-CONTEXT-001` | Session baseline and prompt baseline, including default search posture and candidate-visibility policy, remain resolvable and stable across dropout, return, reseed, and active-agent read/query operations | `unit` + `integration` | `unit_*`, `int_cli_*`, `int_worker_*`, `accept_opencode_*` | `fixture_resumebundle_stale_locator`, `fixture_opencode_dropout_return`, `fixture_worker_task_binding`, `fixture_session_baseline_search_defaults` | baseline snapshot, continuity result summary, post-return context evidence | `rr-015`, `rr-018`, later agent-access beads |
| `INV-TUI-001` | The TUI keeps selection and focus stable across ordinary refreshes, long queues, and recoverable failures when the target remains valid | `unit` + `integration` | `prop_*`, `int_tui_*` | repo review fixtures, queue-state fixtures | reducer snapshots, controller state summaries, preserved-selection evidence | `rr-019`, `rr-011.5` |
| `INV-TUI-002` | The TUI surfaces repair-needed, invalidated, stale, and posting-failed states as bounded operator states with visible next actions | `integration` | `int_tui_*`, `int_github_*`, `int_harness_*` | invalidation, partial post, and findings-repair fixtures | controller state summary, next-action surface snapshot, failure artifact bundle | `rr-019`, `rr-011.4`, `rr-011.3` |
| `INV-ROBOT-001` | `rr --robot` envelopes and exit behavior remain deterministic and machine-readable across supported operator-facing commands, without absorbing the distinct `rr agent` transport | `unit` + `integration` | `unit_*`, `int_cli_*` | robot-output corpora | output envelope snapshot, exit-code summary, schema validation record | `rr-018`, robot CLI beads |
| `INV-UPDATE-001` | Published release assets remain self-consistent across install metadata, core manifest, checksum manifest, release notes, and release-hosted installer entrypoints for every shipped tag | `integration` + `smoke` | `int_release_*`, `smoke_release_install_*` | `fixture_release_bundle_*`, live latest-release probes | verified asset manifest, install-metadata snapshot, publish plan, live latest installer dry-run evidence | `rr-5urd.1`, release/update validation beads |
| `INV-UPDATE-002` | In-place update resolves provenance, channel, version, target, and install layout truthfully, and fails closed on unsupported histories or layouts instead of guessing | `unit` + `integration` | `unit_update_*`, `int_cli_update_*` | unpublished-build fixtures, symlink/layout fixtures, target-matrix fixtures, channel-history fixtures | robot update envelope, target-resolution summary, blocked-reason snapshot, confirmation matrix evidence | `rr-5urd.1`, future update hardening beads |
| `INV-UPDATE-003` | Installer and updater consume the same published checksum and manifest contract for the claimed shell/OS surfaces, or the support claim is explicitly narrowed by surface | `integration` + `smoke` | `int_release_*`, `int_cli_update_*`, `smoke_release_install_*` | synthetic release bundles, cross-shell installer fixtures | installer dry-run summary, updater dry-run envelope, published checksum manifest snapshot, narrowed-claim record when parity is absent | `rr-5urd.1`, release/update validation beads |
| `INV-UPDATE-004` | A representative published-to-published upgrade rehearsal proves install `N`, run old `rr`, update to `N+1`, and retain a usable binary with truthful post-update state | `integration` + `smoke` | `int_update_upgrade_*`, `smoke_update_upgrade_*` | synthetic prior-release bundles, isolated install dirs, prior-schema fixtures when required | upgrade rehearsal manifest, pre/post version summary, post-update smoke output, retained failure artifacts on interruption | `rr-5urd.1`, `rr-1xhg.5`, future release/update validation beads |
| `INV-UPDATE-005` | Blocked update paths emit repair guidance that is usable from the actual operator context rather than assuming a repo checkout or hidden tooling | `integration` | `int_cli_update_*`, `smoke_release_install_*` | blocked-update fixtures, installed-binary recovery fixtures | blocked robot envelope, repair-action snapshot, release-backed reinstall command evidence | `rr-5urd.1`, future update UX hardening beads |
| `INV-STORE-001` | Store migration, schema update, and artifact lookup either complete safely or fail closed without silent store corruption | `integration` | `int_storage_*` | migration fixtures, artifact-store integrity fixtures | migration summary, integrity-check output, artifact lookup evidence | `rr-014`, migration beads |

## Translation To Beads

Every implementation bead that changes behavior should include:

- `invariant_ids`: one or more ids from the matrix above
- `promise`: the concrete user-visible or operator-visible promise
- `failure_scope`: degraded, invalidation, or recovery cases included in the bead
- `suite_families`: the minimum defending suites
- `fixture_families`: the deterministic fixtures or corpora required
- `proof_outputs`: the summary artifact, failure artifact, or proof manifest
  expected at closeout
- `execution_policy`: `local-bead`, CI reproduction, operator stability, or
  `release-candidate`

If a bead cannot name those fields honestly, it is underspecified.

Coverage-gap escalation rule:

- if an agent notices that an invariant has weak ownership, weak degraded-mode
  coverage, or evidence that is not mechanically discoverable, it should create
  or split a testing bead rather than silently accepting the gap
- if faithful test implementation depends on unresolved UX or support-claim
  intent, the agent should create a small research or clarification bead and
  reread the canonical plan before implementing the tests

## Proof-Derivation Rule

A reviewer should be able to derive proof mechanically in this order:

1. start from a support claim or release gate
2. map that claim to one or more invariant ids
3. map invariant ids to suite families and fixtures
4. run the named entrypoint or command
5. inspect the suite summary or failure artifacts
6. resolve the latest attempted or latest passing proof manifest when the suite
   family publishes one

If that chain breaks, the claim is not yet fully proof-derivable.

## Notes

- This matrix names release-critical truths first. It is intentionally smaller
  than the full suite inventory.
- Not every suite family needs a new invariant row; many suites defend local
  shaping logic under a broader invariant.
- A future implementation may add machine-readable invariant ids directly to the
  suite metadata envelope. That would strengthen the proof chain and is aligned
  with this document.
