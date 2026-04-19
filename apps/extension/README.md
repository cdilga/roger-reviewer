# Roger Extension (Bounded `0.1.0` Slice)

This extension injects a Roger launch panel on GitHub PR pages and dispatches
launch intents to local Roger.

Behavior in this slice:

- actions are launch-oriented and may become more contextual over time rather
  than remaining a fixed flat primary set on every PR page
- dispatch path: Native Messaging only (`com.roger_reviewer.bridge`); when host
  registration is missing or broken, launch fails closed with setup guidance
- bounded status mirror: show a badge only when the bridge returns canonical
  Roger attention state plus a truthful freshness indicator
- launch-only honesty: if bounded readback is unavailable or stale, the panel
  hides badges and points users to local Roger (`rr status`) as source of truth
- GitHub-native entry seam: prefer inline placement in PR header action regions,
  then render a bounded right-rail pane above reviewers when header placement is
  not coherent, and only then fall back to a page-local modal
- theme-aware visuals: panel, buttons, status text, and badges derive from
  GitHub/Primer CSS variables so light/dark themes stay legible
- build identity visibility: popup and injected panel surface the packaged
  extension build label so local reloads are distinguishable from tagged
  release builds
- no posting/approval controls are present in-extension

UX direction under active implementation:

- prefer a dedicated in-page `Roger Reviewer` host above the right-rail
  reviewers card when that is the clearest additive placement
- reduce avoidable clicks by inferring the likely primary next action when
  Roger already has enough local state
- keep elevated or mutation-sensitive actions explicit

Scope note for `0.1.0`: this stays a bounded mirror surface. Richer extension
state/history queues remain in the deeper-extension lane.

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
  demoted findings action, and a persistent build/fallback info affordance

Identity assets intentionally avoid mutating posting/approval semantics; they
decorate existing bounded UX rather than widening extension authority.

Load unpacked in Chrome/Brave/Edge using `apps/extension/manifest.template.json`
as the manifest source.
