# Extension Packaging And Release Contract

This document freezes the `0.1.0` packaging, contract-export, install, and
release shape for Roger's browser extension and Native Messaging bridge.

It narrows the accepted direction from
[`PLAN_FOR_ROGER_REVIEWER.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/PLAN_FOR_ROGER_REVIEWER.md),
[`RELEASE_AND_TEST_MATRIX.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/RELEASE_AND_TEST_MATRIX.md),
and
[`ADR 003`](/Users/cdilga/Documents/dev/roger-reviewer/docs/adr/003-browser-bridge-and-extension-dependency-policy.md)
into an implementation-facing contract for `rr-007.1`.

## Scope

This contract exists to answer the packaging questions that remained open after
the bridge-family decision was already settled:

- what the minimum extension build toolchain actually is
- how Rust-owned bridge contracts are exported into the extension tree
- which Roger-owned commands and release jobs own packing, installing, and
  publishing extension and host artifacts

It does not reopen the broader architecture:

- Native Messaging remains the serious `0.1.0` bridge
- the extension remains optional relative to the local CLI/TUI product

## `0.1.0` Packaging Baseline

Roger should treat the extension as a thin, typed, low-dependency exception in
an otherwise Rust-first local product.

### Selected build stance

The default `0.1.0` extension build toolchain is:

- `typescript` for transpile plus typecheck
- `chrome-types` for browser API typings
- Roger-owned scripts for manifest rendering, static asset staging, packaging,
  and archive naming

The default build pipeline is intentionally small:

- no framework runtime
- no browser runtime npm dependencies
- no bundler by default
- plain TypeScript compile plus Roger-owned copy/package steps unless a later
  browser entrypoint constraint proves a bundle step is materially necessary

Bundler rule:

- a bundler is allowed only if the extension entrypoint model or packaging
  repeatability cannot be kept honest with plain TypeScript output plus Roger
  scripts
- if that threshold is crossed, Roger may add exactly one narrow bundler behind
  the same Roger-owned build scripts and must justify it in writing

## Source Tree Shape

The extension packaging contract assumes one extension source tree and one
generated-contract location:

```text
apps/extension/
  manifest.template.json
  src/
    background/
    content/
    generated/bridge.ts
  static/
```

Rules:

- handwritten extension code lives under `apps/extension/src/`
- generated Rust-owned bridge bindings live under
  `apps/extension/src/generated/bridge.ts`
- generated files are committed so review, packaging, and offline local builds
  do not depend on ad hoc generation in the middle of packaging

## Rust-Owned Contract Export

Rust is the source of truth for Roger browser-bridge contracts.

### Contract source

Roger should keep bridge request/response envelopes in a small Rust-owned
contract module or crate with:

- `serde::{Serialize, Deserialize}` for Rust-side transport
- `ts_rs::TS` for TypeScript type generation

The extension must not own parallel handwritten copies of the bridge structs or
message enums.

### Export step

Roger should expose a dedicated export step such as:

```text
rr bridge export-contracts
```

That step is responsible for:

- generating `apps/extension/src/generated/bridge.ts`
- stamping the generated file from the Rust contract source
- failing clearly if export is attempted from an incompatible workspace state

### Drift detection

Contract drift must be caught locally and in CI.

Required checks:

- local hook or scripted verify step regenerates bridge bindings and fails if
  the checked-in output changes
- CI runs the same export verification and rejects drift
- extension packaging jobs depend on the export verification step rather than
  trusting checked-in generated files blindly

Minimum acceptable enforcement shape:

1. `rr bridge export-contracts` writes the generated TypeScript file
2. `rr bridge verify-contracts` regenerates in a temp location or checks for a
   clean git diff
3. pre-commit or pre-push hooks may call the verify command locally
4. CI fails if `verify-contracts` detects drift

JSON Schema generation is optional later tooling, not part of the `0.1.0`
runtime or packaging baseline.

## Roger-Owned Pack And Install Commands

Roger should own the extension and host packaging workflow instead of pushing
that complexity onto manual maintainer steps.

### Required command family

The minimum command surface for `0.1.0` is:

- `rr bridge export-contracts`
- `rr bridge verify-contracts`
- `rr bridge pack-extension`
- `rr extension setup`
- `rr extension doctor`
- `rr bridge uninstall`

Command roles:

- `export-contracts` generates Rust-owned TS bridge bindings
- `verify-contracts` enforces freshness and drift detection
- `pack-extension` builds the browser-installable extension artifact from the
  extension source tree and Roger-owned manifest rendering
