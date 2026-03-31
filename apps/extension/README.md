# Roger Extension (Bounded `0.1.0` Slice)

This extension injects a Roger launch panel on GitHub PR pages and dispatches
launch intents to local Roger.

Behavior in this slice:

- actions: `start_review`, `resume_review`, `show_findings`, `refresh_review`
- dispatch order: Native Messaging first (`com.roger_reviewer.bridge`), then
  custom URL fallback (`roger://launch/...`)
- launch-only honesty: the panel does not claim live local session status when
  readback is unavailable
- no posting/approval controls are present in-extension

Load unpacked in Chrome/Brave/Edge using `apps/extension/manifest.template.json`
as the manifest source.
