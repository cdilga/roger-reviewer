# Roger Extension (Bounded `0.1.0` Slice)

This extension injects a Roger launch panel on GitHub PR pages and dispatches
launch intents to local Roger.

Behavior in this slice:

- actions: `start_review`, `resume_review`, `show_findings`, `refresh_review`
- dispatch path: Native Messaging only (`com.roger_reviewer.bridge`); when host
  registration is missing or broken, launch fails closed with setup guidance
- bounded status mirror: show a badge only when the bridge returns canonical
  Roger attention state plus a truthful freshness indicator
- launch-only honesty: if bounded readback is unavailable or stale, the panel
  hides badges and points users to local Roger (`rr status`) as source of truth
- GitHub-native entry seam: prefer inline placement in PR header action regions
  and only fall back to a floating panel when no stable inline seam exists
- theme-aware visuals: panel, buttons, status text, and badges derive from
  GitHub/Primer CSS variables so light/dark themes stay legible
- no posting/approval controls are present in-extension

Scope note for `0.1.0`: this stays a bounded mirror surface. Richer extension
state/history queues remain in the deeper-extension lane.

Theme/readability smoke checklist:

- `docs/extension-panel-theme-smoke.md`

Load unpacked in Chrome/Brave/Edge using `apps/extension/manifest.template.json`
as the manifest source.