- `extension setup` is the primary user-facing flow: it prepares the unpacked
  extension artifact, guides the one required manual browser load step, learns
  the extension id through Roger-owned discovery or extension self-registration,
  and then registers the Native Messaging host manifest for the current OS
  using the installed `rr` binary in host mode rather than a normal-path
  separate `rr-bridge` binary
- `extension doctor` verifies that the extension package, extension identity,
  local host registration, and bridge reachability are present and truthful
- `uninstall` removes Roger-owned bridge registration state for the current OS

Rules:

- `pack-extension` packages a local installable artifact; browser-store
  submission remains outside the `0.1.0` contract
- `extension setup` and `uninstall` must not silently install or update the
  browser extension itself; the browser load/enable step remains explicit
- the normal user-facing flow must not require a manually typed extension id or
  a user-facing separate bridge-host binary path
- explicit bridge-install flags such as `--extension-id` and `--bridge-binary`
  are repair/development levers only and must not appear in normal onboarding
  steps
- the base one-line Roger install flow remains local-product-only and does not
  imply bridge registration or extension packaging

### Guided setup and doctor contract (`rr-ivjk.1`)

Normal-path contract:

1. user runs `rr extension setup [--browser edge|chrome|brave]`
2. Roger prepares the unpacked extension artifact and prints the one required
   manual browser action (load/enable that unpacked artifact)
3. Roger learns extension identity through discovery or extension-side
   self-registration; the normal user path must not ask for manually typed
   extension ids
4. Roger registers Native Messaging launch assets for the current OS against
   the installed `rr` binary in host mode (no normal-path separate `rr-bridge`
   binary workflow)
5. Roger runs the same bounded checks exposed by `rr extension doctor` and
   reports readiness truthfully

Doctor contract:

- `rr extension doctor` verifies package presence, discovered extension
  identity, host registration linkage, and bridge reachability claims
- doctor output must fail closed if any check is missing or inconsistent
- doctor output must include bounded repair guidance: rerun `rr extension setup`
  for normal-path recovery, and reserve low-level bridge commands for
  development/repair workflows (including explicit `rr bridge install`
  overrides only when guided setup cannot recover cleanly)

## Artifact Ownership

The bridge and extension lane is split into separate artifact classes so Roger
can ship the local product honestly even when the browser lane is not shipped.

### Artifact classes

`0.1.0` bridge-related artifacts are:

- bridge contract export output: generated `bridge.ts` checked into source
- bridge registration bundle: Native Messaging manifest templates plus
  OS-specific install/uninstall helpers
- extension sideload package: browser-installable packaged extension built from
  the shared source tree

### Release ownership split

Release-job ownership is:

- the unified `release` workflow is the operator-facing release lane
- `build-core` builds Rust companion archives only
- `package-bridge` owns Native Messaging manifest rendering, OS-specific
  registration helpers, and platform bridge registration bundles
- `package-extension` owns extension packaging from the shared source tree
  after contract verification passes
- `verify-release-assets` recomputes checksums, verifies package contents, and
  confirms the correct release jobs were produced
- `publish-release` publishes only the artifact classes that actually passed
  verification

### Publication rules

- Roger must not imply browser-launch support for a target unless the matching
  bridge registration bundle and extension package were actually shipped for
  that release
- extension packaging is a separate optional release lane, not an invisible
  side effect of the core companion release
- local installation and release notes must state plainly whether a tag ships
  core-only, core-plus-bridge, or core-plus-bridge-plus-extension artifacts

## Supported `0.1.0` Targets

This contract inherits the release matrix target floor:

- core companion archives: macOS `arm64`, macOS `x86_64`, Windows `x86_64`,
  Windows `arm64`, Linux `x86_64`
- bridge registration bundles: macOS, Windows, Linux where Roger claims
  browser-launch support
- extension packages: one source base targeting Chrome, Brave, and Edge

Linux may remain weaker in ergonomics than macOS or Windows, but that must be
described honestly in release notes rather than hidden in packaging language.

## Acceptance Summary For `rr-007.1`

This document freezes the three acceptance points required by the bead:

1. Minimal tooling: `typescript` plus `chrome-types` plus Roger-owned scripts,
   with zero browser-runtime npm dependencies and no bundler by default.
2. Contract export: Rust owns the bridge schema, exports generated TS bindings
   into the extension tree, and drift is enforced by local verification plus
   CI.
3. Release ownership: Roger-owned pack/install commands and explicit release
   jobs own extension packaging, host-manifest registration assets, checksums,
   and truthful publication boundaries across supported `0.1.0` targets.
