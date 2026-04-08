# Plan For Extension Setup And Happy-Path Validation

## Purpose

This plan reconciles two related failures:

1. Roger's current test posture did not catch several user-visible happy-path
   problems around extension setup, release install, and GitHub-surface UI.
2. Roger's implemented command surface drifted away from the canonical product
   plan, so low-level `rr bridge ...` commands are acting like the primary UX
   even though the accepted product shape is a guided `rr extension setup` flow.

This document does not replace the canonical product plan in
[`PLAN_FOR_ROGER_REVIEWER.md`](PLAN_FOR_ROGER_REVIEWER.md). It narrows one
specific recovery lane: make the extension/browser setup flow coherent and make
the validation system strong enough that these regressions are caught before a
human notices them on GitHub.

---

## Current Truth

### What the canonical plan already says

The accepted product shape is already clear in existing repo docs:

- [`PLAN_FOR_ROGER_REVIEWER.md`](PLAN_FOR_ROGER_REVIEWER.md) says the intended
  normal-path browser workflow is `rr extension setup [--browser ...]`, using
  the installed `rr` binary in host mode, without requiring the user to type an
  extension id or provide a separate `rr-bridge` path.
- [`EXTENSION_PACKAGING_AND_RELEASE_CONTRACT.md`](EXTENSION_PACKAGING_AND_RELEASE_CONTRACT.md)
  says `rr extension setup` is the primary user-facing flow and that
  `rr bridge pack-extension` / low-level bridge operations are support or
  development surfaces, not the normal onboarding path.
- [`REVIEW_FLOW_MATRIX.md`](REVIEW_FLOW_MATRIX.md) already defines
  `F02.1 Guided Browser Setup And Verification` as a first-class happy path.

So the repo does not have a planning gap on intent. It has an implementation
and validation gap.

### What the implementation currently exposes

Current CLI help still exposes:

```text
rr bridge export-contracts
rr bridge verify-contracts
rr bridge pack-extension
rr bridge install --extension-id <id> [--bridge-binary <path>] ...
rr bridge uninstall
```

and now also exposes the intended primary product commands:

- `rr extension setup`
- `rr extension doctor`

That means the repo has the right top-level command names, but the
implementation is still not truthful enough: setup/doctor can pass while the
registered `rr` binary still fails to act as a Native Messaging host at
runtime.

### What the current blessed automated E2E actually covers

The one blessed automated E2E,
[`packages/cli/tests/e2e_core_review_happy_path.rs`](../packages/cli/tests/e2e_core_review_happy_path.rs),
is valuable but intentionally narrow:

- it calls `roger_cli::run(...)` in-process rather than driving the built `rr`
  binary end to end as an external user would
- it seeds findings and drafts directly through storage/app-core helpers
- it uses a posting double rather than a real browser/bridge/setup path
- it does not exercise `F02 Browser Launch` or `F02.1 Guided Browser Setup`
- it does not render or inspect the GitHub content-script panel
- it does not validate light/dark GitHub readability

This means the current E2E is not wrong; it is defending a different promise.

### What the extension UI currently does

The current content script in
[`apps/extension/src/content/main.js`](../apps/extension/src/content/main.js)
constructs the PR panel entirely with inline styles and a Roger-owned hardcoded
palette. It does not consume GitHub/Primer visual tokens. That is why the panel
can look alien or unreadable on real GitHub pages, especially in dark mode.

---

## Root Cause Analysis

### 1. We treated one blessed E2E as if it defended all happy paths

It does not. Roger's current E2E defends the local review core. The bugs we are
seeing are in a different surface area:

- extension onboarding
- browser bridge packaging and registration
- release asset/install reality
- GitHub-page UI presentation

Those require their own executable suites and smoke gates.

### 2. We allowed command-surface drift

The plan says the product should feel like:

- install Roger locally
- optionally run `rr extension setup`
- then `rr extension doctor`

But the implementation still makes the operator think in terms of:

- `rr bridge pack-extension`
- `rr bridge install --extension-id ... --bridge-binary ...`

That is a development/repair mental model, not a user-facing product model.

### 3. The validation matrix is too fragmentary around browser/setup flows

Today we have:

- one local-review E2E
- some bridge/CLI smoke tests
- setup/doctor and host-manifest checks

What we still did not have until live probing:

- a named proof that the actual registered `rr` process answers a Native
  Messaging request over stdin/stdout when launched by the browser contract

That is why a real PR-page `Start` click could hang even after setup guidance
and doctor checks appeared healthy.
- browser launch smoke ids in metadata
- release proof and install proof in separate lanes

But we do not yet have a coherent "happy path coverage matrix" that maps each
published user-visible flow to:

- one primary command surface
- one executable suite family
- one CI tier
- one release/manual smoke if the flow crosses a brittle real-world boundary

### 4. The presentation layer has no explicit readability contract

The extension panel is currently treated as "just some injected buttons." That
is too weak. If Roger wants to live inside GitHub PR pages, the panel needs its
own visual contract:

