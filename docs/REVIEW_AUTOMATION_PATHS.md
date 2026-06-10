# Review Automation Paths

Status: support contract for current automation surfaces and the bounded next
steps. The approval gate is not negotiable: nothing in this document authorizes
automatic posting to GitHub.

## What is automatable today

| Surface | Automation it enables |
| --- | --- |
| `rr prs --robot` | Machine-readable review queue: open PRs joined with local Roger state plus a suggested `next_command` per PR. An agent or script can sweep a repo and start/resume reviews in priority order. |
| `rr review` / `rr resume` / `rr return` `--robot` | Scriptable launch and re-entry with fail-closed envelopes (`outcome`, `repair_actions`). |
| `rr agent` worker transport | Provider sessions submit structured findings packs through `worker.submit_stage_result`; Roger validates, audits, and materializes canonical findings without any human copy-paste. |
| `rr triage` / `rr draft` / `rr approve` / `rr post` `--robot` | The full outbound chain is scriptable end-to-end, while each stage stays an explicit local decision and `rr post` executes only one exact approved batch. |
| `rr search --robot` | Prior-review memory recall for seeding follow-up passes. |
| Skills (`.claude/skills/roger-review-driver`, `roger-copilot-harness`, `roger-inside-roger-agent`) | Give coding agents the operating contract for each side of the boundary. |

A practical operator loop an agent can run today:

```bash
rr prs --robot                          # pick the next PR needing attention
rr review --pr <n> --provider copilot --robot
rr findings --pr <n> --robot            # read materialized findings
rr triage --finding <id> --state accepted --robot
rr draft --pr <n> --finding <id> --robot
# human decision point:
rr approve --pr <n> --batch <id> --robot
rr post --pr <n> --batch <id> --robot
```

## Bounded next steps (not yet landed)

These are the intended expansion lanes. Each must keep the explicit-approval
and daemonless constraints, and each needs its own validation contract before
any support claim widens.

1. **Re-review on push** — `rr prs` already detects PR `updatedAt` drift
   against the session's recorded target. The next slice is a
   `refresh_recommended`-driven re-entry hint per queue item, so a sweep
   script can re-run `rr review` only where the head moved.
2. **Scheduled queue sweeps** — a cron/CI-invoked `rr prs --robot` plus
   `rr review --robot` for `not_started` items. Daemonless by construction;
   the sweep is just repeated one-shot invocations.
3. **Auto-draft from accepted findings** — a `rr draft --all-accepted`
   convenience that drafts every accepted-but-undrafted finding in one batch.
   Approval and posting stay manual.
4. **Attention-state notifications** — surface `needs_attention` transitions
   through the notification contract
   (`docs/ATTENTION_EVENT_AND_NOTIFICATION_CONTRACT.md`) so the operator is
   pulled in only when a decision is actually needed.
5. **Cross-repo queues** — `rr prs` over a configured repo set, for operators
   who drive reviews across many repositories.

## What stays out of scope

- automatic `rr approve` or `rr post` from any scheduled or agent-driven flow
- bypassing the draft/approve/post chain with raw `gh` writes
- a resident daemon to watch GitHub; sweeps are one-shot invocations
