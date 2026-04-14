# Configuration Capability And Defaulting Contract

Status: Proposed support contract  
Audience: Roger maintainers implementing layered config, launch routing, TUI
baseline visibility, CLI surface simplification, and extension/setup hardening  
Scope: `0.1.x` configuration capability requirements, defaulting rules,
operator-versus-machine exposure, file/path-bearing settings, and the use-case
matrix that ties config into the TUI and CLI without overburdening operators

---

## Why this contract exists

Roger's accepted plan already wants:

- layered, inspectable config rather than ambient shell-state behavior
- explicit launch profiles and isolation policy
- truthful preflight before worktree creation or resource rewrites
- demotion of bridge/setup repair internals out of the ordinary user path

The live repo still exposes a thinner and more inconsistent reality:

- [`packages/config/src/lib.rs`](../packages/config/src/lib.rs) is only a small
  env/default shim
- [`packages/app-core/src/cli_config.rs`](../packages/app-core/src/cli_config.rs)
  duplicates part of that shim
- [`packages/app-core/src/lib.rs`](../packages/app-core/src/lib.rs) and
  [`packages/storage/src/lib.rs`](../packages/storage/src/lib.rs) already model
  `LocalLaunchProfile`, routing, and persisted launch-profile records
- [`packages/worktree-manager/src/lib.rs`](../packages/worktree-manager/src/lib.rs)
  already models explicit preflight classification and resource planning
- [`packages/cli/src/lib.rs`](../packages/cli/src/lib.rs) still exposes too many
  raw launch, bridge, and repair details in one flat surface

This contract narrows that gap. It does not replace
[`PLAN_FOR_ROGER_REVIEWER.md`](PLAN_FOR_ROGER_REVIEWER.md). It makes one part of
that plan implementation-ready: what configuration items exist, what
capabilities each item must declare, where each item belongs, which items may
carry file paths, and which details should stay machine-side rather than
operator-facing.

---

## Non-goals

- turning Roger into a generic enterprise policy engine
- making every launch decision a user prompt
- preserving raw env vars and repair flags as the primary UX
- treating config as a substitute for validation or support-claim proof
- silently escalating from `current_checkout` to `named_instance` or
  `worktree`

---

## Core decisions

### D1. Every config item must declare capability metadata

No new config item should land as an unnamed constant, an ambient env var, or a
raw TOML field without a documented capability profile.

### D2. Routine operator surfaces consume resolved config, not raw config

The TUI and normal CLI flows should show the resolved baseline:

- launch profile
- provider and support tier
- isolation mode
- degraded fallback reason when relevant

They should not force the operator to manage raw binary paths, extension IDs,
host manifest roots, or per-resource rewrite rules during ordinary review work.

### D3. Launch default remains `current_checkout`

The accepted default remains:

- `current_checkout` for ordinary read-mostly review
- `named_instance` when mutable repo-local resources would collide but a second
  checkout is not yet required
- `worktree` only on explicit operator choice or explicit preflight approval

This preserves the existing plan and ADR direction while still allowing a
worktree-first future if a later proof pass justifies it.

### D4. Repair-only knobs remain real, but demoted

Settings such as extension identity overrides, host-binary overrides, install
roots, and update transport roots remain valid for repair, CI, and maintainer
work. They are not part of the normal operator mental model and should not stay
in the front-door help surface.

### D5. File-path-bearing settings are allowed only for bounded families

Roger should allow explicit paths for:

- canonical store roots
- provider binary locations
- launch/worktree roots
- per-instance artifact, log, and cache roots
- extension guided-profile and install roots
- allowlisted non-secret resource materialization

It must not rely on hidden path inference, implicit secret-file copying, or
ambient mutable state as a second config channel.

---

## Required capability fields for every config item

Every config item or config family must declare all of the following fields.

