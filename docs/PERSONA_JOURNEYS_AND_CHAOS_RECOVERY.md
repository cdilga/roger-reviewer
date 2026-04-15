Status: support matrix / persona scenario artifact
Audience: maintainers shaping user-facing flows, acceptance criteria, integration
coverage, and E2E candidates
Primary references:

- [`PLAN_FOR_ROGER_REVIEWER.md`](docs/PLAN_FOR_ROGER_REVIEWER.md)
- [`REVIEW_FLOW_MATRIX.md`](docs/REVIEW_FLOW_MATRIX.md)
- [`TEST_HARNESS_GUIDELINES.md`](docs/TEST_HARNESS_GUIDELINES.md)
- [`RELEASE_AND_TEST_MATRIX.md`](docs/RELEASE_AND_TEST_MATRIX.md)
- [`VALIDATION_INVARIANT_MATRIX.md`](docs/VALIDATION_INVARIANT_MATRIX.md)

# Persona Journeys And Chaos Recovery

This document captures six first-class Roger user journeys in user-facing
language.

It exists to do four things:

- make Roger easier to reason about from the user point of view
- give beads and support contracts stable persona and scenario ids
- give integration and E2E planning a set of human-readable story cuts
- force failure, restart, corruption, and recovery behavior into the same
  product story instead of leaving them as invisible implementation details

This is intentionally not an implementation walkthrough. It should read like
what the user is trying to do, what goes wrong, and what Roger must do to get
the user back on track.

## Stable reference model

- `PJ-01` through `PJ-06` are persona families
- `PJ-01A`, `PJ-01B`, and similar ids are stable scenario cuts inside a persona
- beads, acceptance criteria, suite metadata, and manual smoke notes may cite
  either a persona family or a specific scenario cut
- when a scenario cut changes materially, update this document together with
  [`REVIEW_FLOW_MATRIX.md`](docs/REVIEW_FLOW_MATRIX.md)

## How to use this artifact

- cite `PJ-01` through `PJ-06` when a task depends on a whole user journey
- cite `PJ-01A` style scenario ids when a task or test depends on one specific
  failure or recovery branch
- keep the language user-facing; surface-specific detail belongs in the flow
  matrix, support contracts, or suite metadata
- when proposing a new E2E or high-value integration path, start with one
  scenario cut from this document, then map it to flow ids and invariant ids

## PJ-01: Nina, The First-Time Browser Adopter

**Who she is**

- Nina wants Roger to appear on GitHub PR pages without learning Roger
  internals.
- She accepts one unavoidable browser approval step.
- She will not tolerate a setup ritual that feels like local infrastructure
  work.

**Core promise**

- setup should feel like one action with one unavoidable approval, not a chain
  of maintenance commands

**Stable scenario cuts**

| Scenario id | Story cut | Chaos | What Roger must do |
|-------------|-----------|-------|--------------------|
| `PJ-01A` | First successful browser setup and first PR launch | no chaos | Keep setup short, finish automatically, and make `Start` on the PR page feel real |
| `PJ-01B` | Setup interrupted halfway through | browser step closed early, Roger restarted, or user abandons setup and comes back later | Let Nina rerun the same primary setup path and continue safely |
| `PJ-01C` | Setup looks complete but the PR page cannot reach Roger | extension is present, but the bridge or local Roger path still is not usable | Fail clearly, reuse the same repair path, and avoid leaking internal identifiers |
| `PJ-01D` | Unsupported browser or unsupported installation mode | Nina tries a browser Roger does not truly support | Say so plainly and point to the supported path instead of pretending it is close enough |

**Primary path**

1. Nina installs Roger locally.
2. She runs the one Roger setup command for the browser companion.
3. Roger leads her to the one browser approval step that cannot be skipped.
4. She approves Roger in the browser.
5. Roger finishes the rest and says the browser path is ready.
6. Nina opens a PR, sees Roger on the page, clicks `Start`, and lands in a
   local Roger review.

**Chaos and restart variants**

- Nina closes the browser approval screen by mistake.
- The browser update completes, but Roger is no longer reachable from the PR.
- Roger quits or is upgraded during setup.
- Nina repeats setup because she is unsure whether it worked.

**Recovery checkpoints**

