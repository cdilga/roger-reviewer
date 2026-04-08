# Extension Visual Identity Direction (`rr-vsr2.1`)

This note records the first bounded visual-identity decision for extension
surfaces while preserving GitHub-native ergonomics for action controls.

## Candidate directions considered

1. **Pure GitHub mirror**
   - Keep extension visuals effectively brandless and rely entirely on Primer
     defaults.
   - Rejected as primary identity: truthful but fails to create a reusable Roger
     signature across popup, in-page panel, and future non-GitHub surfaces.

2. **Neon AI badge**
   - High-saturation gradients and abstract geometry to signal "agentic review."
   - Rejected for `0.1.x`: too visually loud against GitHub PR chrome; risks
     reducing scan clarity for status and action labels.

3. **Chosen: Signal Ribbon mark**
   - Deep blue base with a cyan review-ribbon slash and compact `R` letterform.
   - Chosen for `0.1.x`: distinctive without fighting GitHub UI density; works
     at small icon sizes and scales to a wordmark lockup.

## Chosen identity assets

- Mark: `apps/extension/assets/roger-mark.svg`
- Wordmark lockup: `apps/extension/assets/roger-wordmark.svg`
- Raster icons for extension manifest:
  - `apps/extension/assets/icon-16.png`
  - `apps/extension/assets/icon-32.png`
  - `apps/extension/assets/icon-48.png`
  - `apps/extension/assets/icon-128.png`

## Surface guidance (current lane)

- Preserve GitHub-native button/seam patterns for launch actions on PR pages.
- Use Roger identity assets for extension-level chrome (toolbar icon, popup
  header lockups, and future branded cards) rather than replacing GitHub action
  affordances.
- Keep the color system bounded to accent/supportive moments; never obscure
  launch status or error guidance readability.

## Validation for this slice

- Manifest icon paths are wired and must resolve to real files.
- Use:
  - `bash scripts/swarm/validate_extension_identity_assets.sh`