| Field | Requirement |
|---|---|
| `key` | Stable dot-path key used across docs, config files, status output, and robot envelopes. |
| `concern_family` | One of `global`, `provider`, `launch_profile`, `isolation`, `extension`, `update`, `repair`, or another bounded family. |
| `value_shape` | Declare the type: `enum`, `string`, `bool`, `integer`, `duration_ms`, `path`, `path_list`, `map`, or structured object. |
| `allowed_layers` | Declare which layers may set it: built-in, user-global, workspace, repo, launch-profile, mode default, instance/worktree overlay, per-review override, repair env/flag. |
| `default_owner` | State which layer normally supplies the value when the operator does nothing. |
| `surface_class` | One of `routine_input`, `baseline_visible`, `advanced_visible`, `repair_only`, or `maintainer_only`. |
| `override_channels` | Name whether the item may be changed through config files, CLI override, TUI baseline editing, extension intake, env var, or maintainer-only flag. |
| `supports_paths` | Explicitly say whether path values are allowed, and if so whether they are file paths, directory paths, executable paths, or URL roots. |
| `inspectability` | The resolved value and provenance must be available through `rr status`, `rr doctor`, robot envelopes, or persisted launch/preflight state. |
| `validation_gate` | State the proof layer that defends it: unit resolution, integration preflight, setup/doctor, or release/maintainer validation. |
| `fail_closed_rule` | State what happens when the value is missing, conflicting, unavailable, or unsafe. |
| `consumer_surfaces` | Name which surfaces actually consume the item: CLI review flow, TUI baseline, extension setup, update, etc. |

Rules:

- items without these fields are not ready to become part of Roger's supported
  config surface
- env vars may remain as compatibility or repair channels, but they still need
  the same metadata and must not become the only documented source of truth
- path-bearing items must record normalized absolute paths or normalized URL
  roots in resolved output

---

## Layer model and on-disk shape

This contract adopts the canonical layer order from
[`PLAN_FOR_ROGER_REVIEWER.md`](PLAN_FOR_ROGER_REVIEWER.md) and makes the on-disk
shape concrete enough for implementation.

### Layer order

1. built-in defaults
2. user-global templates
3. optional workspace/project profiles
4. repo-specific templates
5. selected launch profile
6. mode defaults for `current_checkout`, `named_instance`, or `worktree`
7. named-instance or worktree overlay
8. bounded per-review overrides
9. repair-only env vars or maintainer flags

Rules:

- layers 1 through 8 define the normal product model
- layer 9 exists, but it is outside normal onboarding and should be marked as
  repair or maintainer provenance in status/doctor output
- per-review overrides are transient intake inputs, not durable repo policy

### Recommended `0.1.x` file locations

The exact parser and serialization format can evolve, but `0.1.x` should settle
on one structured file format, preferably TOML.

Recommended default locations:

- user-global: `~/.config/roger-reviewer/config.toml`
- optional workspace profile: `~/.config/roger-reviewer/workspaces/<id>.toml`
- repo-specific checked-in config: `./.roger/repo.toml`
- repo-local uncommitted overlay: `./.roger/local.toml`

Rules:

- `./.roger/local.toml` is for machine-specific or operator-specific local
  overrides and should be ignored by git
- repo-checked-in config must not carry secret-bearing values
- launch profiles may live inside user-global, workspace, or repo config, but
  the resolved profile must behave like one inspectable object regardless of
  where it was declared

---

## Visibility classes

Roger should classify config items by how much of the setting belongs in the
routine operator workflow.

| Class | Meaning |
|---|---|
| `routine_input` | Operator may reasonably supply this during normal review work. |
| `baseline_visible` | Operator should see the resolved value in status/TUI, but should not be forced to provide it each time. |
| `advanced_visible` | Operator may inspect or edit it in settings/baseline workflows, but it should not dominate the front-door path. |
| `repair_only` | Valid only for repair, CI, or dev workflows. Demoted from normal help and onboarding. |
| `maintainer_only` | Build, packaging, release, or test-lane setting. Not part of daily Roger use. |