- Nina should be able to repeat the same setup command without fear.
- Roger should tell her whether the browser path is ready, still incomplete, or
  unsupported.
- Roger should not require Nina to discover extension ids, file paths, or
  bridge terminology in the normal flow.

**Must not happen**

- setup success is claimed before the browser path actually works
- Nina is told to learn a second repair vocabulary just to finish first-time
  setup
- the happy path quietly depends on browser-specific tribal knowledge

**Primary flow links**

- `F02.1 Guided Browser Setup And Verification`
- `F02 Launch from GitHub PR Page`

**Likely proof cuts**

- `PJ-01A` is a high-value browser happy-path candidate
- `PJ-01B` and `PJ-01C` are stronger as integration or operator-smoke recovery
  cuts than as heavyweight E2Es

## PJ-02: Marco, The Browser-First Repeat Reviewer

**Who he is**

- Marco lives on GitHub PR pages.
- He expects Roger to feel like the fastest doorway back to ongoing review
  work.
- He does not want to think about local session bookkeeping.

**Core promise**

- returning to an existing review should feel fast, honest, and unsurprising

**Stable scenario cuts**

| Scenario id | Story cut | Chaos | What Roger must do |
|-------------|-----------|-------|--------------------|
| `PJ-02A` | Resume the obvious existing review from the PR page | no chaos | Offer `Resume` clearly and reconnect Marco to the same review |
| `PJ-02B` | Several plausible local reviews exist for the same PR or repo | Marco cannot tell which prior review is the right one | Stop guessing and show one clear choice path |
| `PJ-02C` | Browser entry exists, but the convenient path is unavailable today | extension or local handoff fails even though it worked before | Give Marco one obvious local recovery path back into Roger |
| `PJ-02D` | The PR changed enough that the old review cannot be treated as fully current | new commits or major drift since the last session | Preserve continuity, but steer Marco toward refresh or a new pass truthfully |

**Primary path**

1. Marco returns to a PR he has already reviewed.
2. Roger is visible on the PR page.
3. Roger offers the next safe action, usually `Resume`.
4. Marco chooses it.
5. Roger brings him back to the existing local review instead of starting over.
6. He continues with his existing local findings, notes, and continuity.

**Chaos and restart variants**

- Marco was reviewing days ago and now has several similar local sessions.
- The last coding environment is gone.
- The PR page entry appears, but clicking it does not get him back to work.
- Marco is no longer on the original repo directory when he tries again.

**Recovery checkpoints**

- when Roger knows the strongest match, it should take Marco there directly
- when Roger is genuinely unsure, it should expose the ambiguity instead of
  hiding it
- the browser must never be the only path back into the same review
- if the old review is stale, Roger should say so plainly rather than
  pretending continuity means freshness

**Must not happen**

- Roger silently chooses the wrong old session
- a broken browser path traps Marco away from his local review
- Marco has to reverse-engineer local state names to recover

**Primary flow links**

- `F01.1 Reinvoke Roger in the Current Repo and Pick Up the Right Session`
- `F01.2 Global Session Finder and Cross-Repo Jump`
- `F02 Launch from GitHub PR Page`

**Likely proof cuts**

- `PJ-02A` is a strong browser resume journey
- `PJ-02B` and `PJ-02C` are strong ambiguity and fallback recovery cuts
- `PJ-02D` is a high-value refresh-truthfulness or stale-continuity cut

## PJ-03: Priya, The Terminal-First Reviewer

**Who she is**

- Priya starts from the repo and expects Roger to be a real local product.
- She does not depend on the browser.
- She expects crisp answers when she asks Roger what is going on.

**Core promise**

- Roger should be fully useful from the shell, including restart and
  re-entry

**Stable scenario cuts**

| Scenario id | Story cut | Chaos | What Roger must do |
|-------------|-----------|-------|--------------------|
| `PJ-03A` | Start a review from the shell and continue locally | no chaos | Launch a local review cleanly and keep the session durable |
| `PJ-03B` | Context is missing or ambiguous at launch time | missing PR, several candidate reviews, or wrong directory | Block clearly or offer a chooser instead of guessing |
| `PJ-03C` | Priya loses the terminal and comes back later | terminal closes, laptop sleeps, or shell history is gone | Let her resume the same work without spawning mystery sessions |
| `PJ-03D` | Priya comes back from a different directory or a second terminal | local context is no longer anchored to the original working shell | Preserve navigability and session discovery |

