# Plan For Truthful Provider Parity And GitHub Copilot CLI

Status: Proposed bounded side-plan. Accepted directions should be folded back
into `PLAN_FOR_ROGER_REVIEWER.md` or the relevant support contracts rather than
leaving this file as a long-lived parallel source of product truth.
Intended repo path: `docs/PLAN_FOR_TRUTHFUL_PROVIDER_PARITY_AND_GITHUB_COPILOT_CLI.md`
Audience: Roger maintainers and implementers working on `0.1.x` hardening and provider expansion

---

## Why this plan exists

Roger already has the right high-level instincts:

- local-first, with Roger's own ledger as the source of truth
- explicit approval gates before GitHub writes
- an explicit harness boundary with capability tiers
- a preference for one honest blessed path over a broad but shallow support claim

The repo is not yet at the point where those ideas are fully true in the shipped paths. The shortfalls are concentrated in six areas:

1. core lifecycle truthfulness (`review`, `resume`, `return`, bridge launch)
2. transactionality and crash safety in the authoritative local ledger
3. explicit outbound approval/posting productization
4. support-claim discipline across OpenCode, Codex, Gemini, and Claude Code
5. lack of a first-class review-worker transport and execution ledger
6. lack of a second serious, local-first provider that fits Roger's architecture cleanly

GitHub Copilot CLI is a strong fit for that fifth gap. It is terminal-native, interactive, supports programmatic invocation, supports resuming previous sessions by session ID, records local session data, exposes repository-scoped hooks with session lifecycle payloads, and offers an ACP server for later integration work. It also has enough policy surface to run in a Roger-owned, fail-closed review posture.

This plan is therefore not just “add another provider.” It is a combined hardening plan:

- make Roger's existing core lifecycle honest
- finish the support claims Roger already wants to make
- add GitHub Copilot CLI as a first-class provider without weakening Roger's safety model

Documentation-maintenance rule for this file:

- use it as a bounded synthesis input while the lane is still moving quickly
- once the direction is accepted, merge the live product truth back into the
  canonical plan and relevant support contracts
- do not let this remain the only place where active Roger behavior is defined

---

## Headline decisions

### D1. Do not broaden providers on top of a synthetic launch model

Provider expansion should not continue on top of a lifecycle where Roger can record a review session as launched before a real harness session has been verified.

**Decision:** land transactional, verified lifecycle changes first, or as part of the same slice that introduces Copilot.

### D2. Keep Roger authoritative; treat provider state as adapter-owned continuity

Roger's session ledger remains the source of truth. Provider session stores, transcripts, and local caches remain harness-side continuity assets only.

**Decision:** Roger stores provider session IDs, policy digests, audit artifacts, and transcript references, but does not outsource truth to provider-local state.

### D3. Make GitHub Copilot CLI a first-class provider through the same tier contract

Copilot should not get a one-off exception. It should enter through the existing `HarnessAdapter` contract and the same Tier A / Tier B / Tier C vocabulary already present in the repo.

**Decision:** add a dedicated `session-copilot` crate and update all planning, test, and support matrices accordingly.

### D3A. Freeze the provider hierarchy explicitly

The authoritative provider order is:

1. GitHub Copilot CLI
2. OpenCode
3. Codex
4. Gemini
5. Claude Code

**Decision:** this is the product support hierarchy everywhere in the docs, while
live claims remain gated by proof for each provider.

### D4. Prefer direct CLI + hooks for the first integration, not ACP-first

Copilot CLI's ACP server is attractive, but it is explicitly in public preview. The CLI already exposes enough stable surfaces for a serious first integration: interactive launch, `--interactive`, `--resume`, repository hooks, repository custom instructions, and local session state.

**Decision:** first pass uses direct CLI invocation plus repository-scoped hooks. ACP becomes a later spike behind a feature flag.

### D5. Disable Copilot capabilities that bypass Roger's safety posture

Roger explicitly forbids hidden GitHub posting and hidden mutation. Copilot CLI includes tool execution, built-in MCP servers, local file mutation, and URL access.