Rules:

- `baseline_visible` is the main class for launch-profile, provider-baseline,
  and isolation-mode output
- a `repair_only` item may still be documented, but only in repair or maintainer
  docs and deeper help surfaces

---

## Capability matrix for required configuration families

### A. User-global anchors and provider/binary defaults

| Key | Concern | Allowed layers | Value shape | Paths? | Default owner | Surface class | Override channels | Fail-closed rule |
|---|---|---|---|---:|---|---|---|---|
| `store.root` | Canonical Roger profile store root | built-in, user-global, repair env | path | yes | built-in/user-global | `advanced_visible` | config file, env | Missing root resolves to Roger default; unreadable root blocks startup/doctor truthfully. |
| `providers.default` | Preferred provider baseline for new sessions | built-in, user-global, workspace, repo, per-review | enum | no | built-in/user-global | `baseline_visible` | config file, bounded CLI/TUI override | Unsupported provider blocks selection; Roger must not silently widen support claims. |
| `providers.<provider>.binary_path` | Executable path for supported local providers | built-in, user-global, repair env | path | yes | built-in/user-global | `advanced_visible` | config file, env | Missing executable yields `doctor`/launch preflight failure for that provider only. |
| `launch.defaults.by_surface.<surface>` | Default named launch profile for CLI, TUI, extension, and robot surfaces | built-in, user-global, workspace, repo | string | no | built-in/user-global | `baseline_visible` | config file, TUI settings | Unknown profile degrades truthfully to “not found”; Roger must not invent a similarly named profile. |
| `extension.default_browser` | Browser choice for guided setup/doctor | built-in, user-global | enum | no | built-in/user-global | `advanced_visible` | config file, setup flag | Unsupported browser blocks setup/doctor with bounded repair guidance. |

Rules:

- provider-specific binary paths should use one map-shaped family rather than a
  one-off env var per provider
- `RR_STORE_ROOT` and `RR_OPENCODE_BIN` can remain compatibility channels, but
  their model should fold into `store.root` and `providers.opencode.binary_path`

### B. Launch-profile items

| Key | Concern | Allowed layers | Value shape | Paths? | Default owner | Surface class | Override channels | Fail-closed rule |
|---|---|---|---|---:|---|---|---|---|
| `launch_profiles.<id>.name` | Human-facing profile label | user-global, workspace, repo | string | no | config file | `baseline_visible` | config file, settings UI | Missing label falls back to profile ID only; does not affect routing. |
| `launch_profiles.<id>.ui_target` | Preferred entry surface (`cli` or `tui`) | user-global, workspace, repo, per-review | enum | no | profile | `baseline_visible` | config file, bounded override | Unsupported target degrades with persisted resolved-launch reason. |
| `launch_profiles.<id>.terminal_environment` | Preferred terminal shell host | user-global, workspace, repo | enum | no | profile | `advanced_visible` | config file, settings UI | Unavailable environment degrades via routing decision with explicit reason. |
| `launch_profiles.<id>.multiplexer_mode` | Preferred muxer strategy | user-global, workspace, repo | enum | no | profile | `advanced_visible` | config file, settings UI | Unavailable muxer falls back to `none` or next supported value with persisted reason. |
| `launch_profiles.<id>.reuse_policy` | `reuse_if_possible` versus `always_new` | user-global, workspace, repo | enum | no | profile | `advanced_visible` | config file, settings UI | Invalid value blocks profile resolution. |
| `launch_profiles.<id>.provider_preference` | Preferred provider baseline for that profile | user-global, workspace, repo, per-review | enum | no | profile | `baseline_visible` | config file, bounded override | Unsupported provider blocks launch or falls back only when an explicit support-preserving rule exists. |
| `launch_profiles.<id>.repo_root` | Repo binding or path anchor for the profile | workspace, repo | path | yes | profile | `advanced_visible` | config file | Missing/unreadable repo root blocks profile use. |
| `launch_profiles.<id>.worktree_root` | Base directory for worktree creation when a worktree is approved | user-global, workspace, repo | path | yes | workspace/repo | `advanced_visible` | config file, settings UI | Unwritable path yields `verification_failed`; Roger must not silently create worktrees elsewhere. |

