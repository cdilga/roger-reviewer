# Roger Extension (Bounded Mirror Surface)

This extension injects a Roger launch panel on GitHub PR pages and dispatches
launch intents to local Roger. It is a bounded launch/mirror companion on the
current live surface: local Roger remains the source of truth.

Behavior on the current surface:

- actions are launch-oriented and may become more contextual over time rather
  than remaining a fixed flat primary set on every PR page
- dispatch path: Native Messaging only (`com.roger_reviewer.bridge`); when host
  registration is missing or broken, launch fails closed with setup guidance
- bounded status mirror: show a badge only when the bridge returns canonical
  Roger attention state plus a truthful freshness indicator
- findings affordance: keep `View Findings` hidden until bounded local state
  says findings are the next truthful focus
- launch-only honesty: if bounded readback is unavailable or stale, the panel
  hides badges, avoids findings-state claims, and points users to local Roger
  (`rr status`) as source of truth through secondary disclosure rather than a
  permanent inline status paragraph
- GitHub-native entry seam: prefer inline placement in PR header action regions,
  then render a bounded right-rail pane above reviewers when header placement is
  not coherent, and only then fall back to a page-local modal
- theme-aware visuals: panel, buttons, feedback text, and badges derive from
  GitHub/Primer CSS variables so light/dark themes stay legible
- build identity visibility: popup and injected panel surface the packaged
  extension build label through a persistent info disclosure so local reloads
  are distinguishable from tagged release builds without consuming prime card
  space
- no posting/approval controls are present in-extension

UX direction under active implementation:

- prefer a dedicated in-page `Roger Reviewer` host above the right-rail
  reviewers card when that is the clearest additive placement
- reduce avoidable clicks by inferring the likely primary next action when
  Roger already has enough local state
- keep elevated or mutation-sensitive actions explicit

Scope note: this stays a bounded mirror surface. Richer extension state/history
queues remain in the deeper-extension lane, deferred for now.

Theme/readability smoke checklist:

- `docs/extension-panel-theme-smoke.md`
- `docs/extension-visual-identity-smoke.md`
- `docs/extension-identity-direction.md`

## Visual Identity Direction (rr-vsr2)

Chosen direction: **Walkie-Talkie Relay**.

- compact walkie-talkie mark (`static/roger-mark.svg`) with metallic shell and
  radio accent for compact browser surfaces
- two-line wordmark (`static/roger-wordmark.svg`) with matching relay motif for
  popup and future cards
- shared token sheet (`static/roger-identity.css`) for consistent metallic
  accent/ink/canvas values across extension surfaces
- popup shell keeps manual-backup semantics with one primary launch action,
  conditional findings visibility, and a persistent build/fallback info
  affordance
- injected panel reuses the compact walkie-talkie mark in its Roger chip and
  keeps fallback/build explanation inside the same persistent `(i)` disclosure

Identity assets intentionally avoid mutating posting/approval semantics; they
decorate existing bounded UX rather than widening extension authority.

For local rebuild/reload loops, prefer Roger's scripted preload path:

```bash
scripts/extension/prepare_browser_test_env.sh --browser edge --reset-profile
```

That helper runs Roger's extension setup/doctor flow against the same dedicated
profile root, then relaunches the browser with the Roger unpacked extension
preloaded. For quick relaunches against an already-good dedicated profile, the
lower-level launcher remains available:

```bash
scripts/extension/launch_preloaded_browser.sh --browser edge --close-existing
```

Manual `Load unpacked` in Chrome/Brave/Edge remains a fallback, using
`apps/extension/manifest.template.json` as the manifest source.