**Primary path**

1. Priya is in the repo locally.
2. She starts a Roger review for the PR she cares about.
3. Roger routes her into the right local review surface.
4. Later, she asks Roger for status.
5. Roger tells her what session it means, what it needs, and whether findings or
   drafts are waiting.
6. She asks for findings and continues from there.

**Chaos and restart variants**

- Priya forgets to specify enough context for Roger to know which PR she means.
- She has several active local sessions.
- Her shell closes right after launch.
- She returns later from another terminal or another working directory.

**Recovery checkpoints**

- Roger should block clearly when it lacks enough context
- Roger should offer session choice when several paths look plausible
- rerunning the same local command should recover work more often than it
  fragments it
- Priya should always have a crisp local answer to "what session is this?" and
  "what does it need from me?"

**Must not happen**

- the browser becomes a hidden dependency for core review work
- Roger creates duplicate mystery sessions because Priya retried after an
  interruption
- status answers are vague enough that Priya still cannot tell what to do next

**Primary flow links**

- `F01 Enter or Resume a Review Locally`
- `F01.1 Reinvoke Roger in the Current Repo and Pick Up the Right Session`

**Likely proof cuts**

- `PJ-03A` is the strongest local-first E2E seed
- `PJ-03B`, `PJ-03C`, and `PJ-03D` are high-value continuity and ambiguity
  integration cuts

## PJ-04: Dante, The Interrupted Reviewer After A Crash

**Who he is**

- Dante was already reviewing when something went wrong.
- He cares less about perfect continuity than about not losing his place.
- He expects Roger to tell the truth after a crash instead of masking it.

**Core promise**

- a crash should degrade Roger, not erase the review

**Stable scenario cuts**

| Scenario id | Story cut | Chaos | What Roger must do |
|-------------|-----------|-------|--------------------|
| `PJ-04A` | Recover after a plain crash or reboot | terminal gone, laptop rebooted, or review surface disappeared | Find the prior review and resume or explain the degraded path |
| `PJ-04B` | The live coding session is gone, but the review record survives | provider or harness continuity is stale | Preserve the local review and keep Dante moving from truthful local state |
| `PJ-04C` | Roger crashed during an in-flight update, refresh, or structure pass | work may be partially complete or partially written | Show what survived, what is uncertain, and what should be retried |
| `PJ-04D` | Dante is unsure whether the old findings are still safe to trust | crash plus PR drift or partial refresh | Make trust boundaries legible and prefer explicit recovery over silent reuse |

**Primary path**

1. Dante had already started a Roger review.
2. His machine or working session dies unexpectedly.
3. Later, he comes back and asks Roger to resume or show status.
4. Roger finds the prior review session and shows what survived.
5. If the live connection is still usable, Roger reconnects to it.
6. If not, Roger still preserves the local review record and gets Dante moving
   again with a truthful degraded path.

**Chaos and restart variants**

- the live coding session is gone
- the local review exists, but the old live connection is stale
- Roger died during a refresh or a review pass
- Dante restarts twice because he cannot tell whether the first recovery worked

**Recovery checkpoints**

- Roger must prefer continuity over amnesia
- Roger must tell Dante which parts survived and which parts need to be
  regenerated
- a second restart should not make the situation more confusing than the first
- if the review is no longer fully trustworthy, Roger must say so plainly

**Must not happen**

- a crash silently turns into a fresh unrelated review
- Roger pretends a half-recovered review is fully healthy
- Dante is forced to abandon the review just because one live process died

**Primary flow links**

- `F01 Enter or Resume a Review Locally`
- `F05 Request Follow-Up or Provide Input`
- `F06 Refresh After New Commits`

**Likely proof cuts**

- `PJ-04A` and `PJ-04B` are strong continuity and degraded-recovery cuts
- `PJ-04C` and `PJ-04D` are strong crash-truthfulness and restart-honesty cuts

## PJ-05: Elena, The Cautious Approver

**Who she is**

- Elena wants Roger's help but refuses to let any tool post remotely without a
  final human decision.
- She expects local drafting, explicit approval, and visible posting results.
- She is sensitive to changes that should invalidate trust.

