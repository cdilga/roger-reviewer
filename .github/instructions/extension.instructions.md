---
applyTo: "apps/extension/**/*"
---

Follow the Roger browser-extension boundary:
- The extension is a bounded surface, not the source of truth for Roger session state.
- Native Messaging through the installed `rr` binary is the only supported `0.1.x` browser bridge.
- Keep the bridge contract typed, explicit, and fail-closed when prerequisites are missing.
- Do not add direct GitHub write behavior or bypass Roger approval flows from browser code.
- Minimize runtime dependencies and keep packaging/install behavior reproducible.
