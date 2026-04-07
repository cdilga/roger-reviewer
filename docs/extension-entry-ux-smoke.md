# Extension Entry UX Smoke

This runbook is the canonical smoke path for validating Roger's extension entry
UX across inline PR placement and browser-action popup fallback behavior.

## Scope

- inline mount uses GitHub PR action seam when present
- floating fallback remains deterministic when inline seam is unavailable
- popup mode detects PR vs non-PR tabs
- popup routes bounded actions (`start`, `resume`, `findings`, `refresh`)
- launch path is Native Messaging only; when Native Messaging is unavailable,
  launch fails closed with setup guidance and does not open `roger://...`

## Automated Smoke Command

Run:

```sh
scripts/swarm/smoke_extension_entry_ux.sh
```

This executes:

- `apps/extension/src/content/main.test.js`
- `apps/extension/src/popup/main.test.js`
- `apps/extension/src/background.test.js`
- `apps/extension/src/background.launch.test.js`

and verifies supported-browser launch smoke suite metadata exists for:

- `smoke_browser_launch_chrome`
- `smoke_browser_launch_brave`
- `smoke_browser_launch_edge`

## Supported-Browser Manual Follow-On (Release/Claim Lane)

When support wording, selector seams, or popup launch UX changes, run a manual
supported-browser probe in at least one of Edge, Chrome, or Brave:

1. Open a GitHub PR tab and click the extension action popup.
2. Confirm Start/Resume/Findings/Refresh buttons are enabled and dispatch.
3. With Native Messaging host uninstalled or misconfigured, click each action
   and confirm launch is blocked with setup guidance (no custom URL tab opens).
4. Open a non-PR tab and click the popup.
5. Confirm non-PR guidance appears and action buttons are disabled.

Record browser, URL shape, and observed behavior in bead notes for release-lane
traceability.

## Pass Criteria

- smoke script exits `0`
- inline and floating placement tests pass
- popup PR/non-PR context and routing tests pass
- launch fail-closed tests pass when Native Messaging is unavailable
- supported-browser smoke suite metadata files are present

## Known Caveat

Inline mounting depends on GitHub DOM seams (for example
`prc-PageHeader-Actions-*`). If GitHub changes those selectors, runtime
behavior may degrade to floating fallback until selectors are updated.
