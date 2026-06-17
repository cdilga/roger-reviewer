# Changelog

Roger Reviewer ships on a CalVer release lane (`vYYYY.MM.DD`). This file groups
the dated releases into the product milestone they belong to. Entries describe
what actually shipped; aspirational or feature-gated work is called out as such.

## 0.2 — Provider honesty, search defaults, and release machinery

This milestone made Roger's public surfaces tell one consistent story, fixed the
search and outbound-readiness edges that could mislead an operator, and hardened
the release and update path.

### Provider coherence (OpenCode-first)

- Reconciled every provider surface to a single OpenCode-first stance. OpenCode
  is the current first-class default and only live Tier B continuity path
  (locator reopen + `rr return`). The `--help`/robot-docs output, README,
  `AGENTS.md`, and the planning/release/onboarding docs now agree verbatim.
- Codex, Gemini, and Claude Code are documented literally as bounded Tier A:
  start, `ResumeBundle` reseed, and raw capture only — no locator reopen and no
  `rr return` claim.
- GitHub Copilot CLI is a feature-gated opt-in (`RR_ENABLE_COPILOT_PROVIDER=1`),
  disabled by default. When enabled it is a bounded Tier B lane (verified start,
  `review_readonly` policy posture, locator/session-id reopen, `rr return`, and
  honest reseed fallback). It is never the default or preferred lane; any
  "Copilot-first" product ordering in the planning docs is now marked explicitly
  as an aspirational future target, not the current stance.
- pi-agent stays out of the live surface: no `rr review --provider pi-agent`,
  admission deferred behind the same capability-tier rubric as every provider.
- Version-neutralized stray `0.1.0` framing in the affected provider docs so the
  current claim reads as the current claim, independent of release number.

### CLI honesty and polish

- Truthful session-picker, Copilot gate, pi-agent doctor, and search-flag
  surfaces (earlier in the cycle): commands stopped implying capabilities they
  do not have.
- `rr sessions --attention <state>` now validates the requested state against
  the canonical attention-state vocabulary and fails closed with a clear blocked
  envelope on a typo, instead of silently returning an empty exit-0 result.
- `rr findings` returns empty/exit-0 for a target with no session yet, matching
  `rr status` and `rr sessions`. Stale-binding and genuine-error cases still
  block.
- `rr triage` brought its required-argument validation (missing `--finding`,
  missing `--state`, unsupported `--state`) into `--robot` envelope conformance:
  a blocked JSON envelope and exit 3, consistent with `rr draft`/`approve`/`post`,
  instead of plain text and exit 2.
- Guarded `--batch` against swallowing a following flag-shaped value.

### Search

- Lexical-default search now exits 0 on the no-hit path instead of treating an
  empty result as a failure.
- Added the `rr assets` surface and a real semantic embedder. Semantic retrieval
  is feature-gated and opt-in: search only claims hybrid/semantic readiness when
  both the assets verify clean and the embedder is operational, so a stub
  embedder or unverified asset can never fake a semantic run.

### Browser extension

- Fixed the Edge status relay to use a `connectNative` Port, restoring the
  native-messaging status path on Edge.
- Mirrored the GitHub host light/dark theme and revamped the panel controls.

### Release and update machinery

- Packaged and shipped the `roger-*` skills alongside `rr`, reconciling the
  shipped skill content with the docs.
- Deprecated the orphaned release trigger and clarified CalVer install
  guidance.
- Slimmed the published `extension.zip` to runtime-only files.
- Derived the binary's embedded store envelope and `store_schema_version` from
  storage truth (migrations) rather than a stale constant, so updates reflect
  the real schema.
