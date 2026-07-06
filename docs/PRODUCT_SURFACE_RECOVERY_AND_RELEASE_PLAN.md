# Product Surface Recovery And Release Plan

Status: active umbrella plan (supersedes overlapping scope in
`docs/SELF_EXPLORATION_AND_REVIEW_WORKBENCH_RECOVERY_PLAN.md` where the two
disagree; that document remains the deeper TUI/journey reference)
Class: implementation support plan / bead creation packet / release gate packet
Audience: maintainers and agents executing the surface simplification and the
next CalVer release

Authority:

- `AGENTS.md` remains the operational authority.
- `docs/PLAN_FOR_ROGER_REVIEWER.md` remains the canonical product plan.
- `docs/CLI_SURFACE_SIMPLIFICATION_CONTRACT.md` (v2) is the CLI grammar
  authority for this plan.
- `docs/RELEASE_CALVER_VERSIONING_CONTRACT.md` and
  `docs/UPDATE_RELEASE_AND_TESTED_UPGRADE_CONTRACT.md` govern the release lane.

## Goal

One CalVer release that makes Roger simple and honest end to end:

1. an operator CLI of **seven verbs** (`doctor, queue, review, open, findings,
   send, setup`) plus two explicit machine surfaces (`api`, `agent`), with
   every old command kept as a quiet compatibility alias;
2. a browser companion that **installs with one command, verifies itself with
   a real native-messaging round trip, and never fails silently** — every
   extension-originated launch renders progress and failure with repair
   guidance;
3. a TUI where the core review loop actually completes: **pick the right
   session, inspect findings, edit the draft, approve** — with memory/search
   posture visible and empty states explained;
4. **ticketed live proof** of the two flows the product has never truthfully
   demonstrated: (a) drop into and manually drive a real GitHub Copilot CLI
   session with Roger skills verifiably loaded, and (b) a full loop where real
   feedback is posted back to GitHub after local editing;
5. `rr update` (and `rr setup update`) that delivers **all of it** — binary,
   TUI, extension package, and native-messaging host refresh — so existing
   users get the repaired product from any install vintage.

## Ground truth this plan is built on (2026-07-06 inventory)

Confirmed by direct code inventory; each item is a defect or gap this plan
closes. File references are as of commit `0297076`.

| # | Fact | Where |
|---|------|-------|
| G1 | `send`, `setup`, `api` are aspirational: help text names them but `parse_args` has no such tokens | `packages/cli/src/lib.rs:594-620`, `:15479` |
| G2 | 21 top-level commands; `bridge`(5)/`extension`(4)/`assets`(3) plumbing at top level; 10 commands lack positive flag whitelists; `rr <cmd> --help` is broken (only first-token help works) | `lib.rs:589-1464` |
| G3 | Draft editing does not exist: no storage body-update API, no CLI command, no TUI edit key; `tui_shell.rs`'s edit model is orphaned (unused dep, zero references from `packages/tui`) | `packages/storage/src/lib.rs`, `packages/app-core/src/tui_shell.rs:167-205` |
| G4 | Session picker discards its candidates: `PickerRequired` resolution collapses to `initial_session_id = None` | `lib.rs:13721` |
| G5 | Bridge-origin launches are recorded as `LaunchSurface::Cli` with a browser-poisoned cwd; `LaunchSurface::{Extension,Bridge}` variants are never constructed; no `--surface` flag exists | `packages/bridge/src/lib.rs:516-552,605`, `packages/cli/src/main.rs:59` |
| G6 | Extension launch feedback goes silent on hangs for 120s; status-probe failures degrade silently by design; the host sends exactly one reply frame (no ack/progress protocol) | `apps/extension/src/background/main.js:23,30`, `packages/cli/src/main.rs:16-56` |
| G7 | `rr extension doctor` performs zero live checks — five filesystem/JSON checks, no native-messaging round trip, no `gh auth` preflight | `lib.rs:4962-5186` |
| G8 | The PR-listing row controls are dead code in real browsers: `manifest.template.json` `content_scripts.matches` covers only `/pull/*`, never `/pulls` | `apps/extension/manifest.template.json` |
| G9 | `rr update` replaces only the binary; extension package and host registration are never refreshed — updated users keep stale extensions indefinitely | `lib.rs:10189-10248` |
| G10 | Copilot has never run live: all proof is fake-binary doubles; launch uses `.output()` (blocking batch) so no interactive drive is possible; `RR_COPILOT_HOOK_AUDIT_DIR` is never set, so advertised denial/transcript audit artifacts are declared-only | `lib.rs:6119-6270`, `scripts/copilot-hooks/*` |
| G11 | `rr post` works live (proven once, rr-5dp9) but only for issue comments and thread replies — no first-class PR review submission | `packages/github-adapter/src/lib.rs:167-233` |
| G12 | TUI gaps vs contract: no composer, no draft edit, no lineage/run grouping, no evidence excerpts, no memory-lane/trust posture, no baseline visibility, no dropout affordance | `packages/tui/src/*` vs `docs/TUI_WORKSPACE_AND_OPERATOR_FLOW_CONTRACT.md` |

