# Round 04 Worktree Setup Research

Status: research note for Round 04 reconciliation. This is not the canonical
spec. It exists to narrow the unresolved multi-instance/worktree questions in
the canonical plan and ADR 005.

Date: 2026-03-29

## Research Question

Roger already has the right high-level stance:

- single-checkout mode is the default
- worktrees are opt-in and elevated
- one canonical Roger store per profile is the default

The remaining question is how Roger should let users define repeatable,
application-specific worktree setup behavior for things like tests, databases,
ports, caches, local env files, and container naming without turning Roger into
an opaque pile of app-specific heuristics.

This note studies recent Claude Code worktree and hook behavior as prior art,
then translates the useful parts into a Roger-specific recommendation.

## Primary External Sources

- Claude Code common workflows:
  <https://code.claude.com/docs/en/tutorials>
- Claude Code hooks reference:
  <https://code.claude.com/docs/en/hooks>
- Claude Code settings reference:
  <https://code.claude.com/docs/en/settings>
- Claude Code slash commands / skills reference:
  <https://code.claude.com/docs/en/slash-commands>

## What Claude Code Actually Provides

### 1. Built-in git worktree support, but environment setup is still project-specific

Claude supports `claude --worktree <name>` and by default creates git
worktrees under `.claude/worktrees/<name>`, with cleanup behavior tied to
whether there are remaining changes or commits.

Important limitation:

- the docs still explicitly tell the user to initialize the development
  environment in each new worktree according to the project's setup
- in other words, Claude creates the isolated checkout, but it does not pretend
  that dependency install, DB provisioning, or app-specific runtime setup can
  be inferred safely

This is the right philosophical stance for Roger as well.

### 2. Worktree lifecycle hooks are the key extension seam

Claude now exposes `WorktreeCreate` and `WorktreeRemove` hooks.

What matters:

- `WorktreeCreate` receives a worktree name and must return the absolute path
  to the created worktree
- custom hooks can replace the default creation logic when needed
- `WorktreeRemove` handles cleanup for custom worktree flows

This is strong prior art for Roger:

- the lifecycle should be hookable
- the hook should own app-specific setup only where built-in primitives are not
  enough
- cleanup should be first-class rather than a human afterthought

### 3. Session-start environment injection is separate from worktree creation

Claude's `SessionStart` hook can write exports to `CLAUDE_ENV_FILE`, making
environment variables available to future shell commands in that session.

This is important because it separates two concerns:

- worktree creation: where the isolated checkout lives
- session environment realization: which env vars, PATH changes, toolchain
  shims, or runtime flags the agent session should receive

Roger should keep the same separation.

### 4. Hierarchical configuration scopes matter

Claude distinguishes:

- user scope
- project scope
- local project scope
- managed scope

That hierarchy is useful because repeatable worktree behavior needs both:

- team-shared checked-in defaults
- local machine-specific overrides

Roger already wants additive layering. Claude's scope model reinforces that
direction.

### 5. Project-defined reusable workflows belong in repo-local automation

Claude supports project-level commands/skills stored under `.claude/`.

The important lesson for Roger is not "copy Claude's exact directory layout."
The lesson is:

- app-specific setup logic should live in repo-visible, repeatable, auditable
  project automation
- the tool should discover and invoke that automation through a stable contract
  rather than hardcoding framework-specific rules

## Main Conclusion

Roger should emulate Claude's shape, not its exact implementation details.

That means:

1. keep single-checkout mode as the zero-config default
2. make worktree mode an explicit, elevated option
3. provide a small built-in set of first-party resource-isolation primitives
4. let repos define repeatable setup behavior through checked-in project hooks
   or scripts
5. keep local machine-specific overrides in an uncommitted local layer
6. separate worktree creation, resource materialization, session env injection,
   verification, and cleanup into distinct phases

## Recommended Roger Model For `0.1.0`

### A. Three operator modes

Roger should expose three explicit modes:

- `current_checkout`
  - default
  - uses the Roger Reviewer repo directory directly
  - no worktree created
  - appropriate for ordinary read-mostly review
- `named_instance`
  - still uses the current checkout
  - isolates selected mutable resources without cloning Roger state
  - useful when the repo can tolerate shared checkout but runtime resources need
    separation
- `worktree`
  - creates or adopts an isolated checkout
  - intended for conflicting local repo state, mutation-capable work, or app
    setups that need separate env/config/DB/runtime state

Roger should never silently upgrade an ordinary session into worktree mode.

### B. Two layers of worktree setup behavior

Roger should split worktree behavior into:

- first-party primitives Roger understands directly
- project-defined setup hooks Roger executes through a stable contract

This is the key to handling "tests, DB, whatever" without overfitting Roger to
any one stack.

### C. First-party primitives Roger should own directly in `0.1.0`

Roger should ship built-in behavior for a narrow set of common resource
classes.

#### 1. Env/config files

Primitive actions:

- `skip`
- `copy`
- `template_copy`
- `symlink`

Recommended default:

- do **not** automatically copy `.env`, `.env.local`, or similar secret-bearing
  files by default
- instead, if worktree mode is requested and these files are present but no
  explicit rule exists, preflight should surface that as user-config-required
  rather than guessing

Reason:

- silent secret copying is risky
- app-specific env semantics vary too much

#### 2. Ports

Primitive actions:

