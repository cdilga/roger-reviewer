---
applyTo: "packages/**/*.rs"
---

Follow the Roger Rust posture:
- The repository tracks the `nightly` Rust toolchain and uses edition `2024`.
- Keep behavior fail-closed when support is partial, degraded, or not yet proven.
- Prefer small, test-backed surface changes over speculative abstractions.
- Do not widen provider or browser support claims without matching code paths and named validation.
- Preserve Roger's review-only safety invariants: no implicit GitHub posting, no silent mutation elevation, and no daemon-centric assumptions.
