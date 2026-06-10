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
Read `AGENTS.md`, `README.md`, and only the plan sections you need. Check Agent Mail. Run `br ready`, inspect one candidate with `br show <id>`, claim it, reserve files, and start. Add tests at the cheapest truthful layer. In a persistent tmux pane, green tests and truthful closeout are only a checkpoint: repeat after each checkpoint and continue to the next bead. Stop only for genuine exhaustion, a real blocker, or user redirection.

### replan_execution_kickoff | Replan Execution Kickoff
Re-anchor on `AGENTS.md`, `README.md`, and the relevant plan sections. Ignore stale assumptions. Check Agent Mail, then use `br ready`, `br list --status open`, and `bv` only for ranking. Claim one bead, reserve files, and implement it now. Add tests in the same slice. If more work is required for honest closeout, finish it if still one slice; otherwise bead it immediately. In a persistent tmux pane, do not stop after proof or closeout; repeat after each checkpoint and move to the next bead.

### reread_agents_md | Reread AGENTS
Reread `AGENTS.md` so the repo rules are fresh again. Reopen the canonical plan sections relevant to your active bead, then check Agent Mail, rerun `br ready`, and continue from the current durable repo state rather than from memory alone.

### next_bead | Next Useful Bead
Check Agent Mail. Use `br ready` as truth. Inspect one candidate with `br show <id>`, claim it, reserve files, and start coding. Add unit tests by default; use integration only for real boundaries. Own one bead at a time, not one bead forever: after each checkpoint, including successful validation and closeout, loop back to Agent Mail and `br ready`.

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
Open the bead, then edit code now. If you do not own a bead yet, use `br ready`, inspect one candidate, claim it, and start. Add or update tests in the same slice. Finish through code, validation, and closeout notes. Do not stop at summaries or partial scaffolding.

### analysis_to_action | Analysis To Action
If you have been reading, tracing, or summarizing for too long, convert that understanding into one concrete implementation move now. Name the bead, the operator-visible promise, and the exact files you will change, reserve those files, then edit code and end this cycle with a real diff plus the validation command you expect to run. Do not reply with analysis only unless you hit a real blocker.

## Verification

### prove_current_slice | Prove Current Slice
Name the promise. Pick the cheapest truthful lane. Add or update the matching tests, run them, fix failures, and record exact evidence. Default to unit tests. If a real integration or budgeted E2E gap remains, either land it now if still one slice or claim the testing follow-on immediately. Once proof is complete, return to the ready loop rather than treating validation as the end of the run.

### closeout_audit | Closeout Audit
Before closing, check every acceptance criterion, confirm the recorded proof, and ask whether any obvious gap remains. Reject closeout if an implementation bead has no new or updated tests and no explicit no-test rationale. Fix the gap if still one slice; otherwise create the follow-on bead.

### testing_first_closeout | Testing First Closeout
Before closing, name the promise, choose the lane, add or update the tests, run them, and record the command and result. Default to unit tests. Use integration for real boundaries. Do not substitute docs, metadata, or smoke when deterministic automated proof is feasible. After truthful closeout in a persistent pane, continue with the next safe bead.

### expand_validation_frontier | Expand Validation Frontier
Find the most obvious missing proof around active work. Prefer unit and integration first. If a budgeted E2E is still only a docs contract, claim or create the narrowest bead that turns it into runnable coverage. Do not let surrounding work close while required proof is only implied.

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
Assume interruption. Re-read `AGENTS.md` and only the plan sections you need. Check Agent Mail, use `br ready`, confirm your bead and validation obligations, then continue from durable state instead of memory. If this is a persistent tmux pane, return to the loop after each checkpoint.

### recovery_exhausted_queue | Recovery Exhausted Queue
Check Agent Mail and verify that the queue is truly exhausted with `br ready`, `br list --status open`, `br blocked`, and `bv` only for ranking. If the next safe slice is obvious, create or split it and announce it. Report exhaustion only if the frontier is genuinely empty for you.

### continue_churning | Continue Churning
This is a persistent tmux pane, not a one-shot cycle. After each checkpoint, including testing and closeout: check Agent Mail, rerun `br ready`, inspect the next candidate, claim it, reserve files, and keep moving. Stop only for exhaustion, a real blocker, or user redirection.

### checkpoint_owned_commit | Checkpoint And Commit Owned Slice
Pause briefly. Run `git status --short`. If your owned, validated slice can be staged surgically, commit it now. Ignore unrelated dirt elsewhere. Only skip for real hunk overlap, missing validation, or explicit no-commit direction. If blocked, name the exact file or hunk.
