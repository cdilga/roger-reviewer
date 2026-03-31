# Swarm Live-Loop Rehearsal 2026-03-31

## Scope

Bounded live rehearsal for `rr-eus.5` on the active swarm session `roger-reviewer`, validating that the refreshed control-plane stack can:

1. observe live worker state
2. stay active across idle checkpoints
3. continue assigning work or explicitly signal exhaustion

## Commands Run

1. `./scripts/swarm/control_plane_status.sh --session roger-reviewer --lines 80`
2. `ntm activity roger-reviewer --json`
3. `br ready`
4. `tmux capture-pane -p -t roger-reviewer-control-plane:0 -S -400 | rg -n 'Assign|assigned|No ready|idle|Completion|Warning|exhaust'`
5. `sleep 35; tmux capture-pane -p -t roger-reviewer-control-plane:0 -S -120 | tail -n 60`
6. `tmux list-sessions | rg 'roger-reviewer-(control-plane|controller|health|ft)' || true`

## Observed Outcomes

- Control-plane status showed the real session with mixed live states (working + idle panes), plus active Agent Mail connectivity.
- Assign-watch log contains successful assignment events, including:
  - `Assigned rr-xr6.1 to pane 0`
  - `Assigned rr-eus to pane 1`
  - `Assigned rr-eus.2 to pane 2`
  - `Assigned rr-xr6 to pane 4`
  - `Assigned rr-1ab to pane 5`
- Follow-up capture after ~35 seconds showed fresh completion events still arriving (timestamps progressed from `22:26:xx` into `22:27:xx`), demonstrating the loop remained active through idle/working churn instead of silently stalling.
- `tmux list-sessions` confirmed these control-plane sessions remained running:
  - `roger-reviewer-control-plane`
  - `roger-reviewer-controller`
  - `roger-reviewer-health`
- `br ready` returned non-empty frontier (`9` ready issues), so this run exercised the "continue assigning work" branch rather than queue-exhaustion signaling.

## Remaining Gaps

- Assign-watch repeatedly logs:
  - `Invalid transition assigned -> completed for rr-eus`
  - `Invalid transition assigned -> completed for rr-xr6.1`
  This indicates completion-state reconciliation drift when beads are already closed or re-triaged outside the assignment lane.
- `roger-reviewer-ft` was not present because `ft`/Frankenterm is not installed on this machine (`command -v ft` fails). This is tracked separately by the Frankenterm wiring/install beads.

## Rehearsal Verdict

`rr-eus.5` acceptance intent is met for the current swarm topology:

- live state is observable
- control-plane loop survives idle checkpoints without silent stalls
- assign-watch continues dispatching work while ready beads exist

Queue-exhaustion branch was not observed in this run because the ready queue remained non-empty.
