# Alien Workflows for Roger Reviewer

Status: planning-stage workflow pack.

This document adapts the strongest parts of the Agent Flywheel methodology into
Roger-specific, self-contained workflows for external critique, research, and
artifact hardening.

The goal is not to import the full flywheel ideology into Roger. The goal is to
extract the parts that produce better planning artifacts:

- self-contained review packets
- stronger external-model critique loops
- better plan-to-beads transfer
- better capture of lessons back into reusable docs and prompts

## What Roger Should Adopt

These ideas from the Flywheel guide are strongly worth keeping:

- plan space, bead space, and code space are different reasoning layers
- plan-to-beads is a translation problem, not a clerical one
- self-contained artifacts matter because external models do not share local
  session context
- repeated critical passes find materially more issues than a single pass
- fresh-session review is different from same-session review
- lessons should feed back into prompts, docs, and skills rather than staying
  anecdotal

## What Roger Should Not Import Blindly

- Roger should not become a global assistant memory or multi-agent flywheel
  product
- Roger should not make a giant tool stack or VPS setup part of the product
  thesis
- Roger should keep using its own local-first, repo-first, approval-gated
  architecture rather than assuming the external workflow is itself the product

## Workflow 1: External Critique Packet

Use this when you want frontier-model criticism of Roger's architecture without
forcing the external model to ingest the full repo.

### Inputs

- canonical plan
- relevant ADRs
- current open questions
- release/test matrix if relevant
- one compact alien packet

### Required artifact

Create or refresh a self-contained packet named like:

- `ALIEN_PACKET_<TOPIC>_FOR_ROGER_REVIEWER.md`

The packet should include:

- what Roger is
- the exact problem being reviewed
- non-negotiable constraints
- already-decided architecture
- still-open questions
- failure modes the reviewer should attack
- any supporting external sources that materially shape the question

### Prompt pattern

```text
Review this Roger Reviewer packet critically. I do not want praise. I want the
strongest possible objections, missing risks, contradictions, and better design
choices.

Focus especially on:
- hidden blockers
- realism of the proposed workflow
- where the packet is hand-waving unresolved design gaps
- whether the decisions are sufficient to support implementation

For each proposed change:
1. explain the problem clearly
2. explain the reasoning
3. provide git-diff style changes against the packet or canonical plan

<PASTE THE SELF-CONTAINED PACKET HERE>
```

### Hardening loop

1. Create or refresh the packet.
2. Run one serious external critique.
3. Re-run the same critique prompt up to 3-5 times if the output feels too
   short or self-satisfied.
4. Integrate the best revisions into the canonical plan and ADRs.
5. Record the outcome in a critique-round or supplement doc.

## Workflow 2: Research and Reimagine

Use this for ambitious new capabilities where an external project has already
solved part of the problem well.

This is the closest Roger equivalent to the Flywheel guide's
"research-and-reimagine" flow.

### Good Roger use cases

- richer review-session durability from adjacent harness/session systems
- better daemonless companion/bridge patterns
- stronger findings-pack validation or repair strategies
- more powerful local search or scope-aware memory tooling

### Required artifact

Create a proposal named like:

- `PROPOSAL_TO_INTEGRATE_IDEAS_FROM_<EXTERNAL>_INTO_ROGER_REVIEWER.md`

### Sequence

1. Ground in Roger first.
   Read the canonical plan, relevant ADRs, and the local open questions.
2. Investigate the external system directly.
   Study the code/docs firsthand rather than relying on latent model memory.
3. Write a first proposal.
   Focus on accretive adaptation, not straight porting.
4. Push for deeper and more ambitious ideas.
5. Invert the analysis.
   Ask what Roger can do because of its local-first review primitives that the
   external system could never do.
6. Run repeated blunder hunts.
   Re-run the same critical prompt multiple times until the proposal stops
   changing materially.
7. Close explicit design gaps.
8. Make the proposal self-contained for cross-model review.
9. Send it to multiple frontier models for git-diff feedback.
10. Integrate only the strongest ideas back into Roger docs.

### Prompt pattern: investigate and propose

