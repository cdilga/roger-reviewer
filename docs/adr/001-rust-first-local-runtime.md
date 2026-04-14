# ADR 001: Rust-First Local Runtime

- Status: accepted
- Date: 2026-03-29

## Context

Roger's accepted local-runtime direction keeps the TUI and core ownership in
Rust. `FrankenTUI` is the current TUI dependency, which reinforces that
direction without becoming the product model. Search is also Rust-native.
Session orchestration, storage, local bridge behavior, and multi-instance
runtime management all live on the same side of the system.

The remaining question was whether Roger should split its local runtime across
Rust and TypeScript or treat Rust as the default for local ownership.

The user explicitly decided:

- favor Rust unless a platform constraint clearly justifies another language
- the browser extension is an acceptable JavaScript/TypeScript exception
- dependency minimization matters, especially in the JS/TS ecosystem

## Decision

Roger adopts a Rust-first local runtime.

Rules:

- the TUI is Rust
- the `rr` CLI should default to Rust
- app-core, storage, search, and local orchestration should default to Rust
- harness adapters should sit behind Roger-owned contracts regardless of
  provider
- the browser extension is the main expected JS/TS exception because it is
  browser-native

## Consequences

- the TUI/app-core protocol is now an intra-Rust runtime boundary first, not an
  assumed Rust-to-TypeScript boundary
- search, storage, and orchestration can share more code and dependency policy
- the remaining local-runtime design question is ownership and module boundaries
  inside Rust, not language split ideology
- if a later non-browser component wants to use JS/TS, it needs a specific
  platform or capability justification

## Follow-up

- define the stable external envelope and background-task contracts between TUI,
  CLI, bridge, and app-core
- keep the initial implementation as one primary `rr` binary with internal
  modes unless a later platform constraint justifies a helper executable
