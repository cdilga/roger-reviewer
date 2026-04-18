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

## Live Sacrificial-PR Operator-Stability Rehearsal (`rr-6iah.8`)

This section tracks the separate live-browser lane for sacrificial PR rehearsal.
It is intentionally outside deterministic `E2E-05` closeout.

### Scope and Safety Rules

- use a sacrificial GitHub account and a resettable PR target (never maintainer-owned PRs)
- do not post findings/comments back to GitHub in this lane
- use an isolated browser profile for the rehearsal
- keep all host-manifest changes reversible (backup then restore)
- retain machine-readable artifacts for every run, including blocked outcomes

### Credential and Cleanup Contract

1. Credentials:
- use only sacrificial credentials in the isolated browser profile
- do not reuse maintainer browser profiles or long-lived authenticated sessions
- if prompted for native-host permission, approve only inside the sacrificial profile

2. Cleanup:
- kill all rehearsal browser processes tied to the isolated profile
- restore or remove any temporary native-host manifest written in the real browser
  host path
- remove temporary profile/store directories if they contain sacrificial session data

3. Artifact retention:
- keep `rr extension setup/doctor` robot JSON outputs
- keep extension package checks (`asset-manifest.json`, icon presence list)
- keep PR-page launch probe output (panel presence, click method, status text)
- keep session/status robot outputs after launch attempt
- keep browser launch logs and extension-id discovery records

### Edge Rehearsal Command Packet (2026-04-17)

1. Package extension and verify icon assets are present:
```sh
./target/debug/rr bridge pack-extension --output-dir /tmp/<run>/pack --robot
ls -la /tmp/<run>/pack/roger-extension-unpacked/assets
```

2. Seed isolated profile extension identity and run setup/doctor:
```sh
RR_STORE_ROOT=/tmp/<run>/store \
RR_EXTENSION_PROFILE_ROOT=/tmp/<run>/edge-profile \
./target/debug/rr extension setup --browser edge --robot

RR_STORE_ROOT=/tmp/<run>/store \
RR_EXTENSION_PROFILE_ROOT=/tmp/<run>/edge-profile \
./target/debug/rr extension doctor --browser edge --robot
```

Use the real per-user native-host path for live browser rehearsal. Do not pass a
temporary `--install-root` here: that only validates a synthetic manifest tree
and can diverge from the host manifest path the real Edge process actually
reads.

3. Launch real PR page with unpacked extension and trigger trusted Start click
   (CDP mouse event, not synthetic `element.click()`), then capture sessions/status.

### Execution Ledger (2026-04-17, Edge)

- symptom treated as real: the Edge lane reported PNG load failure
  (`Could not load icon assets/icon-16.png`).
- repaired packaging evidence: unpacked artifact now contains
  `assets/icon-16.png`, `assets/icon-32.png`, `assets/icon-48.png`,
  `assets/icon-128.png`.
- real PR-page evidence captured at `https://github.com/rust-lang/rust/pull/155408`:
  panel present, Start action present, trusted click dispatched.
- current root-cause repro on this machine:
  - `RR_STORE_ROOT=/tmp/rr-6iah8-edge-run-Qvz1n3/store RR_EXTENSION_PROFILE_ROOT=/tmp/rr-6iah8-edge-run-Qvz1n3/edge-profile ./target/debug/rr extension doctor --browser edge --robot`
    fails closed with `reason_code=native_host_origin_mismatch` against the real
    Edge host path at
    `~/Library/Application Support/Microsoft Edge/NativeMessagingHosts/com.roger_reviewer.bridge.json`
  - the same doctor command with
    `--install-root /tmp/rr-6iah8-edge-run-Qvz1n3/install-root` reports
    `outcome=complete`, proving the temp-root rehearsal was validating a
    synthetic manifest tree that the live Edge process does not read
  - the isolated-profile Roger extension id visible in the live Edge profile is
    `nlomlfojaifagjfhdoikiemchiodldnd`, while the stale real Edge user-level host
    manifest still allowed
    `chrome-extension://abcdefghijklmnopabcdefghijklmnop/`
- repo-side truth fix landed after this first blocked run:
  - the unpacked Roger extension now carries a committed public key, which
    gives the packaged artifact a deterministic browser extension id
    (`djbjigobohmlljboggckmhhnoeldinlp`)
  - `rr extension setup` waits for browser-profile discovery first, but if that
    stronger signal does not arrive it now derives the same deterministic id
    from the packaged manifest key and writes the real Native Messaging host
    manifest with that origin instead of reusing stale store state
  - `rr extension doctor` still prefers `browser_profile_preferences` once the
    unpacked extension is visibly loaded, but `packaged_manifest_key` is now a
    truthful fallback; reruns should no longer rely on stale
    `extension_id_source=store_registry`

### Current Status

The runbook, credential/cleanup rules, and Edge operator-stability execution
artifacts are in place. The first live block is now reduced to one final
browser-policy rerun on the real per-user host path after the deterministic-id
fix. Roger can now prepare the correct allowed origin before the first live
launch attempt, but this bead still needs one fresh real-session launch
completion to prove that the browser reloads and honors the updated policy on
the sacrificial PR page.

### Next Operator Step For `forbidden`

When the live PR-page probe reports:

`Native Messaging unavailable; launch blocked. Access to the specified native messaging host is forbidden.`

use this exact recovery sequence before treating the browser as unrecoverable:

1. Re-run setup and doctor against the same isolated profile/store used for the
   rehearsal, without `--install-root`, so Roger refreshes the real Edge
   user-level host manifest:
```sh
RR_STORE_ROOT=/tmp/<run>/store \
RR_EXTENSION_PROFILE_ROOT=/tmp/<run>/edge-profile \
./target/debug/rr extension setup --browser edge --robot

RR_STORE_ROOT=/tmp/<run>/store \
RR_EXTENSION_PROFILE_ROOT=/tmp/<run>/edge-profile \
./target/debug/rr extension doctor --browser edge --robot
```
2. Confirm the doctor/setup output shows the expected extension identity from
   `browser_profile_preferences` or `packaged_manifest_key`, never stale
   `store_registry`, and that the real Edge host manifest under
   `~/Library/Application Support/Microsoft Edge/NativeMessagingHosts/`
   now allows the same `chrome-extension://<id>/` origin. On a fresh rerun
   before the browser preference file updates, `packaged_manifest_key` is an
   acceptable first-pass source as long as the id is
   `djbjigobohmlljboggckmhhnoeldinlp`.
3. Fully quit the rehearsal browser process, then relaunch Edge with the same
   isolated profile and the same unpacked extension path so the browser reloads
   native-host policy from disk.
4. Re-open the sacrificial PR page and trigger one trusted Start click again.

Expected result after a successful browser-policy refresh:

- the extension dispatches through Native Messaging into the local Roger bridge
- Roger materializes or reuses the correct local session for that PR
- follow-up `rr sessions --robot` / `rr status --session <id> --robot` output
  confirms the launch bound to the expected session instead of failing before
  wrapper execution

Observed blocking result before this repo-side/runbook correction:

- the browser rejected host access before wrapper execution because the live
  Edge process was still reading a stale home-path manifest whose
  `allowed_origins` did not match the rehearsal extension id

## Pass Criteria

- `scripts/swarm/validate_extension_entry_placements.sh` exits `0`
- placement precedence (header -> rail -> modal) assertions pass
- popup action-set and non-PR gating assertions pass
- Native Messaging fail-closed assertions pass
- supported-browser launch metadata files are present