Rules:

- launch profiles are user-facing as named baselines, not as a bag of raw flags
- routine flows may refer to the profile by name or ID; they should not require
  the operator to restate its internal fields

### C. Isolation and resource-policy items

| Key | Concern | Allowed layers | Value shape | Paths? | Default owner | Surface class | Override channels | Fail-closed rule |
|---|---|---|---|---:|---|---|---|---|
| `isolation.default_mode` | Baseline launch mode | built-in, workspace, repo, profile | enum | no | built-in/profile | `baseline_visible` | config file, bounded override | Must default to `current_checkout`; unsupported value blocks resolution. |
| `isolation.named_instance.default` | Whether `named_instance` is the first isolation step when mutable resources collide | built-in, workspace, repo, profile | bool/enum | no | built-in/profile | `advanced_visible` | config file | Unsafe opt-out should yield `unsafe_default_blocked` rather than silent sharing. |
| `isolation.worktree.require_explicit_approval` | Whether worktrees always require confirmation | built-in, workspace, repo | bool | no | built-in | `advanced_visible` | config file | `true` by default; if disabled, preflight still must remain inspectable and fail closed. |
| `isolation.resource_rules.env_files` | Policy for `.env` and other repo-local config files | built-in, workspace, repo | structured map | yes | built-in/repo | `advanced_visible` | config file only | Secret-bearing files never copy implicitly; blocked if policy is missing for a requested copy. |
| `isolation.resource_rules.ports` | Port rewrite or collision policy | built-in, workspace, repo | structured map | no | built-in/repo | `advanced_visible` | config file only | Collision without declared strategy yields `unsafe_default_blocked`. |
| `isolation.resource_rules.repo_dbs` | Repo-local DB rewrite/shared policy | built-in, workspace, repo | structured map | yes | built-in/repo | `advanced_visible` | config file only | Mutation-capable sharing without explicit override is blocked. |
| `isolation.resource_rules.container_names` | Compose/container naming strategy | built-in, workspace, repo | structured map | no | built-in/repo | `advanced_visible` | config file only | Missing strategy yields collision block when a collision is detected. |
| `isolation.resource_rules.caches` | Cache root sharing/isolation strategy | built-in, workspace, repo | structured map | yes | built-in/repo | `advanced_visible` | config file only | Unsafe shared mutable cache plan blocks if preflight cannot verify it. |
| `isolation.resource_rules.artifacts` | Artifact directory strategy | built-in, workspace, repo | structured map | yes | built-in/repo | `advanced_visible` | config file only | Roger-managed artifacts stay canonical unless a declared per-instance root exists. |
| `isolation.resource_rules.logs` | Log directory strategy | built-in, workspace, repo | structured map | yes | built-in/repo | `advanced_visible` | config file only | Missing isolated log root blocks only when isolation is required. |
| `isolation.copy_allowlist` | Explicit allowlist for non-secret file materialization | workspace, repo | path_list | yes | repo | `advanced_visible` | config file only | Absent allowlist means “no copy”; never infer from filename similarity. |

Rules:

- these are policy items, not ordinary review flags
- they belong in config files and preflight output, not in the top-level review
  command line
- resolved resource plans must be visible in `rr doctor`, preflight output, and
  persisted launch/preflight state

### D. Bounded per-review overrides

Per-review overrides are part of the launch/intake contract, not durable repo
policy. They still need a capability profile.

