# Plan For Schema Migrations And Update Compatibility

## Purpose

Roger already has two partially true but currently disconnected facts:

1. the canonical store has a real schema version and a real migration runner in
   `packages/storage`
2. `rr update` can already replace the installed binary in place for published
   releases

What is still missing is the upgrade contract that safely connects those two
facts. Today Roger explicitly defers migration-capable updates in `0.1.x`, which
is correct for safety, but it leaves a major product gap: once a release needs a
real local-state or schema migration, Roger has no honest path beyond "fail
closed and tell the user to reinstall."

This document defines the next planning lane: introduce schema migrations in a
way that preserves Roger's local-first safety model instead of undermining it.

This document does not replace the canonical product plan in
[`PLAN_FOR_ROGER_REVIEWER.md`](PLAN_FOR_ROGER_REVIEWER.md). It narrows the
upgrade/migration lane and turns the current deferred posture into a staged,
implementable roadmap.

The implementation-facing contract for this lane is
[`STORE_MIGRATION_COMPATIBILITY_AND_OPERATOR_CONTRACT.md`](STORE_MIGRATION_COMPATIBILITY_AND_OPERATOR_CONTRACT.md).
If this plan and implementation behavior diverge, update that contract first
and then sync this lane plan.

---

## Current Truth

### What exists today

- `RogerStore::open(...)` already runs a forward migration chain against the
  canonical SQLite database using `PRAGMA user_version` plus the
  `schema_migrations` ledger.
- `rr update` already supports published-release in-place binary replacement
  with:
  - metadata validation
  - target resolution
  - checksum verification
  - confirmation-by-default
  - rollback-on-replacement-failure for the binary itself

### What is explicitly deferred today

- migration-capable updates are still reported as deferred/fail-closed in
  `0.1.x`
- release metadata does not yet advertise store-schema compatibility or
  migration policy
- the updater does not preflight the current local store against the target
  release's schema expectations
- there is no Roger-owned backup/export checkpoint specifically designed for
  schema-changing upgrades
- there is no operator-visible migration journal or recovery workflow if a store
  migration is interrupted

### Why this is dangerous

If Roger begins shipping releases that require more than additive-safe internal
schema evolution, the current model becomes ambiguous:

- the binary may update successfully
- the first later store open may mutate the DB without a proper backup contract
- the release metadata cannot tell the updater whether the new binary is safe
  for the existing local store
- the operator has no structured "this upgrade is safe / risky / unsupported"
  signal before apply

That is precisely the kind of hidden mutation path Roger is supposed to avoid.

---

## Goal

Introduce schema migrations so that Roger can support future state/schema
evolution without lying about safety, rollback, or recoverability.

The end state should be:

- `rr update --dry-run` can tell the operator whether the target release is:
  - binary-only
  - auto-migratable
  - migration-gated
  - unsupported/fail-closed
- the updater still replaces only the binary during apply
- store migration happens under an explicit Roger-owned policy on first run or
  via a dedicated migration step
- the canonical DB is backed up before any migration class that is not
  trivially reversible
- sidecars are invalidated/rebuilt instead of mutated unsafely in place
- interrupted or failed migrations leave Roger in a truthful recovery state

---

## Non-Goals

- downgrade support across store schema versions
- arbitrary cross-profile import/export as part of the first migration slice
- transcript-isomorphic migration across harnesses
- migration during the binary replacement step itself
- hiding destructive or semantically risky upgrades behind one-shot automation

---

## Design Principles

### 1. Separate binary apply from data migration

`rr update` should remain the binary updater. It may preflight migration
requirements, but it should not silently mutate the canonical DB during archive
replacement. Binary replacement and store migration are different failure
domains and should stay visibly separate.

### 2. Forward-only, fail-closed

Roger should support forward migration only. If a target release requires a
migration outside the supported window, Roger must fail closed with backup/export
guidance instead of improvising.

### 3. Migrate the canonical DB first; rebuild everything else