- GitHub-native tone
- readable in light and dark themes
- busy/disabled states remain legible
- fallback-only status remains honest but not visually broken

---

## Goal

Bring Roger to a standard where every user-visible happy path in the browser
lane is defended by named, executable validation and where the command surface
matches the accepted product model instead of exposing repair internals as the
main UX.

This means:

- a new user should encounter `rr extension setup`, not low-level bridge verbs
- the extension panel should feel like GitHub, not an unrelated floating card
- release/install/browser setup claims should each map to a suite or smoke gate
- regressions like unreadable panel styling, missing installer assets, manual
  extension-id prompts, or wrong host-mode assumptions should be caught by
  tests before a human spots them

---

## Non-Goals

- replacing the whole existing validation framework
- adding many heavyweight automated E2Es by default
- making the browser extension part of the base one-line Roger install
- turning the extension into the source of truth for review state
- broadening provider support as part of this lane

The answer is not "make everything an E2E." The answer is "make every product
claim map to the cheapest truthful defending suite."

---

## Target Standard

Every user-visible happy path must have all of the following:

1. A named flow in [`REVIEW_FLOW_MATRIX.md`](REVIEW_FLOW_MATRIX.md)
2. A primary user-facing command or interaction
3. A canonical support claim in docs/help text
4. At least one executable validation suite that defends the claim
5. A defined CI tier where that suite runs
6. A manual smoke requirement if the boundary is too expensive or brittle to
   automate fully in `0.1.x`

No command should remain primary-user-facing if:

- it requires internal identifiers the product is supposed to discover itself
- it assumes dev-only knowledge like a separate host binary path
- it exists mainly to assemble or repair lower-level bridge assets

No UI surface should ship without:

- a theme/readability contract
- a validation path for both GitHub light and dark themes

---

## Command-Surface Reconciliation

### Primary user-facing surface

The intended user-facing command set for the extension/browser lane should be:

- `rr extension setup [--browser chrome|brave|edge]`
- `rr extension doctor`
- `rr extension uninstall`

These commands must describe the product workflow in product language:

- prepare unpacked extension artifact
- instruct the one required manual browser step
- learn extension identity through Roger-owned discovery or self-registration
- register the installed `rr` binary in host mode
- verify setup truthfully

### Dev/repair surface

Low-level bridge commands may still exist, but they must be explicitly treated
as dev/repair surfaces:

- `rr bridge export-contracts`
- `rr bridge verify-contracts`
- `rr bridge pack-extension`
- host registration repair commands if still needed

Rules:

- dev/repair commands must not be the default onboarding path
- normal docs and help text must not require `--extension-id` or
  `--bridge-binary`
- the normal-path UX must speak in terms of `rr extension setup` and
  `rr extension doctor`
- `rr --help` should present the product surface first; low-level commands can
  remain under deeper help or clearly demoted sections

### Contract to enforce

The extension setup contract must assert:

- no manual extension-id entry in the normal path
- no normal-path separate `rr-bridge` binary assumption
- installed `rr` is the host-mode binary for normal flows
- repair guidance points back to `rr extension setup` / `rr extension doctor`
  rather than requiring the user to assemble host registration by hand

---

## Validation Uplift Strategy

## Principle

Keep the one-blessed-E2E discipline for the local review core, but add missing
acceptance, integration, and release-smoke coverage for browser/setup/install
flows.

### What stays as-is

- `E2E-01 Core review happy path` remains the blessed automated local-review E2E
- the E2E budget does not grow casually

### What must be added

#### 1. Command-surface contract tests

Purpose:
- ensure the binary help text, docs, and supported primary flows agree

Needed suites:

- `int_cli_extension_command_surface`
  - verifies `rr extension setup`, `rr extension doctor`, and
    `rr extension uninstall` exist once implemented
  - verifies normal help text does not present manual extension-id entry as the
    primary path
  - verifies dev/repair `rr bridge ...` commands are clearly demoted

- `int_cli_primary_help_truth`
  - snapshots the primary help surface and robot envelopes for setup/doctor
  - fails when canonical product commands disappear or low-level repair commands
    become the only visible path again

#### 2. Guided browser setup acceptance

Purpose:
- defend `F02.1 Guided Browser Setup And Verification`

Needed suite:

- `accept_extension_setup_guided`
  - runs the real `rr` command surface, not an internal helper only
  - verifies Roger prepares the extension artifact, learns extension identity
    via discovery/self-registration fixture, registers installed `rr` host mode,
    and returns truthful doctor-style readiness
  - explicitly proves the normal path works without `--extension-id`

This is acceptance, not a new blessed E2E.

#### 3. Browser panel render/readability integration

Purpose:
- defend the injected GitHub PR panel as a real product surface

Needed suites:

- `int_extension_panel_render_contract`
  - verifies the panel structure, action ids, status/badge nodes, and theme
    token/class wiring
- `int_extension_panel_theme_readability`
  - verifies readable surface/text/button states under GitHub-like light and
    dark theme variables
  - specifically covers idle, busy/loading, and fallback-only states