**Core promise**

- no hidden leap from local draft to remote posting

**Stable scenario cuts**

| Scenario id | Story cut | Chaos | What Roger must do |
|-------------|-----------|-------|--------------------|
| `PJ-05A` | Findings become local drafts, then an approved batch, then posted output | no chaos | Keep the local-first, approval-gated sequence obvious |
| `PJ-05B` | Elena approved, then the PR changed before posting | new commits or changed thread context | Invalidate trust where needed and require a fresh decision |
| `PJ-05C` | Posting partly succeeds and partly fails | some comments land, some do not | Preserve a durable split view of what posted, what failed, and what still needs action |
| `PJ-05D` | Roger crashes after approval or during posting | Elena does not know whether anything went out | Reconstruct the truth safely before allowing another post attempt |

**Primary path**

1. Elena reviews local findings.
2. Roger turns chosen findings into local draft comments.
3. Roger makes it obvious that the drafts still live locally.
4. Elena edits, removes, or groups them as needed.
5. She explicitly approves the batch.
6. Only then does Roger post them and preserve a local audit trail.

**Chaos and restart variants**

- new commits land after approval
- posting partly succeeds and partly fails
- remote conversation context changes while Elena is deciding
- Roger crashes after approval but before Elena sees a final result

**Recovery checkpoints**

- approval should not silently survive trust-breaking changes
- partial posting must stay visible instead of collapsing into one vague result
- Elena should be able to tell what happened before she decides to retry
- retry must feel deliberate, not automatic

**Must not happen**

- Roger posts just because Elena looked at something that seemed ready
- approval remains valid after a meaningful trust boundary changed
- a crash leaves Elena unable to tell whether Roger already posted

**Primary flow links**

- `F07 Draft Review, Approval, and Posting`
- `F08 Inspect History, Original Pack, and Raw Output`

**Likely proof cuts**

- `PJ-05A` is central to the local-draft and approval-gated core loop
- `PJ-05B`, `PJ-05C`, and `PJ-05D` are high-value failure and invalidation
  cuts

## PJ-06: Riley, The Damage-Control Operator

**Who she is**

- Riley arrives when something is already wrong.
- She does not want to become Roger's maintainer.
- She wants one bounded repair path that gets her back to trustworthy work.

**Core promise**

- Roger should stay opinionated under stress and recover through the simplest
  safe path

**Stable scenario cuts**

| Scenario id | Story cut | Chaos | What Roger must do |
|-------------|-----------|-------|--------------------|
| `PJ-06A` | Browser or launch path drift after update | the PR page entry no longer opens local Roger correctly | Reuse the main setup or repair path instead of inventing a second maze |
| `PJ-06B` | Duplicate or stale sessions make the next step unclear | Riley sees several possible local reviews and cannot trust any of them automatically | Offer a clear chooser and explain enough to make a safe decision |
| `PJ-06C` | Local review state is damaged, stale, or migration-sensitive | Roger cannot trust some part of the local store | Fail closed, preserve what can be preserved, and point Riley to one safe next step |
| `PJ-06D` | Riley restarts twice and still needs to know whether Roger is healthy again | repair is in progress or only partly successful | Tell the truth about current health instead of looping vague advice |

**Primary path**

1. Riley notices Roger is not behaving normally.
2. She asks Roger for the simplest supported recovery path.
3. Roger tells her whether this is a browser setup issue, a session-selection
   issue, or a local continuity issue.
4. Riley follows one bounded repair path.
5. Roger either returns to normal operation or clearly says which trust
   boundary is still broken.
6. She gets back to reviewing instead of living in repair mode.

**Chaos and restart variants**

- the browser companion worked before an update and no longer does
- the PR page entry exists, but nothing local opens
- duplicate or stale sessions make it unsafe to guess
- Roger says some local review state is damaged or cannot be trusted
- Riley restarts Roger and wants to know whether that actually fixed anything

**Recovery checkpoints**

- Roger should prefer one obvious repair path over a branching tree of obscure
  commands
- browser repair should still flow through the same primary setup entrypoint
- session ambiguity should open a clear chooser rather than producing silent
  wrong answers
- if local state cannot be trusted, Roger should say so directly and fail
  closed

**Must not happen**