```text
Study <external project / guide> directly and look for ideas Roger Reviewer can
adapt in highly accretive ways. Do not suggest a shallow port. Reimagine the
strongest ideas through Roger's own primitives and constraints:

- local-first
- repo-first scoped evidence
- no daemon as the architecture center
- OpenCode fallback must stay real
- approval-gated GitHub posting

Write a self-contained proposal document named
PROPOSAL_TO_INTEGRATE_IDEAS_FROM_<EXTERNAL>_INTO_ROGER_REVIEWER.md.
```

### Prompt pattern: inversion

```text
Now invert the analysis: what can Roger do because of its own local review
primitives, durable findings model, and approval-gated workflow that this
external system fundamentally cannot do?
```

### Prompt pattern: blunder hunt

```text
Look over everything in the proposal for blunders, mistakes, misconceptions,
logical flaws, errors of omission, oversights, or sloppy thinking.
```

Run that exact prompt repeatedly, not just once.

## Workflow 3: Plan-to-Beads Transfer Audit

Use this when the plan is strong but you are worried the bead graph may drop
important rationale, tests, or sequencing.

### Required artifact

Create a coverage report named like:

- `PLAN_TO_BEADS_TRANSFER_AUDIT_<DATE>.md`

### Report sections

- plan elements with matching beads
- plan elements missing bead coverage
- beads missing clear plan backing
- beads missing validation or smoke tests
- beads missing dependency edges
- recommended bead edits

### Prompt pattern

```text
Walk every important part of the Roger Reviewer markdown plan and map it to
actual beads. Ensure rationale, constraints, and tests are embedded in bead
descriptions. Identify anything in the plan that has no bead or any bead that
has no clear plan backing.

Output:
- a coverage report
- exact bead edits needed to close the gaps
```

### Roger-specific emphasis

Attack these first:

- session durability and dropout/return flows
- approval/posting invalidation
- scope and memory promotion rules
- browser bridge realism
- multi-instance setup and validation

## Workflow 4: Fresh-Eyes Reset

Use this when reviews are getting repetitive or suspiciously mild.

### Prompt pattern

```text
Read AGENTS.md and the relevant Roger planning docs from scratch. Review this
artifact as if you are seeing it for the first time. I want a full fresh-eyes
critical pass unconstrained by the prior session's local minima.
```

### Best targets

- canonical plan after 2-3 integration rounds
- large ADR clusters after several local edits
- bead seed after big reconciliation passes
- transfer-audit report before importing or polishing more beads

## Workflow 5: Feedback-to-Infrastructure Closure

Use this after critique rounds, failed reviews, or repeated agent confusion.

### Required artifact

Create a short closure note named like:

- `FEEDBACK_TO_INFRASTRUCTURE_CLOSURE_<TOPIC>.md`

The note should capture:

- what repeatedly went wrong
- what reusable artifact should change
- which artifact was updated
- what future swarms now inherit automatically

### Typical closure targets

- `AGENTS.md`
- planning prompt packs
- alien packets
- ADR wording
- implementation source notes
- future bead acceptance criteria

### Prompt pattern

```text
Mine the recent critique outputs, planning corrections, and agent confusion
patterns. Distill the useful lessons into Roger's reusable artifacts so the next
swarm starts from the improved baseline.

Output:
- the revised reusable artifact(s)
- a short closure note describing the lesson now encoded
```

## Immediate Next Steps for Roger

These are the highest-value alien-workflow moves to do next:

1. Refresh the core alien packet.
   Expand `ALIEN_ARTEFACTS_FOR_ROGER_REVIEWER.md` so it reflects the accepted
   ADRs, current open seams, and the new memory/bridge decisions.
2. Create one research-and-reimagine proposal.
   Pick exactly one external system or guide that can materially improve Roger.
   Good first targets are local companion/bridge patterns, session durability
   systems, or scoped memory/search systems.
3. Run a plan-to-beads transfer audit.
   Roger is still in planning/bead-polishing stage; this is the right moment to
   test whether the live bead graph actually carries the planning intent.
4. Add feedback-to-infrastructure closure after each critique round.
   Do not let good critique stay trapped in round docs only.

## Good First Candidate Topics

- browser companion install and host-registration workflow
- `UsageEvent` and merged-resolution link storage model
- plan-to-beads coverage for ADR 2 through ADR 5
- extension build/install scripts and dependency budget

## Source

This workflow pack is adapted from:

- Agent Flywheel complete guide:
  <https://agent-flywheel.com/complete-guide>
