# Changelog

Roger Reviewer ships on a CalVer release lane (`vYYYY.MM.DD`). This file groups
the dated releases into the product milestone they belong to. Entries describe
what actually shipped; aspirational or feature-gated work is called out as such.

## 0.3 (v2026.07.08) — In-place updates, real memory, worker transport, session sanity

This release makes several surfaces that previously only *looked* real actually
work, and closes the update trap the prior release left for existing users.

### Updates that just work

- `rr update` now applies additive (class-A) store-schema migrations in place
  instead of failing closed on any schema change. The updater compares envelope
  *format* (not exact struct equality), classifies the real migration delta
  honestly, and — when it still must block — hands you the installer one-liner
  directly. **This is the last release you'll need a one-time installer pass to
  cross a schema bump**; the store advanced to schema 19 here, so users on
  `v2026.07.07` update once with the install command, then in place forever after.

### Memory that actually stores something

- Memory was a well-typed façade: the memory table had no production writer, all
  six search modes collapsed to a first-word-only substring scan, and posted
  review outcomes never entered search. Now: worker memory-review requests
  persist as durable rows; a TUI review surface (`m` toggle, `a`/`x`
  accept/reject) promotes accepted candidates into real memory (the first
  production writer); posted comments and edited draft bodies are indexed into
  search with provenance; search does multi-token AND matching with a direct
  identifier fast-path; and `rr status`/`rr doctor` now report semantic-asset
  posture with an install repair action. The heavier remainder (vector sidecar,
  memory graph, usage-event promotion ladder) is explicitly deferred and tracked.

### A review worker that can see its own session

- `rr review` now pre-stages a canonical worker-task binding (with nonce) and
  commits the session before the provider spawns, so an in-session agent can call
  the `rr agent` worker transport and resolve its own session. The feature-gated
  Copilot read-only policy gained a strict, fail-closed carve-out for that
  transport plus a scoped worker-inbox write exception and an inline
  `--request-b64` submission path — so a policy-sandboxed reviewer can read
  context, search memory, and submit findings without any repository-write
  capability. Live-proven read path against real Copilot; a live agent submitting
  a full findings pack remains an agent-steering follow-up.

### Session sanity and browser candidates

- `rr review` reuses an existing non-terminal session for the same PR instead of
  minting a new one every invocation (`--fresh` to override); candidate lists cap
  at the five most-recent per PR with ages. The browser panel finally renders the
  session inventory it already received — a per-session list with Resume buttons,
  an explicit picker when several sessions match (no more silent auto-pick),
  unified findings rendering, copyable `rr open --session` handoffs, and a
  visible-tab status re-poll. Edge 150+'s `--load-extension` removal is documented.

### Skills that teach the real product

- New `roger-worker-protocol` (the missing "how to be a Roger review worker"
  doctrine), `roger-operator-quickstart`, and `roger-tui-cheatsheet` skills;
  existing skills updated to the seven-verb CLI and `--interactive`; the stale
  "Copilot is preferred" claims in the doc mirrors are gone; the skills bundle
  now installs to the Codex skill root as well.

## 0.3 (v2026.07.07) — Seven-verb CLI, loud browser companion, editable outbound loop

This release massively simplifies Roger's operator surface and closes the last
mile on the flows that previously existed only as primitives.

### A seven-verb CLI

- The operator vocabulary is now `rr doctor, queue, review, open, findings,
  send, setup` plus two explicit machine surfaces (`rr api docs`, `rr agent`).
  Every old command keeps working as a quiet compatibility alias with an
  unchanged robot schema id.
- `rr send triage|draft|edit|approve|post` is the one gated outbound unit;
  `rr setup extension|doctor|fetch|update|assets|uninstall` is the one
  install/repair unit; `rr review --resume`, `rr findings --query`, and
  `rr findings --sessions` fold resume/search/sessions into the core verbs.
- `rr <command> --help` now works at any argument position, and every command
  rejects flags it does not support with an actionable message.

### Draft editing, end to end

- New `rr send edit --draft <id> (--body-file | --editor)` and a TUI `e` key
  revise a local draft body as a durable revision (original always preserved).
- Editing an approved batch revokes the approval with a typed reason and
  re-derives the exact-payload binding; re-running `rr send approve` reissues
  the token for the revised payload. Posted batches refuse edits.
- Live-proven on a sacrificial PR: the posted GitHub comment was byte-for-byte
  the revised body, then deleted with verification.

### Browser companion: louder, truer, fresher

- Native host now streams launch progress (`host_started`, `preflight_ok`)
  before the final result; the extension renders progress, fails fast within
  10s when the host never answers, and the status mirror degrades visibly
  instead of silently.
- PR-listing pages get per-row "Start Review in Roger" controls (the content
  script now actually loads on `/pulls`), and the PR panel gains a read-only
  findings staging view (severity/triage/outbound badges with file anchors)
  over the native bridge.
- Bridge-origin launches are recorded truthfully (`--surface bridge`) with a
  neutral working directory, so browser-launched sessions no longer poison
  repo bindings or block `rr status --session` readback.
- `rr setup doctor --live` performs a real native-messaging round trip through
  the installed launcher — the exact path the browser uses — plus a gh
  preflight, instead of filesystem checks alone.
- `rr setup update` (and `rr update`) now refreshes the fetched extension
  package and rewrites native-messaging host manifests after a successful
  binary swap, so updated users get the repaired companion, not a stale one.

### Copilot: first live proof + interactive drop-in

- Feature-gated Copilot (`RR_ENABLE_COPILOT_PROVIDER=1`) gained
  `--interactive`: `rr review/resume/return` hand a real terminal to the
  Copilot CLI and verify the session after exit.
- First-ever live (non-double) Copilot runs recorded: hook-verified start, the
  Roger-Review-Driver skill provably loaded in-session, `review_readonly`
  policy provably denying disallowed tools, transcript references captured,
  and hook audit events persisted per launch attempt.

### Honesty ledger

- The in-session `rr agent` worker round-trip remains live-unproven on Copilot
  because the read-only policy denies all bash (tracked as an open bug with a
  planned fail-closed carve-out); draft-body staging and clarification kickoff
  from the browser are tracked follow-ons.

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