- `shared`
- `offset_from_base`
- `fixed_map`
- `disabled`

Recommended default:

- no implicit port rewriting in single-checkout mode
- in worktree/named-instance mode, port rewriting only occurs when the selected
  profile declares it

#### 3. Repo-local DBs

Primitive actions:

- `shared`
- `copy_snapshot`
- `fresh_empty`
- `env_redirect`

Recommended `0.1.0` scope:

- Roger should directly support file-backed DB copy/fresh flows
- DB server provisioning and vendor-specific lifecycle should remain profile or
  hook-driven, not Roger-core magic

#### 4. Docker / container naming

Primitive actions:

- `shared`
- `compose_project_suffix`
- `compose_project_prefix`
- `env_redirect`

Recommended default:

- no automatic rename unless the profile declares it
- preflight should detect likely compose/container-name conflicts and suggest a
  naming rule

#### 5. Cache directories

Primitive actions:

- `shared`
- `per_instance_subdir`
- `disabled`

Recommended default:

- shared by default unless the project profile opts into isolation

#### 6. Artifact and log directories

Primitive actions:

- `shared`
- `per_instance_subdir`

Recommended default:

- isolated per instance by default when Roger owns the path
- this is low-risk and usually high-value for debugging

## Recommended Project-Defined Hook Model

Roger should not try to encode every stack-specific setup rule in core. It
should let projects define repeatable lifecycle commands.

Suggested lifecycle phases:

- `preflight`
- `worktree_create`
- `materialize_resources`
- `session_env`
- `verify`
- `cleanup`

Recommended contract:

- each phase is optional
- phases receive a structured input payload
- the payload includes repo path, instance/worktree name, review mode, selected
  profile, and resolved resource plan
- scripts can return environment exports, warnings, or hard failures
- Roger records what ran and whether it succeeded

Important boundary:

- project hooks should extend Roger's setup behavior
- they should not replace Roger's canonical session/state model

## Recommended Config Shape

The exact filename can be decided later, but the config model should support:

- checked-in team defaults
- local uncommitted overrides
- per-profile worktree or named-instance behavior

Suggested logical shape:

```toml
[instances.default]
mode = "current_checkout"

[instances.profiles.webapp_review]
mode = "worktree"
display_name = "Webapp Isolated Review"

[instances.profiles.webapp_review.resources.env_files]
".env" = "copy"
".env.local" = "copy"

[instances.profiles.webapp_review.resources.ports]
strategy = "offset_from_base"
offset = 100

[instances.profiles.webapp_review.resources.logs]
strategy = "per_instance_subdir"

[instances.profiles.webapp_review.hooks]
preflight = "./.roger/hooks/preflight.sh"
materialize_resources = "./.roger/hooks/setup-worktree.sh"
session_env = "./.roger/hooks/export-env.sh"
verify = "./.roger/hooks/verify-worktree.sh"
cleanup = "./.roger/hooks/cleanup-worktree.sh"
```

The specific directory does not matter as much as these properties:

- scripts are repo-visible and versioned
- local overrides are supported separately
- the effective resolved setup is inspectable before Roger runs it

## Recommended Preflight Classification

Roger should classify setup state before launching the agent.

Suggested classes:

- `ready`
  - the selected mode/profile is safe to launch as-is
- `ready_with_actions`
  - Roger knows what setup it will run and can proceed with clear operator
    messaging
- `profile_required`
  - Roger detected mutable resources likely to conflict, but no profile or rule
    is declared
- `unsafe_default_blocked`
  - Roger believes the requested operation could affect shared mutable state in
    a misleading or destructive way
- `verification_failed`
  - setup ran, but health checks or environment verification did not pass

Suggested default behavior:

- for ordinary read-mostly review in the current checkout, do not over-warn
- for worktree/named-instance mode, fail honestly when resource conflicts are
  plausible and no explicit rule exists
- prefer "configure a profile" over hidden auto-magic

## When Roger Should Recommend A Separate Profile

A separate Roger profile should be rare.

Use a separate profile only when the Roger-owned durable state itself needs to
be isolated, for example:

- separate auth or credential domain
- separate review memory/search corpus for policy reasons
- separate trust or plugin policy that should not mix with the main profile
- deliberate operator separation between environments or organizations

Do **not** require a separate Roger profile merely because:

- ports differ
- `.env` files differ
- local DB paths differ
- Docker naming differs
- tests need isolated temp state

Those should stay inside named-instance/worktree configuration.

## Recommended Round 04 Decision

Roger should adopt this product rule:

- `0.1.0` ships a small set of first-party isolation primitives for env files,
  ports, repo-local DB paths, docker/container naming, caches, artifacts, and
  logs
- Roger never guesses secret-copy or DB-provisioning behavior by default
- project-defined checked-in hooks/scripts provide repeatable app-specific
  setup, verification, and cleanup
- local uncommitted overrides allow machine-specific customization
- worktree mode is explicit and elevated; single-checkout mode remains the
  default path

## Why This Is Better Than Trying To Automate Everything

- it preserves Roger's local-first, inspectable architecture
- it supports highly application-specific setup without hardcoding framework
  magic into Roger core
- it keeps the default path simple
- it gives teams a real place to encode repeatable setup logic
- it keeps security-sensitive behavior like `.env` handling explicit
- it matches the strongest lesson from Claude's design: worktree isolation is
  useful, but environment realization must still be project-aware