This suite family should catch the unreadable panel problem shown in the
current screenshot.

#### 4. Native Messaging and fallback truthfulness

Purpose:
- keep the browser launch surfaces honest

Existing direction:
- continue to use `int_bridge_*` plus supported-browser smoke suites

Needed uplift:
- ensure the setup/doctor result envelopes are tied to the same contracts
- ensure launch-only fallback messaging remains readable and bounded

#### 5. Release/install proof

Purpose:
- defend the public install surface users actually touch

Needed rule:
- every release/install claim must keep mapping to:
  - artifact verification
  - latest release live installer proof
  - optional browser/setup smoke only if that lane is claimed for the release

The current `rr-jj1e` / release-proof direction is correct and should remain.

---

## Proposed Coverage Matrix

| Product flow | Canonical surface | Defending suite(s) | Tier |
|---|---|---|---|
| Local core review | `rr review/resume/findings/status/refresh` | `e2e_core_review_happy_path` + existing int/accept suites | nightly/release |
| Guided browser setup | `rr extension setup` | `accept_extension_setup_guided`, `int_cli_extension_command_surface` | gated/nightly |
| Setup truth and repair | `rr extension doctor` | `int_cli_extension_command_surface`, `accept_extension_setup_guided` | gated/nightly |
| Browser panel render | GitHub PR content script | `int_extension_panel_render_contract` | pr/gated |
| Browser panel readability | GitHub PR content script | `int_extension_panel_theme_readability` + manual smoke | pr/release |
| Launch with Native Messaging | extension -> local Roger | existing `int_bridge_*` + host-runtime round-trip smoke + browser smoke suites | gated/release |
| Public install path | release assets + `rr-install` | release-proof lanes + live installer proof | release |

---

## Command Model To Reach

### Phase 1 output

The top-level help should converge on something like:

```text
rr review
rr resume
rr return
rr findings
rr status
rr refresh
rr search
rr update
rr extension setup
rr extension doctor
rr extension uninstall
rr robot-docs
```

The `bridge` namespace should still exist, but as explicitly lower-level
machinery:

```text
rr bridge export-contracts
rr bridge verify-contracts
rr bridge pack-extension
...
```

### UX rules

- a normal user should not need to know what "pack-extension" means
- a normal user should not need to provide an extension id
- a normal user should not need to know whether the host mode uses a dedicated
  bridge binary or the installed `rr`
- the system should present one guided setup path and one truthful doctor path

---

## Rollout Phases

### Phase A — Lock the command contract

- decide the exact top-level `rr extension` surface
- demote or hide low-level `rr bridge` commands from normal-path help
- add command-surface integration tests before further browser/setup changes

Exit:
- docs, help text, and flow matrix agree on the primary command model

### Phase B — Make guided setup real

- implement `rr extension setup`
- remove manual extension-id from the normal path
- target installed `rr` host mode, not user-facing separate `rr-bridge`
- implement `rr extension doctor`

Exit:
- `F02.1` happy path exists as a truthful executable flow

### Phase C — Defend the GitHub panel as a real surface

- replace the current hardcoded inline palette with GitHub/Primer-aligned
  styling
- add DOM/render and light/dark readability tests
- add manual smoke evidence requirements for release lane

Exit:
- the current unreadable panel class of regression is caught by tests

### Phase D — Tie release/install/setup claims together

- ensure release notes, installer assets, setup docs, and browser-lane claims
  stay aligned
- require release smoke only when browser/setup support is claimed

Exit:
- release/install/browser claims are truthful and connected to named evidence

---

## Exit Criteria

This recovery lane closes only when all of the following are true:

- `rr extension setup` and `rr extension doctor` exist and are the primary
  user-facing browser setup path
- normal docs/help no longer require manual extension-id entry or a separate
  bridge-binary path
- the extension panel uses GitHub/Primer-aligned styling and is readable on
  GitHub light and dark themes
- command-surface drift is caught by executable tests
- guided setup truth is caught by executable acceptance coverage
- browser panel readability regressions are caught by executable validation plus
  manual smoke where needed
- release/install/browser support claims each map to named suites or smoke gates

---

## Proposed Bead Spine

This plan should eventually decompose into a small spine like:

1. Freeze primary `rr extension` command contract
2. Implement guided setup and doctor
3. Remove manual extension-id normal-path requirement
4. Replace separate-host assumption with installed `rr` host mode
5. Add command-surface integration tests
6. Add guided setup acceptance suite
7. Add extension panel GitHub/Primer style bead
8. Add light/dark readability validation bead
9. Reconcile release/browser claim gates with the new setup surface

Open beads already align with this direction:

- `rr-ivjk.2`
- `rr-ivjk.3`
- `rr-ivjk.4`
- `rr-ivjk.5`
- `rr-d0ny.1`
- `rr-d0ny.2`

The next planning step after this document is not "add random tests." It is:

- align the command contract
- assign each product claim a named validating suite
- then implement the missing suites and UX surfaces in that order