| Key | Concern | Allowed layers | Value shape | Paths? | Default owner | Surface class | Override channels | Fail-closed rule |
|---|---|---|---|---:|---|---|---|---|
| `review.override.provider` | Operator override for provider choice | per-review | enum | no | none | `routine_input` | CLI or explicit local UI | Must stay within supported providers and declared support tiers. |
| `review.override.launch_profile_id` | Override the named launch profile | per-review | string | no | none | `routine_input` | CLI or TUI/open flow | Unknown profile blocks; no fuzzy fallback. |
| `review.override.ui_target` | Override `cli` versus `tui` | per-review | enum | no | none | `routine_input` | CLI or TUI/open flow | Unsupported target degrades truthfully or blocks. |
| `review.override.isolation_mode` | Explicit `current_checkout`, `named_instance`, or `worktree` request | per-review | enum | no | none | `routine_input` | CLI or preflight prompt | Unsafe requested mode blocks; Roger must not coerce it silently. |
| `review.override.instance_name` | Explicit named-instance binding | per-review | string | no | none | `advanced_visible` | CLI or TUI settings | Ambiguous or conflicting binding blocks. |
| `review.override.worktree_preference` | Explicit worktree ask or opt-down | per-review | enum | no | none | `advanced_visible` | CLI or preflight prompt | Worktree creation remains subject to preflight verification and approval. |

Rules:

- only bounded launch-shaping inputs belong here
- per-review overrides must not weaken provider safety posture, trust floors,
  approval rules, or posting authority

### E. Extension and browser-lane items

| Key | Concern | Allowed layers | Value shape | Paths? | Default owner | Surface class | Override channels | Fail-closed rule |
|---|---|---|---|---:|---|---|---|---|
| `extension.browser` | Browser selection for setup/doctor | built-in, user-global, per-command | enum | no | user-global | `routine_input` in setup only | config file, setup flag | Unsupported browser blocks setup/doctor. |
| `extension.guided_profile_root` | Browser profile root used for guided identity discovery | user-global, repair env | path | yes | built-in/user-global | `repair_only` | config file, env | Missing path only blocks the guided discovery path; doctor must explain fallback. |
| `extension.install_root` | Host manifest install root | user-global, repair flag | path | yes | user-global/HOME | `advanced_visible` in setup only | config file, repair flag | Missing/unwritable root blocks setup/doctor truthfully. |
| `extension.identity_override` | Manual extension ID override | repair env, repair flag | string | no | none | `repair_only` | env, repair command | Never required in the normal path; if missing, setup must continue to demand guided discovery. |
| `extension.host_binary_override` | Manual `rr` host binary override | repair env, repair flag | path | yes | current executable | `repair_only` | env, repair command | Wrong path blocks doctor/setup and marks repair provenance. |
| `extension.registration_wait_ms` | Setup wait budget for extension self-registration | built-in, repair env | duration_ms | no | built-in | `repair_only` | env only | Timeout yields blocked setup with bounded repair guidance; must not convert to synthetic success. |

Rules:

- `rr extension setup`, `rr extension doctor`, and `rr extension uninstall`
  remain the product-facing browser lane
- extension identity and host-binary overrides remain real, but are demoted out
  of normal onboarding and ordinary help

### F. Update and maintainer-lane items

| Key | Concern | Allowed layers | Value shape | Paths? | Default owner | Surface class | Override channels | Fail-closed rule |
|---|---|---|---|---:|---|---|---|---|
| `update.channel` | Stable versus rc track | built-in, user-global, command | enum | no | built-in | `advanced_visible` | config file, update command | Unknown channel blocks update preflight. |
| `update.version_pin` | Explicit version target | command only | string | no | none | `advanced_visible` | update command | Unknown/unpublished version blocks apply. |
| `update.api_root` | Metadata API root | maintainer, repair | path/url root | yes | built-in | `maintainer_only` | update command | Missing or invalid root blocks only the update flow. |
| `update.download_root` | Artifact download root | maintainer, repair | path/url root | yes | built-in | `maintainer_only` | update command | Missing or invalid root blocks only the update flow. |
| `update.target` | Target triple override | command only | string | no | detected target | `maintainer_only` | update command | Invalid target blocks update. |