**Decision:** Roger-managed Copilot sessions must launch under a narrow, review-safe policy profile by default:
- no built-in GitHub MCP server by default
- no raw `gh` review posting path
- no write tool by default in review mode
- no shell execution by default in review mode
- no external URL access by default in review mode
- no provider memory write as a substitute for Roger memory
- no “allow all” / “yolo” mode in Roger-managed review sessions

### D6. Use a dedicated `rr agent` transport for the review worker

Roger's worker/agent boundary should not be left as implied prompt glue or
folded into `--robot`.

**Decision:** make the worker contract transport-neutral, use `rr agent ...` as
the first concrete in-session transport, keep `--robot` for operator-facing
machine-readable commands, and treat MCP as an optional later adapter only.

---

## Definition of “first-class provider” in Roger

A provider is first-class only when all of the following are true:

1. **Canonical docs:** it appears in the main product plan, release/test matrix, onboarding docs, README support snapshot, and current quickstart language.
2. **CLI surface:** the provider is reachable through `rr review --provider <name>` and its support tier is reflected truthfully in help and status output.
3. **Verified start path:** Roger records a real provider session ID, not a synthetic placeholder, before claiming the session is launched.
4. **Durable continuity:** Roger can truthfully support at least Tier A, and should not claim Tier B or Tier C unless the implementation actually proves it.
5. **Guardrails:** Roger controls the provider's write posture, GitHub posture, path scope, and audit trail.
6. **Acceptance coverage:** the provider has deterministic test doubles plus at least one real-world smoke/acceptance path commensurate with the support claim.
7. **Operator guidance:** install, auth, policy failure, and environment drift are all surfaced clearly and fail closed.

Until those conditions hold, the provider is bounded, experimental, or contract-shaping only.

---

## Recommended provider matrix after this plan

### Target state

| Provider | Intended status | Target tier | Notes |
|---|---|---:|---|
| GitHub Copilot CLI | First-class / golden path | Tier B | Authoritative `#1` provider target once verified |
| OpenCode | First-class fallback/reference | Tier B (selected Tier C optional) | Authoritative `#2` provider and current strongest landed continuity path |
| Codex | Secondary, bounded | Tier A | Authoritative `#3` provider; keep truthful or remove live claim until verified |
| Gemini | Secondary, bounded | Tier A | Authoritative `#4` provider; keep adapter lane honest until live launch is exposed |
| Claude Code | Secondary, bounded | Tier A | Authoritative `#5` provider; keep live claim literal and bounded |
| Pi-Agent | Deferred future candidate | Tier A first, only if admitted later | Planning-only after the current matrix stabilizes; use `_exploration/pi_agent_rust` as a reference target and require the same admission rubric rather than special-casing it |
| `gh` | GitHub adapter only | N/A | Never a review harness |

### Immediate truthfulness rule

If OpenCode is not yet truly Tier B in the live path, Roger should **not** describe Copilot as “matching OpenCode parity” on day one. Instead:

- admit Copilot into the first-class **plan**
- ship Tier A or Tier B only when the implementation actually satisfies the claim
- keep provider help, README, and release notes completely literal
- keep Pi-Agent and any other future harnesses out of live claims until a later
  admission spike defines whether they deserve even Tier A

---

## Workstreams

## W1. Hardening the core Roger lifecycle

### Goal

Make `review`, `resume`, `refresh`, `return`, and bridge launch truthful, atomic, and crash-safe before or while adding Copilot.

### Problems to close

- Roger can currently appear to record success before a verified harness launch has happened.
- multi-step lifecycle persistence is vulnerable to partial writes
- bridge launch still behaves like a stubbed dispatch surface
- operator guidance and command surface are drifting (`rr init` guidance versus visible CLI reality)

### Required changes

#### W1.1 Introduce a launch-attempt state machine

Add a distinct lifecycle for harness launch attempts:

- `pending`
- `verified_started`
- `verified_reopened`
- `verified_reseeded`
- `failed_preflight`
- `failed_spawn`
- `failed_session_binding`
- `abandoned`

Do **not** create or finalize a durable `review_session` until the harness adapter has returned a verified `SessionLocator` backed by a real provider session ID.

