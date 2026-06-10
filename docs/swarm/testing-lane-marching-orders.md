Read `AGENTS.md` first, then `README.md`.

You are part of a dedicated Roger Reviewer testing swarm, not the normal
implementation-bead swarm.

Primary objective:

- continuously exercise real `rr` operator paths
- keep provider, CLI, bridge, extension, and recovery claims truthful
- surface exact reproduction commands, artifacts, and contention failures
- avoid shared-state collisions between panes

Required startup workflow:

1. Choose one concrete target journey before you start exploring:
   - a budgeted major journey such as `E2E-01` through `E2E-06`
   - or one persona-plus-flow pairing such as `PJ-03A + F01`
   - or one named cheaper proof owner when the docs say a smoke or integration
     suite is the truthful starting point for that journey
2. Announce the target explicitly in your first substantive output:
   - `Target: <E2E/suite id or persona+flow pair>`
   - `Why this lane owns it: <one sentence>`
   - `Planned proof: <command or manual smoke path>`
3. Start the live operator path immediately after that declaration. Do not
   spend turns on a full testing-doc reread before the first real attempt.
4. If the operator path blocks or the support claim is ambiguous, consult only
   the minimum canonical testing doc needed to disambiguate it:
   - `docs/TESTING.md`
   - `docs/PERSONA_JOURNEYS_AND_CHAOS_RECOVERY.md`
   - `docs/REVIEW_FLOW_MATRIX.md`
   - `docs/RELEASE_AND_TEST_MATRIX.md`
   - `docs/TEST_EXECUTION_TIERS_AND_E2E_BUDGET.md`
5. When you finish or block on one target, pick the next flow from the same
   canonical docs rather than falling back to generic command fishing.

Operating rules:

- isolate your Roger store, worktree, container namespace, and temp dir before
  you touch `rr`
- keep findings concrete: exact command, exact output, exact files or logs
- do not hand-edit `.beads/issues.jsonl`
- if you mutate repo files, reserve them first through Agent Mail
- use Agent Mail to hand off browser-only or cross-lane tasks instead of
  assuming another pane is watching your terminal output
- if you find a reproducible Roger failure, record whether it is CLI-only,
  browser-only, provider-specific, environment-specific, or contention-specific
- stay in your assigned testing lane; do not drift into unrelated product work
  unless the operator explicitly redirects you

Truth boundaries:

- current live provider truth is: `opencode` supports resume/return, while
  `codex`, `gemini`, and `claude` are bounded Tier A paths and should not be
  treated as `rr return`-capable
- `rr extension setup` and `rr extension doctor` are necessary browser-path
  checks, not proof that all extension journeys work
- when the browser lane needs to bootstrap or recover a dedicated browser
  profile, prefer Roger's higher-level helper over manual extension-page pokes:
  `scripts/extension/prepare_browser_test_env.sh --browser <edge|chrome|brave> [--profile-root <path>] [--reset-profile]`
- when the browser lane only needs a quick relaunch against an already-good
  dedicated profile, the lower-level launcher is still available:
  `scripts/extension/launch_preloaded_browser.sh --browser <edge|chrome|brave> [--profile-root <path>] --close-existing`
- do not reach for the stitched all-E2E aggregate by default; prefer one
  specific journey, persona cut, or cheaper proof owner unless the operator
  explicitly asks for the aggregate run
- use `rch exec -- <command>` for CPU-heavy Cargo work when available, but do
  not stall if it fails open to local execution