The canonical DB is the source of truth. Sidecars, search indices, and other
derived assets should carry generation/schema metadata and be invalidated or
rebuilt when necessary rather than force unsafe in-place transformation.

### 4. Backups are part of the migration contract, not an operator afterthought

Any migration class beyond trivial additive evolution needs a Roger-owned,
discoverable checkpoint before mutation.

### 5. Releases must declare upgrade compatibility

No future release should rely on implied schema compatibility. The release
metadata bundle must say what store schemas it can open, auto-migrate, or reject.

---

## Proposed Compatibility Model

Each published Roger release should embed and publish a store compatibility
envelope with fields along these lines:

- `store_schema_version`: the schema expected after successful open/migration
- `min_supported_store_schema`: oldest schema this release can open at all
- `auto_migrate_from`: oldest schema this release may migrate automatically
- `migration_policy`:
  - `binary_only`
  - `auto_safe`
  - `explicit_operator_gate`
  - `unsupported`
- `sidecar_generation`: generation marker for rebuildable derived assets
- `backup_required`: boolean

This envelope should exist in two places:

1. embedded in the `rr` binary for first-run truth
2. exported into release/install metadata so `rr update --dry-run` can evaluate
   the target release before apply

This is the missing bridge between "what version am I upgrading to?" and "is my
local store safe to open afterward?"

---

## Migration Classes

Roger should not treat all schema bumps the same. Use explicit classes.

### Class A: additive-safe

Examples:
- add nullable columns
- add new tables
- add new metadata rows that do not reinterpret existing state

Expected behavior:
- may run automatically
- DB transaction required
- no heavy backup beyond the standard migration checkpoint if the contract says
  additive-safe is reversible enough

### Class B: additive + rebuild

Examples:
- changes that require reindexing sidecars
- changes that invalidate semantic/search generations

Expected behavior:
- canonical DB migration may run automatically
- affected sidecars are invalidated and rebuilt later
- backup checkpoint required before apply if DB interpretation changes

### Class C: semantic rewrite / risky local-state mutation

Examples:
- changing meaning of finding state rows
- splitting or merging canonical entities
- destructive cleanup or irreversible row transforms

Expected behavior:
- do not auto-run silently
- require explicit operator gate
- require backup checkpoint + migration journal
- if the recovery story is weak, defer and fail closed

### Class D: unsupported in current line

Examples:
- transformations Roger cannot yet prove safe
- migrations that need external dependencies or cross-store reconstruction

Expected behavior:
- release must declare unsupported
- updater preflight blocks
- Roger provides backup/export + reinstall guidance only

---

## Proposed Upgrade Flow

### Phase 1: preflight before apply

`rr update --dry-run` should:

1. resolve the target release metadata
2. inspect the current store schema version if a local store exists
3. compare that schema against the target release compatibility envelope
4. report one of:
   - `no_migration_needed`
   - `auto_safe_migration_after_update`
   - `migration_requires_explicit_operator_gate`
   - `migration_unsupported`

This output should be machine-readable and available in normal CLI messaging.

### Phase 2: binary apply

If the target is otherwise allowed:

- update continues to replace only the binary
- confirmation prompt should mention migration class when relevant
- unsupported migration states block before apply

### Phase 3: first run after update

On first open of the canonical store with the new binary:

- Roger checks the embedded compatibility envelope against the local store
- if no migration is needed, continue normally
- if Class A/B migration is allowed:
  - create migration checkpoint
  - run DB migration transactionally
  - mark affected sidecars invalid
  - record migration journal entry
- if Class C/D migration is not allowed automatically:
  - fail closed
  - present explicit backup/export + operator guidance

This keeps the binary updater simple while making migration state visible and
recoverable.

---

## Backup, Export, And Recovery Contract

Before any non-trivial schema migration Roger should create a migration
checkpoint under the profile/store root, for example:

`backups/<timestamp>/pre-migration-schema-v<old>-to-v<new>/`

