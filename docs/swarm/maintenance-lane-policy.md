# Swarm Maintenance Lane Policy

This policy separates **maintenance-lane** work (bead-health, queue-trust, and
swarm operability repair) from ordinary implementation pickup.

## Lane Definitions

### Implementation lane (default)

- Default for normal swarm workers.
- Primary responsibility: claim and execute product implementation beads from
  `br ready`.
- Must **not** treat tracker-repair work as incidental background cleanup.
- If queue health issues are observed, report them and continue implementation
  unless a maintenance bead is explicitly claimed.

### Maintenance lane (explicit)

- Opt-in lane for queue trust and operability repairs.
- Allowed work examples:
  - bead graph hygiene and dependency repair
  - `br` operability regressions and pinned-path health
  - swarm launch/supervisor/runbook reliability fixes
- Must avoid consuming product implementation beads unless maintenance work is
  exhausted or leadership explicitly redirects.

## Activation Rules

Use maintenance lane when one of these is true:

1. queue trust is degraded (for example, stale blocked-cache or repeated
   `database is busy` contention causing coordination failures)
2. launcher/supervisor behavior is preventing safe parallel work
3. a maintenance bead is the highest-leverage unblocked frontier item

Otherwise, stay in implementation lane.

## Operational Guidance

- Keep maintenance workers in a separate tmux session or explicitly marked lane.
- Announce maintenance claims in Agent Mail so implementation workers route
  around overlapping files.
- Record validation evidence for maintenance changes just like implementation
  beads.
- Close and sync bead state as soon as maintenance objectives are met.