- failure dumps Riley into internal jargon and operator-only concepts
- Roger hides damage or uncertainty just to keep the surface looking simple
- restarting Roger makes recovery less legible instead of more legible

**Primary flow links**

- `F02.1 Guided Browser Setup And Verification`
- `F01.2 Global Session Finder and Cross-Repo Jump`
- `F08 Inspect History, Original Pack, and Raw Output`

**Likely proof cuts**

- `PJ-06A` is a strong update-drift and repair entry scenario
- `PJ-06B` is a strong ambiguity and safe-choice scenario
- `PJ-06C` and `PJ-06D` are strong corruption, fail-closed, and restart-truth
  scenarios

## Ownership reconciliation for PJ-04 through PJ-06

This reconciliation makes the recovery-heavy persona families point at concrete
product owners instead of leaving them spread only across flow and testing
docs.

Closed baseline beads remain authoritative current owners until a newer Round 06
hardening bead replaces them. This pass does not create additional follow-on
beads because the remaining executable and hardening gaps already have explicit
owners in `rr-6iah.1` through `rr-6iah.5`, `rr-x51h.8.2`, `rr-x51h.8.4`,
`rr-8isd.5.2`, `rr-8isd.5.3`, `rr-g7j6.8`, `rr-ph77.5`, `rr-1pz7`, and
`rr-x51h.9.1`.

Shared degraded-recall rule:

- when recovery drops into recovery-oriented search instead of normal recall,
  `rr-x51h.9.1` owns the requirement that `recovery_scan` stays explicit and
  degraded across the affected `PJ-04` and `PJ-06` paths

| Scenario id | Primary product owners | Current proof lane | Recovery truth carried here |
|-------------|------------------------|--------------------|-----------------------------|
| `PJ-04A` | `rr-003.3`, `rr-003.8`, `rr-x51h.3.2` | `rr-6iah.1`, `rr-6iah.5` | crash or reboot resumes the same review when possible and degrades to explicit reseed when not |
| `PJ-04B` | `rr-003.3`, `rr-003.4`, `rr-x51h.3.2` | `rr-6iah.1`, `rr-6iah.5` | local review state survives even when live harness continuity is stale or gone |
| `PJ-04C` | `rr-004.1`, `rr-016.3`, `rr-x51h.3.2` | `rr-011.3`, `rr-x51h.8.2` | partial refresh or structure work preserves raw artifacts, bounded repair state, and retry-safe lifecycle truth |
| `PJ-04D` | `rr-011.2`, `rr-x51h.3.2`, `rr-x51h.5.2` | `rr-6iah.3`, `rr-x51h.8.2` | restart after drift or partial refresh keeps stale trust boundaries explicit instead of silently reusing old findings or drafts |
| `PJ-05A` | `rr-ph77.1`, `rr-x51h.5.1`, `rr-1pz7` | `E2E-01` | draft, approve, and post remain one Roger-mediated path rather than ambient GitHub mutation |
| `PJ-05B` | `rr-ph77.1`, `rr-x51h.5.2` | `rr-011.2`, `rr-011.4`, `rr-6iah.3`, `rr-x51h.8.4` | approvals revoke automatically on refresh, rebase, retarget, or drift |
| `PJ-05C` | `rr-008.1`, `rr-ph77.5`, `rr-1pz7` | `rr-011.4`, `rr-x51h.5.2`, `rr-x51h.8.4` | partial post preserves exact posted-versus-pending lineage and a safe retry path |
| `PJ-05D` | `rr-ph77.5`, `rr-x51h.5.1`, `rr-1pz7` | `rr-011.4`, `rr-x51h.5.2`, `rr-x51h.8.4` | crash after approval or during posting must reconstruct exact remote and local truth before another post attempt |
| `PJ-06A` | `rr-8isd.5.1`, `rr-8isd.5.2`, `rr-8isd.5.3`, `rr-b58q.4.4` | `rr-6iah.4` | update or setup drift routes back through `rr init` and `rr doctor`, not ad hoc bridge repair paths |
| `PJ-06B` | `rr-005.2`, `rr-005.2.1`, `rr-009.1` | `rr-011.6` | duplicate or stale sessions fail closed into an explicit chooser instead of silent guesswork |
| `PJ-06C` | `rr-1xhg.2`, `rr-1xhg.3`, `rr-1xhg.4`, `rr-1xhg.5` | `rr-8isd.5.2` | damaged or migration-sensitive local state fails closed with checkpoint or journal evidence and one safe next step |
| `PJ-06D` | `rr-8isd.5.2`, `rr-g7j6.8`, `rr-8isd.5.3` | `rr-1xhg.5`, `rr-x51h.8.2` | restart after partial repair must say whether health is verified, deferred to first launch, or still blocked |