## Delivery tracks

### Track 1 — CLI surface simplification (closes G1, G2)

Implement the full v2 grammar in `docs/CLI_SURFACE_SIMPLIFICATION_CONTRACT.md`:

- `rr send triage|draft|edit|approve|post` container routing to the existing
  fail-closed handlers plus the new `edit` handler (Track 3 dependency).
- `rr setup extension|doctor|fetch|update|assets|uninstall` container routing
  to the existing extension/update/assets handlers plus the new live doctor.
- `rr api docs <topic>` routing to robot-docs. `rr agent` stays as-is.
- `rr review --resume [--pr n|--session id]` routing to the resume handler.
- `rr findings --query <text>` routing to search; `rr findings --sessions`
  routing to sessions.
- Per-command `--help` (any position) printing focused usage per command.
- Positive flag whitelists for all commands (kills the silent-flag class).
- `rr --help` leads with the seven verbs; README truth pass in the same slice.
- All old commands stay routable; robot schema ids unchanged (aliases emit the
  underlying command's schema id).

Validation: parser unit tests for every alias→handler route and whitelist;
`cli_polish_smoke` extended; robot-docs truthfulness check.

### Track 2 — Extension bootstrap, feedback, and update freshness (closes G5–G9)

1. **Manifest fix**: add PR-listing matches so the landed row controls run;
   extend the CDP live harness with a listing-page scenario.
2. **Ack/progress protocol**: the native host emits an immediate ack frame
   after spawn and a progress frame after preflight, then the final result on
   the same Port. Extension renders "host connected / launching…" states; the
   no-ack watchdog drops to ~10s (fast, loud failure when the host is absent)
   while the overall completion watchdog stays generous. Status-probe silent
   degrade gains a visible "mirror unavailable" note instead of nothing.
   `bridge.ts` contract re-exported; `verify-contracts` stays green.
3. **Launch surface truth + cwd repair**: bridge dispatch passes
   `--surface bridge`; `rr` parses it, records `LaunchSurface::Bridge`, and
   bridge-origin launches bind to the resolved repo/PR target rather than the
   browser-poisoned cwd; explicit-session readback (`rr status --session`)
   must not block on bridge-origin bindings.
4. **Doctor gets teeth**: `rr setup doctor --live` spawns the actual launcher
   script, sends a length-prefixed StatusProbe over stdin, asserts a
   well-formed reply, and runs the gh-auth preflight — proving the exact
   browser-spawn path minus the browser.
5. **Update freshness**: after a successful binary swap, `rr update` refreshes
   the fetched extension package to the new version and rewrites host
   manifests/launcher when (and only when) the store shows extension setup
   ever happened; surfaces a "reload the extension in your browser" repair
   action. Never blocks a CLI-only user.

Validation: bridge/CLI integration tests for surface tagging and cwd binding;
live CDP panel-interaction harness run (Edge strict case) including
listing-page launch; doctor `--live` integration test against a real spawned
host; update-refresh covered in the Docker install/update E2E.

### Track 3 — Draft revisions + TUI completion (closes G3, G4, part of G12)

1. **Storage**: `OutboundDraftRevision` model — revise body pre-approval,
   preserve original and revision lineage; editing an approved draft
   invalidates the approval token (fail-closed re-approve). Additive schema
   migration under the store migration contract.
2. **CLI**: `rr send edit --draft <id> (--body-file f | --editor)`.
3. **TUI**: `e` on a draft item suspends the TUI into `$EDITOR` (git-style),
   saves a revision on exit; session picker screen fed by the previously
   discarded `PickerRequired` candidates; memory/search posture line
   (semantic asset state, retrieval mode, degraded reasons) in Search screen
   and status strip; empty findings states explain worker/task status and the
   next command.
4. **tui_shell.rs reconciliation** is beaded separately: merge the orphaned
   DTO shell into the live backend or delete it; the current
   dead-but-tested state is a standing lie and stays a tracked bead, not a
   silent debt.

Validation: storage unit tests for revision lineage + approval invalidation;
TUI model tests via FakeBackend for picker/edit/posture; e2e: edit → approve →
post uses the revised body.

### Track 4 — Copilot live drop-in + audit truth (closes G10)

1. **Interactive drive**: a copilot launch mode with inherited stdio (the
   OpenCode reopen pattern) behind the existing feature gate — `rr review
   --provider copilot --interactive` and `rr return` for copilot sessions
   attach a real terminal; verified-start hook artifact still checked after
   the session ends.
2. **Audit wiring**: set `RR_COPILOT_HOOK_AUDIT_DIR` to a session-scoped store
   path at launch; record artifact references after session end so
   `copilot_tool_denial` / `copilot_transcript_reference` become real, not
   declared-only.
3. **Skills-loaded validation**: session-start artifact (policy digest +
   profile id) plus an in-session probe — the seeded prompt directs the agent
   to call `rr agent worker.get_status`, which leaves store-visible evidence
   that the in-session binding genuinely works.

Validation: existing double-based smoke stays; the live bar is Track 5.

### Track 5 — Ticketed live proof (falsifiability bar per AGENTS.md)

Each proof is a real run with replayable artifacts recorded in the closing
bead (run id, transcript, store diff, or command transcript):

- P1: fresh `rr setup extension --browser edge` on this machine → live CDP
  harness proves panel and listing-row launch feedback, including a forced
  failure (host removed) rendering loud guidance.
- P2: live Copilot drop-in — `RR_ENABLE_COPILOT_PROVIDER=1`, real
  `copilot` 1.0.61, interactive session driven manually, session-start
  artifact + audit dir + `worker.get_status` store evidence captured.
- P3: full outbound loop on a sacrificial PR — findings → triage → draft →
  **edit (revised body)** → approve → post; `PostedAction` records the
  remote comment URL; the posted body provably equals the revised body;
  cleanup per the rr-5dp9 runbook.
- P4: in-session agent memory flow (existing bead
  `rr-in-session-agent-memory-flow-live-unproven-r3zp`) — live OpenCode or
  Copilot session round-trips `worker.search_memory` and
  `worker.submit_stage_result`.

### Track 6 — Release (closes G9 for users)

- Cut CalVer tag, run the release workflow, publish via `workflow_dispatch`
  with `publish_mode=publish` + `operator_smoke_ack` after the operator smoke
  checklist (`docs/release-publish-operator-smoke.md`).
- Docker install/update E2E from the previous published version to the new
  one, now also asserting extension-package refresh.
- CHANGELOG highlights authored for the release notes generator.
- README/docs truth pass ships in the same release.

## Sequencing

Track 1 and Track 2 items 1–4 and Track 3 storage/CLI are parallel-safe.
Track 3 TUI depends on Track 3 storage. Track 2 item 5 (update freshness)
lands before Track 6. Track 5 runs after Tracks 1–4 land. Track 6 is last and
is an operator-gated publish.

## Bead mapping

- New epic `rr-cli-simplification` (Track 1) with parser/help/README children.
- Track 2 folds into existing `rr-github-pr-listing-review-kickoff-ziv8`
  (manifest + live validation) and `rr-self-exploration...-tmmn.1`
  (launch-surface/cwd), plus new beads for ack protocol, live doctor, and
  update freshness.
- Track 3 maps to `tmmn.4` (draft revisions), `tmmn.9` (session picker),
  `tmmn.3` (workbench upgrade — partial, truthfully scoped), plus a new
  tui_shell reconciliation bead.
- Track 4/5 are new beads; P4 is the existing `r3zp` bead.
- Track 6 is a new release bead bound to the release-candidate gate.

## Non-goals for this plan

- No first-class PR review submission (G11 stays issue-comment/thread-reply;
  bead it for a later lane).
- No full TUI contract completion (composer/prompt palette remain beaded
  gaps).
- No devbox environment convergence: local `main` is the convergence
  authority per operator direction (2026-07-06); devbox is compute offload
  only for this iteration.
