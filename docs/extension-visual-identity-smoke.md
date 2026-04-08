# Extension Visual Identity Smoke

This runbook captures truthful smoke evidence for the extension branding slice
(`rr-vsr2`) without widening Roger behavior claims.

Identity rationale and narrowed alternatives are tracked in:

- `docs/extension-identity-direction.md`

## Scope

- popup shell renders the new Roger mark and wordmark assets
- shared identity token sheet is imported by popup HTML
- popup still preserves manual-backup messaging and the bounded 4-action set
- identity assets are present as static artifacts for future in-page reuse

## Automated Command

Run:

```sh
node --test \
  apps/extension/src/popup/index.test.js \
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
4. Verify Start/Resume/Findings/Refresh remain available.
5. Open popup on a non-PR tab and confirm manual-backup guidance still appears.

## Pass Criteria

- automated command exits `0`
- popup header displays brand shell (mark + wordmark)
- manual-backup copy and bounded launch behavior remain intact

## Latest Automated Evidence (2026-04-08)

- `node --test apps/extension/src/popup/index.test.js apps/extension/src/popup/main.test.js`
  - PASS
  - confirmed: brand-shell markup, mark/wordmark asset references, identity token
    import, popup manual-backup messaging, and bounded 4-action routing
- `scripts/swarm/validate_extension_entry_placements.sh`
  - PASS
  - confirmed: header/rail/modal placement coverage and Native Messaging
    fail-closed launch guidance remain intact after branding changes

Observed coverage in this cycle:

- automated popup + extension seam tests executed locally
- manual browser pass is still required for release-lane proof when visuals or
  selector seams change
