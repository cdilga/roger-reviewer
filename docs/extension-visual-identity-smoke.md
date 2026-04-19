# Extension Visual Identity Smoke

This runbook captures truthful smoke evidence for the extension branding slice
(`rr-vsr2`) without widening Roger behavior claims.

Identity rationale and narrowed alternatives are tracked in:

- `docs/extension-identity-direction.md`

## Scope

- popup shell renders the new Roger mark and wordmark assets
- shared identity token sheet is imported by popup HTML
- popup preserves manual-backup messaging and the bounded 3-action set with
  revised hierarchy/copy:
  `Start Review in Roger`, `Resume Existing Review`, `View Findings`
- popup info affordance remains details-based and discloses build/fallback copy
  without restoring the old inline build/version row
- identity assets are present as static artifacts for future in-page reuse

## Automated Command

Run:

```sh
node --test \
  apps/extension/src/popup/index.test.js \
  apps/extension/src/popup/layout_redesign.test.js \
  apps/extension/src/popup/main.test.js
```

Companion bounded-behavior smoke (recommended when popup copy/styles change):

```sh
scripts/swarm/validate_extension_entry_placements.sh
```

## Manual Follow-On

In one supported browser (Chrome, Brave, or Edge):

1. Load the unpacked extension.
2. Open the popup on a GitHub PR tab.
3. Verify mark + wordmark render in the popup header.
4. Verify action copy/hierarchy remains:
   Start Review in Roger, Resume Existing Review, View Findings.
5. Expand the info affordance and verify build/fallback details render.
6. Open popup on a non-PR tab and confirm manual-backup guidance still appears.

## Pass Criteria

- automated command exits `0`
- popup header displays brand shell (mark + wordmark)
- action copy/hierarchy and info affordance behavior remain intact
- manual-backup copy and bounded launch behavior remain intact

## Latest Validation Evidence (2026-04-19)

- `node --test apps/extension/src/popup/index.test.js apps/extension/src/popup/layout_redesign.test.js apps/extension/src/popup/main.test.js`
  - PASS
  - confirmed: brand-shell markup, walkie-talkie asset references, details-based
    info affordance, revised action hierarchy/copy, PR-context-aware launch
    dispatch routing, and bounded manual-backup messaging
- `scripts/swarm/validate_extension_entry_placements.sh`
  - PASS
  - confirmed: header/rail/modal placement coverage and Native Messaging
    fail-closed launch guidance remain intact after popup redesign changes
- Supported-browser popup smoke probe (Edge):
  - artifact root:
    `out/operator-stability/rr-22ak.4-popup-smoke-20260419T055246Z`
  - command packet:
    - `./target/debug/rr init --robot`
    - `./target/debug/rr bridge pack-extension --output-dir <run>/pack --robot`
    - `RR_EXTENSION_PROFILE_ROOT=<run>/edge-profile ./target/debug/rr extension setup --browser edge --robot`
    - `RR_EXTENSION_PROFILE_ROOT=<run>/edge-profile ./target/debug/rr extension doctor --browser edge --robot`
    - `node <run>/edge_popup_probe.js`
  - result:
    `edge_popup_probe.json` reports `ok=true`, expected action labels,
    `brand_shell_found=true`, and info affordance disclosure transition from
    `"Build and fallback details"` to `"Hide Info"`

Observed coverage in this cycle:

- automated popup + extension seam tests executed locally
- one supported-browser popup smoke probe executed in Edge and captured under
  the artifact root above