If launch fails after a partial provider interaction but before final Roger commit, record that failure in the attempt ledger, not as a completed Roger review session.

#### W1.2 Make core lifecycle commits transactional

Wrap the following operations in a single storage transaction per command:

- review launch binding
- resume binding / rebinding
- refresh run creation
- return rebind
- continuity and attention updates that depend on the same user-visible event

Target packages:
- `packages/storage`
- `packages/app-core`
- `packages/cli`
- any provider package that currently leaks partially-written lifecycle state

#### W1.3 Replace bridge “pretend success” with actual `rr` execution

The bridge should:

- validate preflight
- call the real Roger CLI in `--robot` mode
- parse the returned machine-readable payload
- return the canonical Roger session ID and status only after the actual CLI call succeeds

Do not report success with `None` session IDs.

#### W1.4 Reconcile bootstrap/doctor/init reality

Pick one of these and finish it:

- implement `rr init`, or
- remove `rr init` guidance everywhere and replace it with the actual store/bootstrap command

Settled path: implement a lightweight `rr init` plus `rr doctor` family because
Roger now needs a cross-provider preflight surface anyway.

### Acceptance criteria

- no user-visible success state is emitted before a real provider session ID exists
- crash during launch does not leave a completed-looking session in Roger
- bridge responses contain canonical session IDs on success
- bootstrap guidance and CLI surface are consistent everywhere

---

## W2. Productize the explicit outbound approval/posting flow

### Goal

Finish the workflow Roger already claims: draft -> approve -> post, with explicit human control and clear lineage.

### Problems to close

- the safety model is already documented, but the visible CLI surface does not yet present the outbound flow as a complete product path
- approval and posting state need to be obvious to operators and to Roger's own status surfaces

### Required changes

#### W2.1 Add or formalize explicit CLI commands

Introduce a first-class CLI path for outbound state transitions. Examples:

- `rr draft`
- `rr approve`
- `rr post`

or a finding-scoped subcommand family if that fits the current domain better.

What matters is not the exact command names. What matters is that the command surface visibly mirrors the documented approval model.

#### W2.2 Make outbound state machine visible

For every finding or outbound draft, make the following explicit and queryable:

- draft created
- awaiting approval
- approved
- posted
- superseded
- invalidated by refresh / retarget

#### W2.3 Preserve GitHub write mediation

All provider-originated suggestions still flow through Roger. Neither Copilot nor any other provider may comment, review, or merge directly on GitHub in a Roger-managed review session.

### Acceptance criteria

- operator can complete the whole review -> approve -> post loop from the product surface
- state transitions are queryable and auditable
- provider sessions cannot bypass Roger to perform remote writes in review mode

---

## W3. Finish OpenCode truthfully before using it as the benchmark

### Goal

Bring the blessed OpenCode path up to the support claims Roger already wants to make.

### Required changes

- replace any synthetic session start path with a verified one
- finish real `return_to_roger_session`
- wire worktree identity into the live path, not just into the supporting crate
- add real continuity probes so “usable / degraded / unusable” reflects actual reopen behavior
- treat stale or mismatched target/worktree bindings as hard failures, not soft guesses

### Acceptance criteria

- OpenCode start, reopen, reseed, and return are all real
- worktree-aware target binding is enforced in the path users actually execute
- OpenCode remains the standard against which Tier B support is measured

---

## W4. Narrow Codex and Gemini to honest claims

### Goal

Keep Codex and Gemini helpful without overstating parity.

### Required changes

- keep Codex in live CLI only if the launch path is verified and the docs remain explicit about Tier A limits
- keep Gemini as adapter-contract only until the live launch surface is truly exposed
- remove or hide commands/help text that imply deeper support than the provider actually has

### Acceptance criteria

- no provider claims reopen/dropout/return support unless it is demonstrably implemented
- README, help text, release notes, and tests all tell the same story

---

## W5. Add GitHub Copilot CLI as a first-class provider

## Why Copilot is a good fit

GitHub Copilot CLI has several properties that align well with Roger:

