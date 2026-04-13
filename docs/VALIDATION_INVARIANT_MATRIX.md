# Validation Invariant Matrix

This document is Roger's registry of release-critical product truths.

Use it when:

- shaping or splitting implementation beads
- deciding whether a suite is defending a real claim or just exercising code
- deciding whether a support claim may be widened
- checking whether proof is machine-derivable from current artifacts

This document complements, but does not replace:

- [`../TESTING.md`](../TESTING.md)
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
| `INV-BRIDGE-001` | Browser or Native Messaging launch never reports fake Roger success | `integration` | `int_bridge_*` | `fixture_bridge_launch_only_no_status`, `fixture_bridge_transcripts` | bridge envelope transcript, host result snapshot, truthful no-status evidence | `rr-021`, `rr-011.4` |
| `INV-BRIDGE-002` | Missing host manifest, version drift, or install damage fails closed and presents a bounded repair path | `integration` + `smoke` | `int_bridge_*`, `smoke_bridge_install_*` | `fixture_bridge_install_recovery` | install diagnosis summary, manifest-state snapshot, repair guidance artifact | `rr-021`, `rr-011.4` |
| `INV-SEARCH-001` | Search and recall never widen scope silently beyond the declared repo, project, or broader overlay | `unit` + `integration` | `unit_*`, `int_search_*` | search corpora and scope fixtures | search result summary with scope bucket, provenance labels, query context | `rr-024` |
| `INV-SEARCH-002` | Degraded lexical-only search remains truthful about mode, provenance, and limits | `unit` + `integration` | `unit_*`, `int_search_*` | lexical-only degrade fixtures | retrieval-mode evidence, result provenance snapshot, degraded-mode marker | `rr-024` |
| `INV-TUI-001` | The TUI keeps selection and focus stable across ordinary refreshes, long queues, and recoverable failures when the target remains valid | `unit` + `integration` | `prop_*`, `int_tui_*` | repo review fixtures, queue-state fixtures | reducer snapshots, controller state summaries, preserved-selection evidence | `rr-019`, `rr-011.5` |
| `INV-TUI-002` | The TUI surfaces repair-needed, invalidated, stale, and posting-failed states as bounded operator states with visible next actions | `integration` | `int_tui_*`, `int_github_*`, `int_harness_*` | invalidation, partial post, and findings-repair fixtures | controller state summary, next-action surface snapshot, failure artifact bundle | `rr-019`, `rr-011.4`, `rr-011.3` |
| `INV-ROBOT-001` | `rr --robot` envelopes and exit behavior remain deterministic and machine-readable across supported commands | `unit` + `integration` | `unit_*`, `int_cli_*` | robot-output corpora | output envelope snapshot, exit-code summary, schema validation record | `rr-018`, robot CLI beads |
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
