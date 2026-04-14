# NTM Command Palette (`command_palette.md`)
#
# Install into ~/.config/ntm/command_palette.md, or symlink it there with:
#   ./scripts/swarm/install_ntm_palette.sh
#
# NTM also checks ./command_palette.md in the current project directory.
#
# Format:
#   ## Category Name
#   ### command_key | Display Label
#   Prompt text...

## Roger Swarm

### default_new_agent | Default New Agent
Read `AGENTS.md` first, then `README.md`, then re-anchor on `docs/PLAN_FOR_ROGER_REVIEWER.md` enough to understand the authority order, current implementation phase, local-core-first architecture direction, and support-claim truthfulness model. If you are shaping beads, writing prompts, or recovering from prior partial closeouts, also read `docs/beads/BEAD_AND_PROMPT_FAILURE_PATTERNS.md`. Register with Agent Mail, check and acknowledge your inbox, then inspect `br ready`, verify the best candidate with `br show <id>`, claim it, reserve files, and start useful work immediately. Finish beads truthfully, not mechanically. Do not drift into communication purgatory.

### reread_agents_md | Reread AGENTS
Reread `AGENTS.md` so the repo rules are fresh again. Reopen the canonical plan sections relevant to your active bead, then check Agent Mail, rerun `br ready`, and continue from the current durable repo state rather than from memory alone.

### next_bead | Next Useful Bead
Check Agent Mail first. Then use `br ready` as queue truth, inspect the best candidate with `br show <id>`, claim it, reserve files, and start coding immediately. Finish the bead truthfully: satisfy the acceptance criteria, but do not stop mechanically if honest closeout also requires a missing child bead, a dependency correction, or a support-claim correction. If the queue looks thin or suspicious, use `bv` only for ranking context and verify with `br`.

### frontier_widening | Widen The Frontier
Check Agent Mail, then inspect `br list --status open`, `br blocked`, and `bv --robot-triage`. Read `docs/beads/BEAD_AND_PROMPT_FAILURE_PATTERNS.md` if the frontier looks suspicious, overlapped, or under-split. If a safe next slice is obvious, split or create the missing bead with one proof boundary, one main validation story, and a truthful closeout contract, then announce it. Do not do speculative work without either claiming or minting the right bead.

## Investigation

### read_agents_and_investigate | Read Agents And Investigate
First read `AGENTS.md` and `README.md` carefully, then re-anchor on the relevant sections of `docs/PLAN_FOR_ROGER_REVIEWER.md`. If the investigation is about execution drift, bead shaping, or misleading closeouts, also read `docs/beads/BEAD_AND_PROMPT_FAILURE_PATTERNS.md`. Then investigate the codebase deeply: trace data flow, inspect the main execution paths, and build a concrete understanding of how the current implementation actually works. Prefer specific code references and real boundaries over vague architectural summaries.

### trace_data_flow | Trace Data Flow
Read `AGENTS.md`, then identify one important user-facing or system-critical flow and trace it end-to-end through the code with precise file references. Explain the current path, seams, state transitions, and any obvious risks or mismatches you find.

### fresh_review | Fresh Review
Review the code you most recently touched with fresh eyes. Look carefully for behavioral regressions, edge cases, missing validation, weak tests, or places where the implementation overclaims support. Fix real problems you confirm and record exact validation evidence.

## Execution

### implement_current_bead | Implement Current Bead
If you already own a bead, reopen `br show <id>` and the relevant plan/support sections, then move immediately from understanding to code changes. If you do not already own one, use `br ready`, verify the best candidate with `br show <id>`, claim it, reserve files, and start editing now. Finish the slice truthfully through code, validation, and closeout notes; do not stop at summaries, TODOs, or partial scaffolding.

### analysis_to_action | Analysis To Action
If you have been reading, tracing, or summarizing for too long, convert that understanding into one concrete implementation move now. Name the bead, the operator-visible promise, and the exact files you will change, reserve those files, then edit code and end this cycle with a real diff plus the validation command you expect to run. Do not reply with analysis only unless you hit a real blocker.

## Verification

