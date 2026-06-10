# Extension Surface Render Harness

Status: local DOM-backed extension harness for bead `rr-588y`.

This harness gives agents a repeatable local extension surface without
requiring a live browser session. It is now biased toward the in-page injected
Roger card because the current visual breakage is in the dark-mode GitHub panel
surface, not the toolbar popup.

Primary surface:

- dark GitHub-like PR host scaffold
- real injected panel DOM from `apps/extension/src/content/main.js`
- rendered rail or inline Roger card with computed style metadata

Secondary surface:

- real popup shell HTML from `apps/extension/src/popup/index.html`
- real popup script from `apps/extension/src/popup/main.js`
- kept for local backup-surface inspection, but not the default harness target

## What It Covers

- real injected-panel DOM against a dark GitHub-like host scaffold
- real popup DOM as a secondary surface
- shared Roger identity assets from `apps/extension/static/`
- real extension JS against mocked `chrome.runtime` boundaries
- named panel scenarios for:
  - PR rail card in dark mode
  - PR rail card with resume-primary status
  - PR rail card with findings-ready status
  - PR inline/header-control render in dark mode
- named popup scenarios for:
  - non-PR tab
  - PR tab with default launch hierarchy
  - PR tab with resume-primary status
  - PR tab with findings-ready status

The default scenario is `panel:pr-dark-rail`, which exercises the visually
problematic Roger PR-card render path for
`https://github.com/rust-lang/rust/pull/155408`.

## What It Does Not Prove

- true Edge/Chrome/Brave browser rendering or popup sizing
- real browser popup sizing, fonts, or rasterization fidelity
- GitHub page theme inheritance or live GitHub DOM interactions
- extension packaging, Native Messaging registration, or `rr extension setup`
- trusted click semantics inside Edge/Chrome/Brave
- live browser-only regressions such as first-run prompts or extension reload
  races

Use this harness as a fast local inspection aid before live browser validation,
not as a substitute for the real browser lanes.

## Install

One-time dependency install:

```sh
bun install --cwd apps/extension/testing
```

Fallback if you prefer npm:

```sh
npm install --prefix apps/extension/testing
```

The repo runner script performs the same install automatically if
`apps/extension/testing/node_modules` is missing.

## Run

Default injected-card render:

```sh
scripts/extension/run_popup_render_harness.sh
```

Render a specific panel scenario:

```sh
scripts/extension/run_popup_render_harness.sh --surface panel --scenario pr-dark-findings-ready
```

Render all built-in panel scenarios:

```sh
scripts/extension/run_popup_render_harness.sh --surface panel --all
```

Render popup as the secondary surface:

```sh
scripts/extension/run_popup_render_harness.sh --surface popup --scenario pr-idle
```

Emit machine-readable JSON to stdout:

```sh
scripts/extension/run_popup_render_harness.sh --surface panel --scenario pr-dark-rail --json
```

Generated artifacts land in `apps/extension/testing/artifacts/latest/` by
default:

- `<scenario>.rendered.html`
- `<scenario>.summary.json`

Pass `--output-dir <path>` to redirect the artifact root.

## Expected Agent Workflow

1. Run the harness for the injected-panel state you want to inspect.
2. Read the generated `*.summary.json` to confirm visible actions, labels,
   panel mode, mount slot, computed style metadata, and mocked bridge traffic.
3. Inspect `*.rendered.html` when you need the fully rendered DOM shell with
   the injected Roger card mounted into the dark GitHub-like host scaffold.
4. Use popup mode only when the toolbar backup surface itself is under review.
5. Use live browser lanes afterward for anything involving true extension
   loading, GitHub seams, or Native Messaging setup.