- it is terminal-native and local-first
- it supports interactive sessions plus programmatic invocation
- it supports `--resume SESSION-ID` and `--continue`
- it records local session state and a local session store
- it exposes repository-scoped hooks with `sessionId`, `cwd`, `source`, tool events, and transcript paths
- it supports repository-wide and path-specific custom instructions
- it supports local and remote MCP servers
- it exposes an ACP server for later, more structured integration

That makes Copilot a much stronger architectural fit for Roger than a one-shot stateless prompt client.

## W5.1 Add a dedicated provider crate

Create:

- `packages/session-copilot/`

Update:

- root `Cargo.toml` workspace members
- CLI provider parsing and help
- config defaults / environment overrides
- docs and matrices

Recommended provider identifier:

- CLI: `copilot`
- internal provider slug: `copilot`

### Configuration additions

Add explicit config for:

- `RR_COPILOT_BIN` (default: `copilot`)
- `RR_COPILOT_MODEL` (optional explicit model pin)
- `RR_COPILOT_CONFIG_DIR` (advanced override only; default to Copilot's normal config path)
- `RR_COPILOT_POLICY_PROFILE` (`review_readonly`, `review_shell_readonly`, future `fix_mode`)
- `RR_COPILOT_DISABLE_BUILTIN_MCPS` (default true in review mode)
- `RR_COPILOT_HOOK_PROFILE_VERSION`

Do **not** make Roger depend on experimental Copilot flags for its baseline support claim.

## W5.2 Choose the initial integration surface

### Chosen first pass: direct CLI + repository hooks

Use the stable CLI surface first:

- `copilot --interactive "<seed prompt>"` for new interactive sessions
- `copilot --resume <SESSION-ID>` for reopen
- `copilot --continue` only for human convenience, not for Roger's deterministic rebind logic
- repository hooks for session IDs, audit events, tool gating, transcript references
- repository custom instructions for Roger-specific rules
- path-specific instructions for Rust crates and the extension

### Deferred spike: ACP

ACP is worth evaluating later for:
- tighter control of session lifecycle
- richer structured streaming
- better continuity probes
- future in-harness Roger commands

But because ACP support is currently public preview, Roger should not center a `0.1.x` support claim on it.

## W5.3 Make the Copilot launch path Roger-owned and verifiable

### Start path

For `rr review --provider copilot`:

1. resolve repo/worktree root first
2. generate Roger prompt pack and policy profile
3. create a **pending** launch attempt only
4. start Copilot from the repo/worktree root with Roger-managed flags and environment
5. wait for a session-start hook artifact that includes a real Copilot `sessionId`
6. only then commit the Roger `review_session` + `SessionLocator` transaction

### Recommended launch shape

Use interactive mode, not one-shot prompt mode, for live review sessions:

- `copilot --interactive "<Roger seed prompt>"`

This keeps the session interactive while allowing Roger to inject the initial review prompt immediately.

### SessionLocator for Copilot

Copilot `SessionLocator` should store at least:

- `provider = "copilot"`
- real `session_id`
- `invocation_context_json` containing:
  - repo/worktree root
  - Roger session ID
  - Copilot model (if pinned)
  - policy profile name and digest
  - hook profile version and digest
  - custom-instructions digest(s)
  - whether built-in MCP servers were disabled
  - timestamp and platform metadata
- `captured_at`
- `last_tested_at`

Do not store secrets in `invocation_context_json`.

## W5.4 Implement Tier A support first, then Tier B

### Tier A exit criteria

Copilot Tier A is complete only when Roger can truthfully do all of the following:

- start a Roger-owned Copilot review session
- bind it to a review target
- capture raw output durably
- normalize findings from that output
- reseed a fresh session from a `ResumeBundle`
- report continuity quality without bluffing

### Tier B target

Copilot earns first-class Tier B status when Roger can additionally:

- reopen by locator using `copilot --resume <SESSION-ID>`
- support bare-harness continuation in direct Copilot CLI
- support `rr return` from a Copilot session back into Roger
- degrade to reseed honestly when locator reopen is stale or unusable

## W5.5 Use Copilot hooks as the main observability and control surface

Create a Roger-owned hook profile under the repository, plus scripts in `scripts/copilot-hooks/`.

### Hook files to add

Recommended repository additions:

- `.github/copilot-instructions.md`
- `.github/instructions/rust.instructions.md`
- `.github/instructions/extension.instructions.md`
- `.github/hooks/roger-review.json`
- `scripts/copilot-hooks/session-start.sh`
- `scripts/copilot-hooks/session-start.ps1`
- `scripts/copilot-hooks/user-prompt.sh`
- `scripts/copilot-hooks/pre-tool-use.sh`
- `scripts/copilot-hooks/post-tool-use.sh`
- `scripts/copilot-hooks/agent-stop.sh`
- `scripts/copilot-hooks/session-end.sh`

### Hook responsibilities

#### `sessionStart`

Capture:

- `sessionId`
- `cwd`
- `source` (`new`, `resume`, `startup`)
- Roger pending launch ID
- initial prompt hash/digest

Write a Roger-readable event artifact so the CLI can verify the launch before committing final session state.

#### `userPromptSubmitted`

Capture:

- prompt digest
- Roger session ID
- whether the prompt came from Roger seed/reseed or the user

This helps separate Roger-controlled continuity prompts from later human steering.

#### `preToolUse`

Enforce policy and audit:

- deny raw GitHub write commands
- deny dangerous shell writes in review mode
- deny file writes by default in review mode
- deny broad path escapes
- deny external URL access by default
- deny memory writes as a substitute for Roger state
- log attempted tool name and args digest

#### `postToolUse`

Log successful tool results and attach summarized audit artifacts where useful.

#### `agentStop`

Capture `transcriptPath` and related end-of-turn metadata. Use this as the primary handoff point for raw output references.

#### `sessionEnd`

Mark session completion reason and finalize the provider-side audit envelope.

## W5.6 Use Copilot's local session data, but do not let it become the authority

Copilot records local session data and a local session store. Roger should use that only as harness-side continuity evidence.

### Roger should store

- Copilot session ID
- session-state directory reference or derived artifact reference
- transcript path reference(s)
- continuity probe results
- hook audit references
- prompt pack digests

### Roger should not do

- treat Copilot's session store as the canonical review history
- let Copilot “memory” replace Roger memory/search state
- rely on experimental session-history features for required correctness

## W5.7 Default Copilot policy profile for review mode

Recommended default profile: `review_readonly`

### Allowed by default

- read-oriented file inspection
- Roger-owned seed and reseed prompts
- prompt planning / reasoning inside the provider
- repository custom instructions and path-specific instructions
- hook-driven auditing

### Disabled or denied by default

- built-in GitHub MCP server
- all broad MCP access not explicitly allowed by Roger
- shell execution
- write tool
- external URL access
- provider memory writes
- “allow all” / “yolo” permissions
- raw `gh` write paths
- remote delegation / PR creation behaviors not mediated by Roger

### Optional later profile

`review_shell_readonly` may allow tightly constrained shell commands, but only after the deny-list and allow-list rules are test-backed. It should not be the first shipped profile.

## W5.8 Keep worktree isolation central for Copilot

Copilot's trust model is directory-scoped and heuristic. That makes Roger's worktree isolation **more important**, not less important.

**Decision:** always launch Roger-managed Copilot sessions from the resolved repo/worktree root, never from `$HOME` or an overly broad ancestor directory.

Worktree identity should be part of:

- the Copilot `SessionLocator`
- the continuity probe
- the stale-binding invalidation rules
- the operator-visible status surface

## W5.9 Add explicit operator guidance for Copilot

Roger docs should tell the truth about prerequisites:

- Copilot CLI installation
- authentication / policy enablement
- trusted-directory implications
- review-mode restrictions Roger imposes
- what Roger captures locally
- what Roger does **not** allow Copilot to do

Recommended first-pass quickstart for maintainers:

1. install Copilot CLI
2. authenticate with Copilot
3. open the repo root (or Roger worktree root)
4. run `rr review --provider copilot`
5. verify Roger captures a real Copilot session ID and hook audit trail
6. use `rr resume` / `rr return` only after the Tier B path lands

---

## W6. Testing plan

## Principles

- do not spend the E2E budget casually
- add deterministic lower-layer coverage first
- add provider-acceptance suites before adding another big end-to-end test
- only claim a provider capability that has matching acceptance coverage

## W6.1 Add deterministic test doubles for Copilot

Create a fake Copilot binary fixture that can simulate:

- interactive start
- `--resume`
- hook payload emission
- local session-state files
- transcript paths
- policy violations
- auth failure
- missing binary
- stale locator
- crash between launch and final binding

This allows reliable PR/gated coverage without requiring a paid or licensed live Copilot environment on every CI worker.

## W6.2 Add provider acceptance suites for Copilot

Minimum deterministic coverage:

- `start_session` writes a verified session ID before Roger commits final state
- `seed_from_resume_bundle` creates a fresh session and preserves Roger continuity
- `reopen_by_locator` works for a valid session ID
- stale locator downgrades truthfully to degraded or reseed
- raw output capture resolves transcript references correctly
- pre-tool policy blocks forbidden commands
- built-in MCP servers are disabled in review mode
- no write/tool/url access is silently allowed
- worktree mismatch fails closed

## W6.3 Add transaction and crash-recovery tests

Critical new tests:

- crash between provider launch and Roger commit
- crash after artifact write but before session binding
- crash during return/rebind
- repeated retry after partial failure
- stale hook event from prior launch attempt does not bind to the wrong Roger session

These tests matter more than another monolithic E2E.

## W6.4 Add a real-Copilot smoke lane only when the support claim warrants it

Once Copilot is marketed as first-class, add one real boundary smoke path in a licensed environment. It does **not** need to run on every PR, but it does need to exist and be run in a clearly documented lane.

Recommended scope:

- install/auth preflight
- start a review session
- verify real session ID capture
- resume by session ID
- confirm a denied write/MCP action is actually denied
- capture transcript artifact
- close session cleanly

If that lane cannot run, Roger should narrow the support claim until it can.

## W6.5 Keep the E2E budget honest

Do **not** add another heavyweight automated E2E just because Copilot arrives. Most Copilot integration risk should be covered by:

- provider acceptance suites
- transaction/crash tests
- one real-provider smoke lane
- release-smoke checklist items

---

## W7. Operational safeguards and engineering controls

## W7.1 Add provider-agnostic `rr doctor`

Implement a provider-aware doctor surface:

- `rr doctor`
- `rr doctor --provider opencode`
- `rr doctor --provider copilot`
- `rr doctor --provider codex`
- `rr doctor --provider gemini`

For Copilot, doctor should validate at least:

- binary present
- current cwd is a repo/worktree root, not a broad ancestor
- Roger hook files present and parseable
- Roger custom instruction files present
- store/bootstrap state present
- launch policy profile resolvable

If auth cannot be checked non-invasively, doctor should say “auth not preflight-verified; first launch will confirm and fail closed if needed.”

## W7.2 Add operator-safe bootstrap

Implement or reconcile:

- `rr init`

Baseline responsibilities:

- create local Roger store if missing
- write initial metadata/version markers
- verify file permissions
- optionally emit provider guidance (`copilot login`, `opencode` install, etc.)

## W7.3 Add audit artifact classes

Introduce or formalize provider audit artifact classes such as:

- launch attempt envelope
- hook event capture
- tool-denial event
- provider transcript reference
- continuity probe result

These should be digested and referenced from Roger's store, not scattered as ad hoc logs.

## W7.4 Add policy digests to status output

Expose enough information for operators to know **which** policy was active for a session:

- provider
- policy profile name
- policy digest or version
- hook profile version
- custom-instructions digest
- built-in MCP disabled/enabled
- worktree root

This is especially important once Copilot joins the matrix.

---

## W8. Suggested file changes

## New files

- `packages/session-copilot/src/lib.rs`
- `packages/session-copilot/Cargo.toml`
- `.github/copilot-instructions.md`
- `.github/instructions/rust.instructions.md`
- `.github/instructions/extension.instructions.md`
- `.github/hooks/roger-review.json`
- `scripts/copilot-hooks/session-start.sh`
- `scripts/copilot-hooks/session-start.ps1`
- `scripts/copilot-hooks/user-prompt.sh`
- `scripts/copilot-hooks/pre-tool-use.sh`
- `scripts/copilot-hooks/post-tool-use.sh`
- `scripts/copilot-hooks/agent-stop.sh`
- `scripts/copilot-hooks/session-end.sh`
- `packages/cli/tests/provider_copilot_acceptance.rs`
- `packages/storage/tests/transaction_recovery.rs`
- fixture files for fake Copilot session-state and hook payloads

## Existing files to update

- `Cargo.toml`
- `AGENTS.md`
- `README.md`
- `docs/PLAN_FOR_ROGER_REVIEWER.md`
- `docs/HARNESS_SESSION_LINKAGE_CONTRACT.md`
- `docs/RELEASE_AND_TEST_MATRIX.md`
- `docs/REVIEW_FLOW_MATRIX.md`
- `docs/TEST_HARNESS_GUIDELINES.md`
- `docs/DEV_MACHINE_ONBOARDING.md`
- `packages/app-core/src/lib.rs`
- `packages/storage/src/lib.rs`
- `packages/config/src/lib.rs`
- `packages/cli/src/lib.rs`
- `packages/bridge/src/lib.rs`
- `packages/session-opencode/src/lib.rs`
- `packages/session-codex/src/lib.rs`
- `packages/session-gemini/src/lib.rs`
- `packages/test-harness/...`
- any validation metadata / suite registration files

---

## W9. Recommended slice order

## Slice 1 — docs and truthfulness scaffolding

Land on `main` early:

- this planning doc
- provider matrix updates
- support-claim wording cleanup
- launch-attempt state machine design
- `rr init` / `rr doctor` decision
- Copilot provider admitted into the plan and docs as proposed work

**Why first:** it lets the implementation proceed against a stable, explicit target.

## Slice 2 — transactional lifecycle and bridge realism

Land next:

- storage transactions
- verified launch gating
- bridge actual dispatch
- bootstrap/doctor command surface reconciliation

**Gate to next slice:** no synthetic launch success remains.

## Slice 3 — OpenCode parity completion

Land next:

- real OpenCode return
- real continuity probe
- worktree enforcement in the live path

**Gate to next slice:** the blessed path is actually blessed, not just described that way.

## Slice 4 — Copilot Tier A behind a feature flag

Land next:

- `session-copilot` crate
- CLI provider parsing
- Copilot hooks and instructions
- verified interactive launch
- raw capture
- `ResumeBundle` reseed
- deterministic acceptance coverage

Recommended temporary gate:

- `RR_ENABLE_EXPERIMENTAL_COPILOT=1`

**Gate to next slice:** Tier A is truthful and test-backed.

## Slice 5 — Copilot Tier B and first-class status

Land next:

- reopen by real session ID
- continuity probe
- return/rebind
- operator docs and smoke lane
- remove experimental gate once the support claim is honest

## Slice 6 — outbound flow completion and broader polish

Land next:

- explicit CLI approval/posting workflow
- provider-neutral audit/artifact polish
- Codex/Gemini narrowing or cleanup
- status/doctor/reporting refinements

## Slice 7 — ACP spike (optional)

Only after the above are stable:

- ACP proof-of-concept
- structured multi-agent or in-harness command experiments
- possible Tier C evaluation

---

## W10. Main-branch landing strategy

Preferred landing path:

1. land this plan doc directly on `main` as a docs-only commit
2. land implementation in small, reviewable slices on `main`, behind feature flags where needed
3. avoid a giant long-lived “provider parity” branch

### Why direct-to-main is preferred here

This repo already has a dense planning and contract culture. A docs-only commit that establishes the implementation target is low risk and high leverage. It should not need a massive branch.

### When to use a PR instead

Use a PR if:

- branch protection blocks direct pushes to `main`
- the implementation slice is large enough to need dedicated review
- the change touches contracts and multiple packages at once
- a release-support claim would change immediately on merge and you want explicit sign-off

### Recommended commit sequence

#### Commit 1
`docs: add provider parity and GitHub Copilot CLI implementation plan`

#### Commit 2
`core: make launch lifecycle transactional and verified`

#### Commit 3
`bridge: dispatch real rr robot commands and return canonical session ids`

#### Commit 4
`opencode: finish truthful tier-b resume and return`

#### Commit 5
`copilot: add tier-a provider integration with hooks and instructions`

#### Commit 6
`copilot: add tier-b resume and return support`

#### Commit 7
`cli: expose explicit outbound approval and posting flow`

---

## Acceptance checklist for the overall program

Roger is “where it needs to be” for this program when all of the following are true:

- [ ] Roger never reports a launched review session before a verified provider session exists
- [ ] lifecycle writes are transactional and crash-safe
- [ ] bridge launch returns real Roger session IDs
- [ ] bootstrap/doctor/init guidance is reconciled
- [ ] OpenCode truthfully satisfies its claimed tier
- [ ] Codex and Gemini are described no more generously than they deserve
- [ ] Copilot is present in the canonical plan, README, onboarding, and test matrix
- [ ] `rr review --provider copilot` launches a verified session with a real Copilot session ID
- [ ] Copilot raw output capture is durable and audit-backed
- [ ] Copilot default review mode cannot silently write files, call raw `gh`, or use GitHub MCP writes
- [ ] Copilot can truthfully reseed from a `ResumeBundle`
- [ ] Copilot can truthfully reopen by locator before Roger claims Tier B
- [ ] `rr return` works for Copilot before Roger claims Tier B
- [ ] worktree identity is part of provider binding and stale-binding failure
- [ ] outbound draft/approve/post flow is fully productized
- [ ] provider acceptance and transaction/crash coverage exist for the new claims
- [ ] at least one real-provider smoke lane exists for each first-class external surface that Roger claims

---

## Reference notes for implementation

### Roger repo references

- Current repo root and default branch: `https://github.com/cdilga/roger-reviewer`
- `AGENTS.md`
- `docs/PLAN_FOR_ROGER_REVIEWER.md`
- `docs/HARNESS_SESSION_LINKAGE_CONTRACT.md`
- `docs/RELEASE_AND_TEST_MATRIX.md`
- `docs/REVIEW_FLOW_MATRIX.md`
- `README.md`

### GitHub Copilot CLI references

- About GitHub Copilot CLI: `https://docs.github.com/en/copilot/concepts/agents/copilot-cli/about-copilot-cli`
- Installing GitHub Copilot CLI: `https://docs.github.com/en/copilot/how-tos/copilot-cli/set-up-copilot-cli/install-copilot-cli`
- Using GitHub Copilot CLI: `https://docs.github.com/en/copilot/how-tos/copilot-cli/use-copilot-cli-agents/overview`
- Running GitHub Copilot CLI programmatically: `https://docs.github.com/en/copilot/how-tos/copilot-cli/automate-copilot-cli/run-cli-programmatically`
- GitHub Copilot CLI command reference: `https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference`
- Hooks configuration: `https://docs.github.com/en/copilot/reference/hooks-configuration`
- Using hooks with Copilot CLI: `https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/use-hooks`
- Using hooks with Copilot CLI for predictable, policy-compliant execution: `https://docs.github.com/en/copilot/tutorials/copilot-cli-hooks`
- Adding repository custom instructions: `https://docs.github.com/en/copilot/how-tos/configure-custom-instructions/add-repository-instructions`
- About GitHub Copilot CLI session data: `https://docs.github.com/en/copilot/concepts/agents/copilot-cli/chronicle`
- Using GitHub Copilot CLI session data: `https://docs.github.com/en/copilot/how-tos/copilot-cli/chronicle`
- About Model Context Protocol (MCP): `https://docs.github.com/en/copilot/concepts/context/mcp`
- Copilot CLI ACP server: `https://docs.github.com/en/copilot/reference/copilot-cli-reference/acp-server`

---

## Bottom line

The right order is:

1. make Roger's lifecycle truthful and atomic
2. finish the provider claims Roger already makes
3. add Copilot as a serious provider through the same harness contract
4. keep safety stronger than convenience the entire way

That sequence gets Roger to a state where it can honestly say it is a durable, local-first review system with explicit approval gates and more than one serious terminal-native provider.
