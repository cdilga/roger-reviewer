# Extension Entry UX Smoke

This runbook is the canonical smoke path for validating Roger PR-page entry
placement precedence and bounded fallback behavior.

## Scope

- header seam placement is preferred when GitHub PR header actions are present
- right-rail placement above reviewers is used when header seams are unavailable
- modal fallback is used when neither header nor rail seams are usable
- popup remains a manual backup surface (PR-aware, non-PR fail-closed guidance)
- popup actions preserve the bounded launch set:
  `start_review`, `resume_review`, `show_findings`
- launch path is Native Messaging only; when unavailable, launch fails closed
  with setup guidance and does not open `roger://...`

## Automated Validation Command

Run:

```sh
scripts/swarm/validate_extension_entry_placements.sh
```

This command executes focused placement/popup/launch tests and asserts:

- header/rail/modal placement contract coverage exists in
  `apps/extension/src/content/main.test.js`
- popup action routing covers all 3 documented launch actions in
  `apps/extension/src/popup/main.test.js`
- Native Messaging fail-closed launch behavior is exercised in
  `apps/extension/src/background.launch.test.js`
- supported-browser launch smoke suite metadata files exist:
  - `tests/suites/smoke_browser_launch_chrome.toml`
  - `tests/suites/smoke_browser_launch_brave.toml`
  - `tests/suites/smoke_browser_launch_edge.toml`

## Scenario Matrix

- Header host available:
  `resolvePanelPlacement` prefers inline/header seam (`content/main.test.js`)
- Header missing, rail available:
  `resolvePanelPlacement` mounts rail pane above reviewers (`content/main.test.js`)
- Header + rail unavailable:
  `resolvePanelPlacement` returns modal fallback (`content/main.test.js`)
- Popup on PR tab:
  Start/Resume/Findings routes are enabled and dispatched (`popup/main.test.js`)
- Popup on non-PR tab:
  guidance mode is `non_pr` and launch controls are disabled (`popup/main.test.js`)
- Native Messaging unavailable:
  launch fails closed with setup/doctor guidance (`background.launch.test.js`)

## Supported-Browser Manual Follow-On (Release/Claim Lane)

When support wording, selector seams, or popup launch UX changes, run a manual
probe in at least one of Edge, Chrome, or Brave:

1. Open a GitHub PR tab and verify header/rail/modal host behavior matches the
   current seam availability.
2. In popup on a PR tab, verify Start/Resume/Findings remain enabled.
3. With Native Messaging host uninstalled or misconfigured, verify launch is
   blocked with setup guidance and no custom URL tab opens.
4. In popup on a non-PR tab, verify non-PR guidance and disabled launch actions.

Record browser, URL shape, seam condition, and observed mode in bead notes.

## Pass Criteria

- `scripts/swarm/validate_extension_entry_placements.sh` exits `0`
- placement precedence (header -> rail -> modal) assertions pass
- popup action-set and non-PR gating assertions pass
- Native Messaging fail-closed assertions pass
- supported-browser launch metadata files are present
