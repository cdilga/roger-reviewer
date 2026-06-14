# Extension Panel Theme Readability Smoke

This checklist is the canonical smoke path for validating Roger extension panel
readability on live GitHub PR pages across light and dark themes.

## Scope

- panel surface/background/text readability
- action button readability (rest and disabled/busy)
- badge readability (attention/fallback states)
- status text readability for idle and fallback-only messaging

## Host-Theme Mapping Contract (Roger-owned)

The injected panel does not guess at colours; it remaps a small set of
Roger-owned tokens onto GitHub/Primer host variables, and restates them for
dark across **every** way GitHub signals an active dark theme:

- `:root[data-color-mode="dark"]` — GitHub's most common explicit case (on `html`)
- `[data-color-mode="dark"]` on any ancestor — body/wrapper placement (real
  GitHub and the render harness both emit the signal on `<body>`)
- `#roger-reviewer-panel[data-color-mode="dark"]` — signal on the panel itself
- `data-color-mode="auto"` + `@media (prefers-color-scheme: dark)` — GitHub
  "auto" theme
- bare `@media (prefers-color-scheme: dark)` on `:root:not([data-color-mode="light"])`
  — degraded host with no explicit mode attribute

Dark surfaces resolve from canonical Primer dark sources
(`--overlay-bgColor` / `--bgColor-default` / `--bgColor-muted` /
`--fgColor-default` / `--fgColor-muted` / `--borderColor-emphasis`). The
metallic Roger sheen flows through `--rr-panel-metal-tint` (white in light, a
restrained `--fgColor-default` overlay in dark) instead of hardcoded `white`,
so buttons/title/chip no longer wash out in dark.

The popup (a standalone document with no GitHub host) follows the OS/browser
preference via `color-scheme: light dark` plus a `prefers-color-scheme: dark`
remap of its `--rr-popup-*` tokens.

### Proof this was host-theme drift, not just contrast

Before the fix the only dark override was `:root[data-color-mode="dark"]`. Real
GitHub (and the render harness) put `data-color-mode` on `<body>`, not `:root`,
so the override never matched and the panel stayed on its light treatment under
a dark host — a faded title and washed buttons. The render harness `<body>`
carries `data-color-mode="dark"`; the `panel theme mapping covers every GitHub
dark signal …` harness probes assert all five selector forms now exist in the
injected sheet, in both the light and dark host probes.

## Automated Guard (Fast)

Run (the `src` suites use Node's directory recursion; the render harness needs
Node >= 20 for jsdom 29):

```sh
node --test apps/extension/src/background.test.js apps/extension/src/content/main.test.js
node --test apps/extension/src/popup/index.test.js apps/extension/src/popup/layout_redesign.test.js
( cd apps/extension/testing && node --test ./popup_render_harness.test.cjs )
```

Required assertions from `apps/extension/src/content/main.test.js`:

- anchor selection + fallback mount behavior stays deterministic
- mode classing toggles correctly (`inline` vs `floating`)
- status classing toggles correctly for readable ok/error states via `setStatus`
- the injected stylesheet maps the dark host theme across every GitHub theme
  signal and binds Roger surfaces to dark Primer variables
- the metallic sheen routes through the theme-aware `--rr-panel-metal-tint`
  token (no literal-white surface/button mixes)

Required assertions from the popup suites and render harness:

- popup opts into `color-scheme: light dark` with a `prefers-color-scheme: dark`
  token remap; card/button surfaces route through `--rr-popup-*` tokens
- the shared `.rr-brand-chip` ships a dark-theme remap in `roger-identity.css`
- the panel render harness mounts and asserts the full dark-signal matrix in
  BOTH the `pr-light-rail` and `pr-dark-rail` host probes

## Live GitHub Smoke (Manual)

Target page used in this cycle:

- `https://github.com/cdilga/roger-reviewer/pull/1`

### Light Theme Probe

1. Open the PR page.
2. Ensure browser color scheme is light.
3. Probe token values used by panel/button/badge states.

Observed (current cycle):

- `panelBg=#f6f8fa`
- `panelFg=#1f2328`
- `buttonBg=#f6f8fa`
- `buttonFg=#25292e`
- `disabledBg=#eff2f5`
- `disabledFg=#59636e`
- `fallbackBg=#cf222e`
- `fallbackFg=#fff`
- sampled contrast ratios:
  - idle (`panelFg` vs `panelBg`): `14.84`
  - busy (`disabledFg` vs `disabledBg`): `5.44`
  - fallback (`fallbackFg` vs `fallbackBg`): `5.36`

### Dark Theme Probe

1. Keep the same PR page open.
2. Switch browser color scheme to dark.
3. Re-run the same token probe.

Observed (current cycle):

- `panelBg=#151b23`
- `panelFg=#f0f6fc`
- `buttonBg=#212830`
- `buttonFg=#f0f6fc`
- `disabledBg=#212830`
- `disabledFg=#9198a1`
- `fallbackBg=#da3633`
- `fallbackFg=#fff`
- sampled contrast ratios:
  - idle (`panelFg` vs `panelBg`): `15.91`
  - busy (`disabledFg` vs `disabledBg`): `5.11`
  - fallback (`fallbackFg` vs `fallbackBg`): `4.61`

## Pass Criteria

- all probed tokens resolve (no missing variables)
- light and dark probes both show readable foreground/background pairings;
  in this run all sampled states remained above `4.5:1`
- status ok/error classes and disabled/busy class behavior remain covered by
  automated tests
