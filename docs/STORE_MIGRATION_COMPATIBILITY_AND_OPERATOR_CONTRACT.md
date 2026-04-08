# Store Migration Compatibility And Operator Contract

This document converts the migration lane plan in
[`PLAN_FOR_SCHEMA_MIGRATIONS_AND_UPDATE_COMPATIBILITY.md`](PLAN_FOR_SCHEMA_MIGRATIONS_AND_UPDATE_COMPATIBILITY.md)
into the implementation-facing contract for bead `rr-1xhg.1`.

It is intentionally narrow:

- define the compatibility envelope Roger publishes and embeds
- define migration classes and fail-closed boundaries
- define what `rr update --dry-run` must report before apply
- define what first-run store open may or may not do automatically
- confirm the follow-on bead split and ownership boundaries

It does not authorize shipping migration-capable updates by itself.

## Scope And Authority

For migration/update behavior disagreements, this contract has higher authority
than prose in historical critique docs. It narrows and clarifies:

- [`PLAN_FOR_ROGER_REVIEWER.md`](PLAN_FOR_ROGER_REVIEWER.md)
- [`RELEASE_AND_TEST_MATRIX.md`](RELEASE_AND_TEST_MATRIX.md)
- [`PLAN_FOR_SCHEMA_MIGRATIONS_AND_UPDATE_COMPATIBILITY.md`](PLAN_FOR_SCHEMA_MIGRATIONS_AND_UPDATE_COMPATIBILITY.md)

Current product truth remains unchanged until follow-on beads land:

- `0.1.x` still reports migration posture as deferred/fail-closed
- `rr update` still applies binary replacement only

## Compatibility Envelope

Every published release that participates in migration preflight must expose the
same compatibility envelope in two places:

1. embedded in the `rr` binary
2. exported in release/install metadata consumed by `rr update --dry-run`

Required fields:

- `store_schema_version`: schema expected after successful open/migration
- `min_supported_store_schema`: oldest schema this release can open at all
- `auto_migrate_from`: oldest schema this release may migrate automatically
- `migration_policy`: `binary_only` | `auto_safe` | `explicit_operator_gate` | `unsupported`
- `migration_class_max_auto`: `class_a` | `class_b` | `none`
- `sidecar_generation`: generation marker for derived assets
- `backup_required`: whether pre-mutation checkpoint is mandatory
- `envelope_version`: Roger-owned version for envelope format evolution

Envelope mismatch rule:

- if embedded envelope and published metadata envelope disagree for a target
  release, update and first-run migration behavior must fail closed

## Migration Classes

Roger migration classes are explicit and exhaustive for the first migration
lane.

### Class A: additive-safe

- additive schema change with no destructive reinterpretation
- may run automatically only when allowed by envelope and policy checks

### Class B: additive plus rebuild

- additive canonical DB migration plus sidecar generation invalidation/rebuild
- may run automatically only when allowed by envelope and policy checks

### Class C: semantic rewrite

- row meaning changes, merges/splits, destructive transforms, or weak recovery
- must not auto-run on first open
- requires explicit operator gate path

### Class D: unsupported

- migrations Roger cannot prove safe or cannot recover from in-line
- update/migration paths must fail closed with repair/export guidance

## Fail-Closed Boundaries

Roger must fail closed before mutation when any of the following is true:

- release metadata is missing, ambiguous, or checksum-invalid
- compatibility envelope is missing or malformed
- embedded and published envelopes disagree
- local store schema is below minimum supported schema
- migration class/policy requires explicit gate but user is on default path
- migration posture is unsupported for current release line

Fail-closed means:

- no binary apply for unsupported migration posture discovered in updater
- no store mutation on first open when policy/class checks fail
- explicit operator-facing guidance naming blocked reason and next steps

## `rr update --dry-run` Pre-Apply Contract

Before apply, `rr update --dry-run` must surface all migration posture details
needed for operator and robot decisions.

Minimum required migration payload fields:

- `migration.status`:
  - `no_migration_needed`
  - `auto_safe_migration_after_update`
  - `migration_requires_explicit_operator_gate`
  - `migration_unsupported`
- `migration.current_store_schema`
- `migration.target_store_schema`
- `migration.min_supported_store_schema`
- `migration.auto_migrate_from`
- `migration.policy`
- `migration.classification` (`class_a` | `class_b` | `class_c` | `class_d` | `none`)
- `migration.backup_required`
- `migration.apply_allowed` (boolean)
- `migration.blocked_reason` (required when `apply_allowed=false`)

Rules:

- updater apply is blocked when `migration.apply_allowed=false`
- `--yes` bypasses confirmation only; it does not bypass migration blocks
- in `0.1.x`, posture remains binary-only/deferred until `rr-1xhg.2+` land

## First-Run Store-Open Contract

First-run with a newly updated binary evaluates embedded envelope against local
store schema before any store mutation.

Allowed automatic behavior:

- no-op open when no migration is needed
- Class A/B migration only when:
  - envelope policy permits `auto_safe`
  - schema is within auto-migrate window
  - required checkpoint/journal hooks are available

Disallowed automatic behavior:

- Class C/D migration on default first-open path
- any migration when envelope integrity checks fail
- any migration during binary replacement step itself

When auto behavior is disallowed:

- Roger fails closed
- Roger records/prints explicit recovery guidance
- Roger does not silently downgrade behavior to partial mutation

## Follow-On Bead Alignment

This contract confirms the existing `rr-1xhg` child split; no dependency or
scope changes are required at this stage.

- `rr-1xhg.2`: implement envelope publication + updater preflight/reporting and
  blocked apply boundaries
- `rr-1xhg.3`: implement migration checkpoints + migration journal contract
- `rr-1xhg.4`: implement Class A/B runner with sidecar generation invalidation
- `rr-1xhg.5`: implement prior-schema, interruption, and unsupported-path
  validation and release gates

If implementation finds missing fields or states, extend this contract first,
then update child bead acceptance text before closing affected beads.