Rules:

- update transport roots and target overrides belong only in `rr update`
- they are not part of the day-to-day review surface and should not leak into
  `rr review`, `rr status`, or TUI baseline controls

---

## Use-case matrix

The matrix below describes what Roger should do when the operator does not
override anything, what the operator should see, and where overrides belong.

| Use case | Default resolved behavior | What the operator sees | Allowed overrides | Hidden or demoted details |
|---|---|---|---|---|
| Ordinary CLI review in the current checkout | Use the repo/default launch profile, default provider, `current_checkout`, and machine-resolved terminal/muxer routing. | Resolved provider, launch profile, isolation mode, and any degraded routing reason. | Provider, launch profile, `ui_target`, explicit isolation mode. | Binary paths, worktree root, port rewrites, env-file policy, bridge/setup internals. |
| Repo-local review where mutable resources would collide | Preflight prefers `named_instance` with declared per-resource rewrites. | `ready_with_actions` or equivalent preflight result naming the resources that would collide. | Explicit instance name, explicit worktree request, opt-down only when safe. | Underlying rewrite map details unless the user opens the preflight detail view. |
| Dirty working tree plus target branch mismatch | Preflight recommends or requires `worktree`. No silent escalation. | A plain-language preflight prompt or blocked result showing why worktree isolation is needed. | Approve worktree, select worktree-capable profile/root, or abort. | Internal worktree-create hooks and resource materialization details. |
| Browser-launched review from a PR page | Extension triggers the surface-default launch profile and provider; review starts locally if preflight is satisfied. | Start/resume/open-local CTA plus truthful status mirror only. | None from the PR page beyond the requested action; baseline changes happen locally. | Extension ID, host-binary path, manifest roots, repair flags. |
| Browser setup on a healthy system | `rr extension setup` uses the default browser or a browser flag, learns identity, and registers the installed `rr` binary. | Browser choice, package path, manifest status, doctor result, next-step guidance. | Browser override, install-root override when needed. | Extension ID and host-binary override knobs. |
| Browser repair or CI setup | Same flow, but repair channels may supply identity or host-binary overrides if normal discovery fails. | Repair provenance in doctor/setup output. | Repair env vars or repair-specific flags. | These details stay out of ordinary onboarding/help. |
| Robot/automation review flow | Uses the same config resolution and preflight logic as human flows. | Machine-readable resolved profile, provider, isolation mode, degraded flags, and repair guidance. | Stable robot-safe override keys only for bounded launch shaping. | Human-only prose and TUI-only editing flows. |
| Update and release validation | Uses update defaults and release-specific roots only inside the update/release lane. | Update preflight or apply result only. | Channel, version, target, maintainer roots. | None of these items appear in ordinary review UX. |

---

## Adversarial synthesis for the main default-policy debates

This contract folds in adversarial passes over the most fragile default-policy
questions instead of pretending the answers were obvious.

| Concern | Inclusion case | Simplification case | Adopted rule |
|---|---|---|---|
| Worktrees by default | A worktree-first policy would make the safest isolated launch the ordinary path and reduce repeated topology reasoning. | It taxes the common read-mostly path, conflicts with the accepted default-mode plan, and explodes the routine config burden. | Keep `current_checkout` as the default. Use `named_instance` as the default isolation step when resources collide. Require explicit worktree choice or preflight approval. |
| Launch/provider/isolation exposure | These are truth-bearing inputs and must stay inspectable; the operator needs to see what Roger actually chose. | The current flat CLI already overexposes raw knobs; routine commands should not become a topology-management UI. | Show these as resolved baseline and preflight state, not as an ever-growing raw flag set. |
| File/path-capable settings | Operators and CI sometimes need explicit store roots, binary paths, worktree roots, and setup roots. | If every path becomes a normal flag, the UX becomes a bag of internals. | Allow path-bearing settings only for bounded families and prefer config files or repair flows over routine command flags. |
| Bridge/setup repair knobs | Explicit overrides remain necessary when setup discovery fails or CI must seed state deterministically. | Extension ID and host-binary flags do not belong in the happy path. | Keep them as `repair_only` with explicit provenance and demoted help placement. |

