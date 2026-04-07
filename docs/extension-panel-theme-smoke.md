# Extension Panel Theme Readability Smoke

This checklist is the canonical smoke path for validating Roger extension panel
readability on live GitHub PR pages across light and dark themes.

## Scope

- panel surface/background/text readability
- action button readability (rest and disabled/busy)
- badge readability (attention/fallback states)
- status text readability for idle and fallback-only messaging

## Automated Guard (Fast)

Run:

```sh
node --test apps/extension/src/background.test.js apps/extension/src/content/main.test.js
```

Required assertions from `apps/extension/src/content/main.test.js`:

- anchor selection + fallback mount behavior stays deterministic
- mode classing toggles correctly (`inline` vs `floating`)
- status classing toggles correctly for readable ok/error states via `setStatus`

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
