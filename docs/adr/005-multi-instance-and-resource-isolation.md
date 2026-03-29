# ADR 005: Multi-Instance and Resource Isolation

- Status: accepted
- Date: 2026-03-29

## Context

Roger must support at least two concurrent reviewers without hidden assumptions
about worktrees, env files, ports, caches, or local state. Earlier planning had
too much ambiguity around copied state and DB synchronization.

The current plan already points in the right direction: one canonical Roger
store per profile, with worktrees opt-in rather than default.

## Decision

Recommended policy:

- Roger uses one canonical local store per user profile by default
- single-repo mode is a first-class default path and should work with the
  user's existing checkout without requiring worktrees or named instances
- normal read-mostly review flows use the current checkout plus a recorded repo
  snapshot
- dedicated worktrees are created only for isolated execution, code changes, or
  conflicting local repo state
- named instances isolate repo-local mutable resources rather than Roger DB
  state by default

Resource classes to make explicit:

- copied env/config files
- local ports
- repo-local dev DBs
- docker-compose project names, container names, and similar local orchestrator
  identifiers
- cache directories
- artifact/log directories

Recommended product stance:

- Roger should expose a small set of isolation primitives rather than trying to
  guess every local-dev topology
- power users should be able to configure how a named instance treats each
  mutable resource class as shared, isolated, copied, renamed, offset, or
  disabled
- creating a new Roger profile should be reserved for cases where the canonical
  Roger store itself must be isolated; ordinary concurrent review should stay in
  one profile with named instances

Recommended primitive families:

- file-copy rules for `.env`, `.env.local`, repo-local config, and similar files
- port strategies such as fixed override, offset-from-base, or explicit map
- local DB strategies such as shared, copied snapshot, or fresh empty target
- docker/container naming strategies such as compose-project prefix/suffix
  overrides
- cache, artifact, and log directory strategies such as shared or per-instance

## Consequences

- the default model avoids DB-copy synchronization complexity
- single-repo mode stays simple for ordinary users
- worktree creation becomes a visible, elevated choice rather than background
  magic
- preflight diagnostics become part of the product, not a nice-to-have

## Open Questions

- which file-copy and naming rules should Roger ship as built-in defaults?
- how opinionated should the preflight UX be when it detects likely conflicts?
- which resource classes need first-party primitives in `0.1.0`, and which can
  remain manual overrides at first?

## Follow-up

- define the instance-preflight checklist
- define the per-resource shared vs isolated matrix
- add unit tests for instance setup resolution, file-copy rules, port and naming
  rewrites, and preflight conflict classification
- write acceptance tests for the two-reviewer case