Minimum contents:

- copy of the canonical SQLite DB before migration
- manifest file with:
  - old/new schema versions
  - Roger release version
  - migration class
  - backup creation time
  - sidecar generations present
  - recovery guidance

Initial scope should not require copying all artifacts and sidecars if they are
content-addressed and rebuildable, but the manifest must state exactly what is
and is not protected by the checkpoint.

Recovery rules:

- if migration fails before commit, reopen against the original DB
- if migration fails after backup but before final success marker, Roger should
  leave the backup intact and report recovery steps clearly
- no automatic rollback after a committed schema migration unless that downgrade
  path is explicitly designed and validated

---

## Migration Journal

Add a Roger-owned migration journal distinct from the simple
`schema_migrations` ledger.

`schema_migrations` answers "which SQL migrations were applied?"

The migration journal should answer:

- what release attempted the migration
- what schema versions were involved
- what migration class was used
- where the backup checkpoint lives
- whether the attempt is:
  - `started`
  - `committed`
  - `failed_pre_commit`
  - `needs_operator_recovery`

This is what makes crash recovery and operator support honest.

---

## Sidecars And Derived Assets

Do not broaden the migration problem by mutating every derived asset in place.

For the first migration-capable slice:

- canonical DB rows migrate first
- semantic/search/index sidecars are generation-tagged
- a schema/generation mismatch invalidates them
- rebuild happens lazily or through an explicit rebuild path

This keeps migrations scoped to the source of truth.

---

## Validation And Release Gates

No release should claim migration support without:

1. fixture coverage from at least one prior schema version
2. interrupted-migration recovery rehearsal
3. updater preflight coverage for:
   - safe auto migration
   - explicit gate required
   - unsupported/fail-closed
4. sidecar invalidation/rebuild verification
5. exact closeout evidence that the release metadata envelope matches the binary
   compatibility envelope

Use the existing
[`fixture_migration_and_artifact_integrity`](VALIDATION_FIXTURE_CORPUS_AND_MANIFEST.md)
family as the seed, but broaden it to include:

- old-store fixtures per supported schema floor
- backup checkpoint assertions
- interrupted migration journal assertions
- unsupported-migration fail-closed scenarios

---

## Recommended Rollout

### Slice 1: truth surfaces only

- add compatibility envelope to binary + release metadata
- surface migration classification in `rr update --dry-run`
- block unsupported migration states before binary apply

### Slice 2: backup and migration journal

- create migration checkpoints
- add explicit migration journal
- keep migration-capable updates still mostly blocked except for trivial classes

### Slice 3: Class A/B auto-safe migrations

- allow additive-safe canonical DB migration on first run
- invalidate/rebuild sidecars
- prove crash/interruption behavior

### Slice 4: explicit-gated higher-risk migrations

- only after the operator-facing recovery and backup story feels boring and
  reliable
- may introduce a dedicated `rr migrate` surface if the interaction complexity
  no longer fits cleanly inside first-run flow

---

## Bead Impact

This plan should feed a dedicated migration lane rather than being squeezed into
the already-closed `rr-5urd` update lane.

Minimum follow-on beads:

1. define compatibility envelope and release-metadata contract
2. surface migration preflight in `rr update --dry-run` and blocked apply paths
3. add migration checkpoint + journal
4. implement Class A/B migration runner with sidecar invalidation
5. add release/fixture validation for prior-schema upgrades and interruption
   recovery

---

## Bottom Line

The right way to introduce schema migrations is not "let `rr update` start
modifying the DB." The right way is:

- declare compatibility in release metadata and in the binary
- preflight migration posture before apply
- keep binary update and DB migration as separate, truthful steps
- checkpoint before risky local-state mutation
- make sidecars rebuildable instead of magical
- only auto-run migrations Roger can actually recover from

That gives Roger a path from today's binary-only updater to a genuinely safe
local-first upgrade story.
