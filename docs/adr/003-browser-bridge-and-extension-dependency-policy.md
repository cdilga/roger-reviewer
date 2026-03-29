# ADR 003: Browser Bridge and Extension Dependency Policy

- Status: accepted
- Date: 2026-03-29

## Context

Roger wants a first-class GitHub PR entrypoint without making the extension the
source of truth or reintroducing a daemon. The extension is also the main JS/TS
surface in the system.

The user direction is:

- keep dependencies low because vulnerability and churn surface matter
- TypeScript is acceptable if it materially improves contract safety and
  maintainability
- think explicitly about both the local companion binary and the extension
  package
- ensure release targets exist for Windows, macOS, and major architectures

The main bridge options are:

- custom URL protocol for launch-tier behavior
- Native Messaging for richer companion-tier behavior

## Decision

Recommended direction:

- treat the extension as a low-dependency JS/TS exception in an otherwise
  Rust-first system
- use browser APIs, direct DOM integration, and small hand-rolled TS/JS by
  default
- reject framework-heavy extension stacks unless a capability proves they are
  necessary
- use Native Messaging as the primary v1 bridge
- keep custom URL launch as a convenience and fallback path, not as the only
  serious bridge

Rationale:

- the planned v1 extension behavior already includes bounded state readback,
  active-session lookup, same-PR instance disambiguation, and local focus
  actions
- those are companion-tier behaviors, not launch-tier-only behaviors
- a custom URL scheme can hand work off to Roger, but it does not provide the
  bidirectional request/response path needed for status, targeting, or richer
  local coordination

Recommended product shape:

- the Rust `rr` binary should be the first Native Messaging host in a dedicated
  host mode such as `rr bridge host`
- keep the extension thin: service worker plus small content-script/page hooks,
  with Native Messaging requests routed into Roger-owned local commands and
  bounded state lookups
- only split out a separate helper binary later if browser-host registration or
  packaging realities force that change
- provide Roger-owned local install helpers such as `rr bridge install` and
  `rr bridge pack-extension` so host-manifest registration and packed-extension
  setup are reproducible without a store-publishing workflow

### Dependency policy split

Roger should distinguish extension runtime dependencies from build-time tooling
dependencies.

Runtime policy:

- browser runtime dependencies should be zero by default
- any shipped runtime npm dependency needs explicit written justification
- prefer Roger-owned code plus browser APIs over helper libraries

Build-time policy:

- a small TypeScript-first toolchain is acceptable if it materially improves
  message contracts, manifest correctness, packaging consistency, or browser
  compatibility
- keep the build toolchain intentionally small and pinned
- prefer tools that do one narrow job well over frontend framework ecosystems

Selected `0.1.0` build stance:

- use `typescript` as the compiler and typechecker
- use the official `chrome-types` definitions as the browser API typing source
- use Roger-owned build, pack, and install scripts for manifest rendering,
  artifact creation, and local host registration
- do not adopt a framework-led bundler stack
- avoid bundling by default; if extension entrypoint constraints later prove a
  bundler is materially worth its cost, allow exactly one narrow bundler
  dependency behind the same Roger-owned scripts

### Contract discipline

The browser bridge should use explicitly versioned Roger-owned message
contracts.

Recommended approach:

- define request/response envelopes once
- make Rust the source of truth for bridge contracts
- derive `serde::{Serialize, Deserialize}` plus `ts_rs::TS` on the same bridge
  structs and enums
- generate the TypeScript bridge types into the extension tree from Rust during
  a dedicated export step
- enforce freshness in local hooks and CI so generated TS never drifts from the
  Rust contract
- prefer compile-time contract checking over convenience wrappers or hand-kept
  parallel types

Recommended workflow:

- keep Roger-owned bridge types in a small Rust contract crate or module
- export `bindings/bridge.ts` or equivalent from Rust with `ts-rs`
- commit the generated TS so packed-extension builds stay simple and reviewable
- fail CI if regeneration changes the checked-in output
- add a lightweight pre-commit or pre-push hook so drift is caught before merge

Optional later addition:

- if Roger later needs machine-readable schema fixtures for external tooling or
  compatibility snapshots, generate JSON Schema from the same Rust types with
  `schemars`
- do not make JSON Schema generation a v1 runtime requirement

### Companion binary requirements

The local companion surface may be the `rr` binary in a dedicated mode or a
small sibling host binary, but it must satisfy the same requirements.

Required behavior:

- start quickly and run on demand rather than acting as a daemon
- support one-shot launch flows and request/response companion flows
- accept structured Roger-owned bridge messages
- emit stable machine-readable responses and exit codes
- remain usable as a local CLI-owned component, not a separate application

Platform packaging requirements:

- macOS: build and package for `arm64` and `x86_64`
- Windows: build and package for `x86_64` and `arm64`
- Linux: build and package at least for `x86_64`; add `arm64` where the release
  flow can support it cleanly
- produce checksums and versioned artifacts for each target
- support the platform-specific registration/install steps needed for custom URL
  handlers and Native Messaging manifests
- treat local packed-extension install plus local host-manifest registration as
  the v1 packaging baseline; Chrome Web Store publication is not a product
  requirement

### Extension package requirements

Required behavior:

- support Chrome, Brave, and Edge from one source base
- keep the in-browser surface thin and PR-local
- use strongly typed bridge messages and manifest/build validation
- avoid framework-heavy UI layers unless a later capability truly needs them

Packaging requirements:

- allow a small TS transpile/bundle step if it improves typed contracts and
  repeatable packaging
- generate browser-installable extension artifacts from the same source tree
- keep dependency count and vulnerability scan output visible in release review
- separate source/runtime requirements from store-packaging concerns
- optimize for packed local installation rather than store submission

### Release and DevOps implications

Multi-platform artifact generation is part of the release/devops surface, even
if the first owner is not decided yet.

The release flow should eventually own:

- companion-binary builds for the supported target matrix
- extension artifact generation for Chrome/Brave/Edge
- checksums, version stamping, and artifact publication
- manifest placement guidance and local install flows for Native Messaging hosts
- signing or notarization only if later distribution requirements justify it;
  it is not a gate for local packed-extension use

This may deserve its own later ADR or bead if the packaging surface grows beyond
the bridge/runtime decision itself.

## Consequences

- the extension stays thin and PR-local
- bridge realism is evaluated separately from extension UI polish
- dependency count and vulnerability surface become explicit acceptance criteria
  for extension work
- a small TS toolchain is allowed when it pays for stronger contracts
- release engineering for companion binaries and extension artifacts becomes a
  first-class planning concern rather than an afterthought

## Follow-up

- run the bridge spike and document its result
- implement the chosen contract export, pack, and host-install flow behind
  Roger-owned scripts or commands
- add a release/devops follow-up for multi-arch companion builds and extension
  packaging
