# Implementation Sources

This document saves external implementation-time sources that are likely to be
useful when Roger moves from planning into code.

It is not a substitute for the canonical Roger plan. It is a reference ledger
for APIs, packaging constraints, and workflow methodology that informed the
planning decisions.

## Browser Bridge and Extension

### Chrome Extensions messaging

- URL: <https://developer.chrome.com/docs/extensions/develop/concepts/messaging>
- Why it matters:
  - canonical Chrome message-passing reference for extension architecture
  - relevant for service worker, content script, and local companion request
    flow design

### Chrome Native Messaging

- URL: <https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging>
- Why it matters:
  - authoritative reference for Native Messaging host behavior
  - relevant for host manifest shape, stdio protocol, and extension permissions

### URL protocol handlers

- URL: <https://developer.chrome.com/docs/web-platform/best-practices/url-protocol-handler>
- Why it matters:
  - reference for custom protocol / launch-tier behavior
  - useful for the convenience/bootstrap path even though Native Messaging is
    the primary `0.1.0` bridge

### Microsoft Edge Native Messaging

- URL: <https://learn.microsoft.com/en-us/microsoft-edge/extensions/developer-guide/native-messaging>
- Why it matters:
  - authoritative Edge-specific Native Messaging reference
  - relevant for manifest placement, install flows, and Windows/macOS support

### Port Chrome extensions to Edge

- URL: <https://learn.microsoft.com/en-us/microsoft-edge/extensions/developer-guide/port-chrome-extension>
- Why it matters:
  - highlights cross-browser packaging and `allowed_origins` considerations for
    Native Messaging hosts

## Rust to TypeScript Contract Generation

### `ts-rs`

- URL: <https://github.com/Aleph-Alpha/ts-rs>
- Why it matters:
  - primary candidate for Rust-owned bridge contracts exported to TypeScript
  - aligns with the ADR 3 decision to avoid hand-maintained parallel types

### `schemars`

- URL: <https://docs.rs/schemars/latest/schemars/>
- Why it matters:
  - useful for optional JSON Schema snapshots from the same Rust types
  - not required for `0.1.0` runtime, but useful for tooling, compatibility
    fixtures, and external automation later

## Workflow Methodology

### Agent Flywheel complete guide

- URL: <https://agent-flywheel.com/complete-guide>
- Why it matters:
  - source for the plan-space / bead-space / code-space framing
  - useful for research-and-reimagine workflows, plan-to-beads transfer audits,
    fresh-eyes resets, and feedback-to-infrastructure loops
- Roger should adapt the methodology selectively rather than importing the
  whole flywheel stack as product architecture

## Claude Code Prior Art For Worktree And Setup Behavior

### Claude Code common workflows

- URL: <https://code.claude.com/docs/en/tutorials>
- Why it matters:
  - documents built-in `--worktree` behavior and cleanup semantics
  - explicitly states that new worktrees still need project-specific
    environment initialization
  - useful prior art for Roger's single-checkout-default plus explicit
    worktree-mode stance

### Claude Code hooks

- URL: <https://code.claude.com/docs/en/hooks>
- Why it matters:
  - documents `WorktreeCreate` and `WorktreeRemove` lifecycle hooks
  - documents `SessionStart` environment export through `CLAUDE_ENV_FILE`
  - strong prior art for Roger hookable worktree setup, session env injection,
    and cleanup contracts

### Claude Code settings

- URL: <https://code.claude.com/docs/en/settings>
- Why it matters:
  - documents project, local, user, and managed scope layering
  - useful prior art for Roger's additive config model and local override story

### Claude Code slash commands and skills

- URL: <https://code.claude.com/docs/en/slash-commands>
- Why it matters:
  - shows how project-visible reusable workflows can live in the repo rather
    than only in personal config
  - useful prior art for Roger repo-defined setup or verification automation

## Notes

- Re-verify browser docs close to implementation if browser API details become
  critical, because extension docs and browser packaging guidance change over
  time.
- Prefer these primary sources over blog posts when implementation decisions
  depend on browser, manifest, or protocol behavior.