## Cross-journey expectations

These should feel true across all six personas:

- there is always one obvious next step
- Roger absorbs internal complexity instead of explaining it to ordinary users
- repeating a primary command or action should usually recover work rather than
  fragment it
- crashes, stale sessions, partial posting, and damaged local state should
  degrade truthfully rather than erase continuity
- browser convenience must never become the only path back into the review
- Roger should be allowed to say "I cannot trust this yet" when that is the
  safest answer

## Scenario index for validation planning

| Scenario id | Summary | Flow links | Likely cheapest truthful proof |
|-------------|---------|------------|--------------------------------|
| `PJ-01A` | First-time browser setup and first PR launch | `F02.1`, `F02` | integration or operator smoke, with future E2E potential |
| `PJ-01B` | Interrupted setup and safe retry | `F02.1` | integration |
| `PJ-01C` | Setup appears complete but PR page still cannot reach Roger | `F02.1`, `F02` | integration |
| `PJ-01D` | Unsupported browser path fails honestly | `F02.1` | integration |
| `PJ-02A` | Obvious browser resume | `F02`, `F01.1` | integration |
| `PJ-02B` | Ambiguous resume requires explicit choice | `F01.1`, `F01.2`, `F02` | integration |
| `PJ-02C` | Browser unavailable, local recovery still works | `F02`, `F01.1` | integration |
| `PJ-02D` | Resume with PR drift requires truthful freshness handling | `F02`, `F06` | integration |
| `PJ-03A` | Local-first review happy path | `F01` | `E2E-01` seed |
| `PJ-03B` | Local launch blocked by missing or ambiguous context | `F01`, `F01.1` | integration |
| `PJ-03C` | Local restart after shell loss | `F01`, `F01.1` | integration |
| `PJ-03D` | Re-entry from another terminal or directory | `F01.1`, `F01.2` | integration |
| `PJ-04A` | Crash or reboot recovery | `F01` | integration |
| `PJ-04B` | Live session gone, local review survives | `F01`, `F05` | integration |
| `PJ-04C` | Crash during in-flight review update | `F05`, `F06` | integration |
| `PJ-04D` | Restart after crash with uncertain trust boundary | `F06`, `F08` | integration or release smoke |
| `PJ-05A` | Local draft, approval, and posting happy path | `F07` | `E2E-01` seed |
| `PJ-05B` | Approval invalidated by new commits | `F06`, `F07` | integration |
| `PJ-05C` | Partial posting remains visible and recoverable | `F07`, `F08` | integration |
| `PJ-05D` | Crash after approval or during posting | `F07`, `F08` | integration or release smoke |
| `PJ-06A` | Update drift repair through the main setup path | `F02.1` | integration |
| `PJ-06B` | Duplicate or stale sessions require a safe chooser | `F01.2` | integration |
| `PJ-06C` | Local state damage or corruption fails closed | `F08` | integration or release smoke |
| `PJ-06D` | Restart-truth after partial repair | `F02.1`, `F08` | integration |

## Suggested test-usage rule

When designing a new high-value integration test, release smoke, or future E2E:

1. start from one `PJ-xxY` scenario id in this document
2. map it to one or more flow ids in
   [`REVIEW_FLOW_MATRIX.md`](docs/REVIEW_FLOW_MATRIX.md)
3. name the relevant invariant ids from
   [`VALIDATION_INVARIANT_MATRIX.md`](docs/VALIDATION_INVARIANT_MATRIX.md)
4. choose the cheapest truthful proof lane before escalating to E2E
5. if a scenario is proposed as a heavyweight E2E, explain why lower-level
   integration cuts do not defend the same promise cheaply enough

This keeps user-facing stories, flow families, invariant ownership, and proof
cost tied together.