### prove_current_slice | Prove Current Slice
Take the current bead or active change and drive one real user-visible or operator-visible journey to truthful working state. Identify the defended promise, choose the cheapest truthful validation lane (`unit`, `integration`, `e2e`, or explicit smoke only when truly correct), run it, fix anything that fails, and record exact evidence. Do not widen support claims beyond the proof you actually exercised.

### closeout_audit | Closeout Audit
Before closing or handing off a bead, audit the slice with fresh eyes. Check each acceptance criterion explicitly, compare support wording to exercised proof, verify the exact validation evidence is recorded, and ask whether another agent would have to rediscover an obvious remaining gap. If yes, do not close mechanically; fix it if still one truthful slice or split/create the missing child bead and leave explicit notes.

### install_and_use_fresh_eyes | Install And Use Fresh Eyes
Act like a real operator on a fresh install/setup/usage path as far as the current environment truthfully allows. Follow the documented steps exactly, actually invoke the tools, and compare docs and support claims against live behavior. Raise or update repair beads for reproducible failures, make an explicit test-added versus no-test decision where required, and do not stop at doc review alone.

### ci_failure_claim_and_fix | Claim And Fix CI Failure
Treat the relevant failing GitHub Actions run as owned work, not ambient noise. Claim exactly one local bead for it, announce ownership in Agent Mail with the required run metadata, reproduce locally where truthful, fix the underlying problem, and record remote closeout evidence before closure. Do not create duplicate repair beads for the same run or stop at log reading without either a fix, a bounded child bead, or an explicit blocker note.

### tool_use_feedback | Tool-Use Feedback
Look at your current run and recent swarm behavior and identify one place where a tool, prompt, skill, or command surface should have been used earlier but was missed. If a small repo-local improvement would fix that, implement the prompt/palette/docs update now; otherwise record the exact guidance gap in a bead or Agent Mail note so it becomes reusable swarm infrastructure rather than another one-off operator nudge.

## Coordination

### analyze_beads_and_allocate | Analyze Beads And Allocate
Reread `AGENTS.md` first, then read `docs/beads/BEAD_AND_PROMPT_FAILURE_PATTERNS.md` before making allocation decisions. Use `bv` and `br` together to determine the highest-leverage division of work across active agents. Prefer proof-unit leaves over broad theme buckets, check for overlap and hidden dependency lies, then send Agent Mail messages with concrete work suggestions, explain why those choices are sound, and avoid duplicating anyone’s active claim.

### check_and_respond_to_mail | Check And Respond To Mail
Check Agent Mail now, acknowledge anything that requires it, reply where needed, and make sure you know the names and current work of the active agents before continuing.

### introduce_to_fellow_agents | Introduce To Fellow Agents
Before doing anything else, read `AGENTS.md`, register with Agent Mail, and introduce yourself to the other active agents. Then check the ready queue and start real work.

### swarm_shape_check | Swarm Shape Check
Inspect recent activity, Agent Mail threads, and the current in-progress frontier and decide whether the swarm is missing a capability, duplicating work, or over-analyzing. If the right move is to add or redirect a reviewer, architect, verification, maintenance, or fresh-eyes lane, say so explicitly with reasons and recommended mission text; otherwise recommend the narrowest prompt redirect that restores momentum without creating role brittleness.

## Recovery

### recovery_continue | Recovery Continue
Assume you were interrupted or compacted. Re-read `AGENTS.md`, reopen the relevant sections of `docs/PLAN_FOR_ROGER_REVIEWER.md`, and check `docs/beads/BEAD_AND_PROMPT_FAILURE_PATTERNS.md` if the prior run produced partial work or confusing closure. Then check Agent Mail, inspect the current queue truth with `br ready`, and continue from the durable repo state rather than from memory. Confirm your bead, files, support-claim boundaries, and validation obligations before resuming edits.

### recovery_exhausted_queue | Recovery Exhausted Queue
Re-read `AGENTS.md`, reopen the relevant plan sections, check Agent Mail, and verify whether the queue is truly exhausted. Use `br ready`, `br list --status open`, `br blocked`, and `bv --robot-triage`. If the next safe slice is obvious, create or split it and announce it. Use `docs/beads/BEAD_AND_PROMPT_FAILURE_PATTERNS.md` as a checklist against under-splitting, overlap, and fake exhaustion. Only report exhaustion if the frontier is genuinely empty for you.