---

## TUI and CLI integration rules

### CLI rules

- `rr review`, `rr resume`, `rr return`, `rr status`, and `rr refresh` should
  operate on resolved config and session baseline, not on repeated low-level
  routing flags
- ordinary `rr review` should only carry bounded launch-shaping overrides when
  the operator is intentionally deviating from the baseline
- `rr status` and `rr doctor` should show:
  - resolved launch profile
  - resolved provider and support tier
  - resolved isolation mode
  - degraded fallback reason or blocked preflight reason
  - config provenance when requested or when a repair-only layer influenced the
    result
- `rr --help` should foreground product commands first and keep repair or
  maintainer knobs out of the front-door view

### TUI rules

- the TUI header or session-baseline pane should show the resolved profile,
  provider, and isolation mode for the active session
- baseline changes must be explicit, forward-only, and visible in history
- detailed resource plans, path rewrites, and copy allowlists belong in
  inspectable settings/preflight detail views, not the main attention queue

### Extension rules

- the extension may request start/resume/open-local actions, but it should not
  become a settings surface for low-level launch policy
- when local setup or bridge state is broken, the extension should point the
  operator toward local repair (`rr extension setup` / `rr extension doctor`)
  rather than surfacing raw bridge internals

---

## Validation and proof obligations

This contract changes support claims, so it needs proof obligations.

Minimum required proof layers:

1. unit coverage for layered resolution and provenance capture
2. integration coverage for preflight classification and fail-closed behavior
3. integration coverage for launch-profile fallback and degraded routing output
4. extension setup/doctor coverage for the demoted repair-only override paths
5. CLI/TUI/status proof that resolved baseline information is surfaced
   truthfully

Concrete proof targets:

- configuration resolution must prove layer precedence and provenance
- path normalization must prove absolute-path capture and rejection of invalid
  paths
- unsafe resource sharing must prove `unsafe_default_blocked`
- missing worktree root or invalid binary path must fail closed with bounded
  repair guidance
- robot envelopes must expose the same resolved baseline the human CLI/TUI
  rely on

---

## Immediate implementation consequences

This contract implies the following follow-on work:

- promote the current thin env/default shim in
  [`packages/config/src/lib.rs`](../packages/config/src/lib.rs) into a first-class
  layered config object model
- remove duplicated default constants from
  [`packages/app-core/src/cli_config.rs`](../packages/app-core/src/cli_config.rs)
  in favor of one authoritative resolver
- keep using persisted launch profiles and routing records in
  [`packages/storage/src/lib.rs`](../packages/storage/src/lib.rs) and
  [`packages/app-core/src/lib.rs`](../packages/app-core/src/lib.rs), but route
  them through the new config model
- keep explicit preflight and resource planning in
  [`packages/worktree-manager/src/lib.rs`](../packages/worktree-manager/src/lib.rs),
  while moving raw resource-policy inputs out of the routine command surface
- narrow the CLI surface so product help emphasizes resolved baselines and
  bounded overrides, while repair-only knobs move deeper

---

## Merge-back rule

This is a narrow support contract, not a second canonical plan.

If later work settles:

- final config file paths
- final user-visible flag names
- final TUI settings layout
- final update/release configuration surfaces

those accepted truths should be merged back into
[`PLAN_FOR_ROGER_REVIEWER.md`](PLAN_FOR_ROGER_REVIEWER.md) and any narrower
implementation-facing contracts that own those surfaces.
