# Postmortem — Edge panel native-messaging silently no-ops

Two user-facing failures shipped past green test suites on Edge, both with the
same root mechanism and both invisible to our contract/jsdom layers:

- `rr-s8wx` — the content→background→native **status relay** never settled, so
  the panel degraded to launch-only.
- `rr-edge-launch-buttons-oneshot-sendnativemessage-...` — the panel **launch
  buttons** (Start/Resume) silently did nothing when clicked.

Mechanism: on Edge's MV3 **module service worker**, a one-shot
`chrome.runtime.sendNativeMessage` request is torn down with the worker before
the native host's reply callback fires, so the round trip never settles.

## 5 Whys

1. **Why did the launch buttons do nothing?** The click → background worker →
   `dispatchNative()` used one-shot `chrome.runtime.sendNativeMessage`; Edge's
   MV3 module service worker was torn down before the reply callback fired, so
   the launch never settled (flaky — it worked only when the worker was warm).
2. **Why was launch on the fragile call when status wasn't?** Fixing `rr-s8wx`
   switched only the **status probe** to a long-lived `connectNative` Port. We
   fixed the one reported call site and did not audit the **sibling** native
   dispatch (launch) that had the identical pattern.
3. **Why wasn't the sibling call site audited?** There was no systematic step
   (grep / lint / checklist) to find every native-dispatch call site after we
   learned the one-shot pattern is unsafe in an MV3 background, and no test that
   would fail on the bad pattern.
4. **Why did no test catch it?** The launch path's only coverage was
   `handleLaunchMessage` contract tests that **stub the transport itself**
   (`sendNativeMessage`/`dispatchNative`). A test that mocks the exact mechanism
   that breaks can never observe the break. No test drove a real click through a
   real worker to a real native host.
5. **Why was there no real-click test?** The testing methodology treated
   automated E2E as rare/expensive and blessed seeded-contract coverage as
   sufficient, with no category for **live interaction surfaces** — browser UI
   whose behavior only exists across the real click→worker→native boundary. So
   the most failure-prone surface had the weakest evidence.

## Root causes and fixes

- **RC1 — No "live interaction" test tier.** The panel was covered only by
  jsdom/contract tests that stub the boundary the bugs live in.
  **Fix:** committed live-interaction E2E harness that drives real clicks via
  CDP and asserts observable outcomes (settles, *visible* feedback, the
  worker-idle regression, `i` toggle):
  `scripts/extension/test_panel_interaction_e2e.sh` +
  `apps/extension/testing/panel_interaction_e2e.cjs`. Plus the Testing
  Philosophy "live interaction surfaces" rule in `AGENTS.md`.
- **RC2 — Symptom-scoped fixes without a sibling audit.** Fixing the status
  relay did not trigger an audit of the identical pattern on the launch path.
  **Fix:** when fixing a transport/boundary defect, audit **all** call sites of
  that mechanism before closing; record it in the bead.
- **RC3 — No guard against the known-unsafe pattern.** Nothing flagged one-shot
  `sendNativeMessage` in the MV3 background.
  **Fix:** static guard test
  `apps/extension/src/background.native_transport_guard.test.js` fails if the
  background worker calls `sendNativeMessage(` instead of using a
  `connectNative` Port.
- **RC4 — Stub-the-transport blind spot.** Contract tests mocked the exact
  failing mechanism.
  **Fix:** the "seeded-contract vs. genuine-live evidence" falsifiability rule
  in `AGENTS.md`, plus the live harness, make the real boundary observable.

## What "done" looks like for native-messaging / interaction work

- reply-expecting native dispatch in the background worker uses a long-lived
  `connectNative` Port (guarded by the static test)
- the live-interaction harness passes on Edge (the strict case), including the
  worker-idle regression
- the closing bead records the live run evidence, not just green stubs
