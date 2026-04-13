# Human Operator Flywheel Guide

This is a guide for you as the human operator of an NTM-driven coding swarm.

It is not a worker prompt. It is an operating doctrine for the person steering
the machine.

The goal is to move from "I launched some agents" to "I am deliberately running
an agent flywheel."

---

## What The Operator Actually Does

The operator is not a glorified broadcaster.

The operator:

- shapes the swarm
- chooses when to widen or narrow it
- decides when the agents need recon versus execution versus review
- notices drift before it turns into waste
- injects better prompts instead of repeating vague instructions
- uses continuity tools so progress survives interruption, compaction, and provider switching

The operator should think like an air traffic controller plus research lead,
not like a chat user issuing one-off requests.

---

## Core Mental Model

Jeffrey-style flywheel usage is not:

- spawn a bunch of panes
- send one giant prompt
- hope they self-organize

It is closer to:

1. create a swarm with the right shape
2. initialize it with strong orientation
3. watch what actually happens
4. inject targeted prompts when drift appears
5. add specialist agents when the current swarm shape is wrong
6. use continuity and recovery tools so work keeps compounding instead of resetting

That is the flywheel:

- better prompts produce better local work
- better local work produces clearer signals
- clearer signals let you route better
- better routing reduces waste
- reduced waste makes larger swarms practical

---

## The Operator's First Principle

Do not confuse activity with leverage.

A lot of agents typing is not the same thing as a productive swarm.

A productive swarm has:

- clear current mission
- strong local rules
- truthful queue or work frontier
- explicit recovery path
- specialist prompts available on demand
- low ambiguity about who should do what next

If those are absent, launching more agents usually makes things worse.

---

## What NTM Is For

Use `ntm` as an operator cockpit, not just a launcher.

The important operator surfaces are:

- `ntm spawn`
- `ntm add`
- `ntm palette`
- `ntm send`
- `ntm activity`
- `ntm status`
- `ntm controller`
- `ntm coordinator`
- `ntm handoff`
- `ntm interrupt`
- `ntm kill`

If you only use `spawn`, `send`, and `assign`, you are using only a thin slice
of the tool.

The real power comes from:

- prompt libraries
- targeted prompt injection
- specialist expansion
- continuity and handoff
- recovery after drift or compaction

---

## Your Job At Launch

Before launching a swarm, decide five things:

1. What is the mission?
2. Is this primarily recon, execution, review, or maintenance?
3. What mix of agents do I actually need?
4. What prompt family should initialize them?
5. What recovery path will I use if the session drifts or gets compacted?

If you cannot answer those, you are not ready to launch.

Do not launch a swarm just because work exists. Launch when you know how the
machine should behave.

---

## Prompt Families, Not One Prompt

One generic marching-orders prompt is not enough.

You want prompt families.

At minimum:

- `default_new_agent`
- `read_agents_and_investigate`
- `next_bead` or `next_useful_task`
- `frontier_widening`
- `fresh_review`
- `recovery_continue`
- `recovery_exhausted_queue`

These are now seeded in [command_palette.md](/Users/cdilga/Documents/dev/roger-reviewer/docs/swarm/command_palette.md).

The operator should think:

- "This worker is drifting, give it recovery"
- "This pane is idle, give it frontier widening"
- "This new agent should start in recon mode"
- "This part of the system needs a fresh-eyes review"

That is a more advanced and more truthful way to run a swarm than resending one
universal prompt.

---

## The Command Palette Is A Real Tool

`ntm palette` is not documentation. It is an operator interface.

It reads prompt entries from:

```bash
~/.config/ntm/command_palette.md
```

This repo keeps a shared palette at:

```bash
docs/swarm/command_palette.md
```

Install it with:

```bash
./scripts/swarm/install_ntm_palette.sh
```

Once installed, use:

```bash
ntm palette roger-reviewer
```

The operator habit to build is:

- use the palette first
- use raw `ntm send` when the palette does not already express the mission
- when a new prompt proves valuable, add it to the palette

That is how ad hoc prompting turns into reusable swarm infrastructure.

---

## Direct NTM Usage Beats Hidden Wrappers

Repo wrappers are useful only if you understand the direct NTM behavior they are
wrapping.

Good wrapper usage:

- standardizing defaults
- pointing at the right prompt library
- setting project-specific conventions
- reducing repetitive typing

Bad wrapper usage:

- hiding how prompts actually get into panes
- masking the difference between launch prompts and rebroadcast prompts
- preventing you from learning the real `ntm` operator surface

The operator should remain fluent in direct `ntm`, even when wrappers exist.

---

## Add Specialists, Don't Just Nag The Existing Swarm

One of the most important flywheel habits is this:

when the current swarm shape is wrong, fix the swarm shape.

Do not keep yelling more detailed instructions into the same set of panes if
what you actually need is a different kind of agent.

Use:

```bash
ntm add roger-reviewer --persona=reviewer --prompt "Review the active changes deeply"
```

or:

```bash
ntm add roger-reviewer --persona=architect --prompt "Trace the current data flow and identify weak seams"
```

The exact persona set is NTM-defined, but the principle matters more than the
defaults.

Operator rule:

- if the current swarm lacks a capability, add it
- if it has the capability but is drifting, redirect it
- if the whole frontier is wrong, widen or reshape it

---

## Do Not Over-Rely On Mega Prompts

Big prompts feel powerful because they sound thorough.

In practice they often create:

- fuzzy priorities
- vague ownership
- weak closeout behavior
- context dilution
- confused stopping conditions

Prefer prompts that are:

- short
- explicit
- reversible
- mission-shaped
- targeted at one next mode of work

Good example:

- "Read `AGENTS.md`, trace the CLI to storage path, summarize the real data flow with file references, and identify the first unsafe seam."

Weaker example:

- "Understand the whole project deeply, fix bugs, improve architecture, coordinate everyone, and write tests."

The operator’s craft is in decomposing missions, not inflating them.

---

## Use Recovery As A First-Class Tool

Agents drift. Sessions get interrupted. Claude compacts. Humans detach and come
back later.

A mature swarm treats recovery as normal.

You need recovery at three levels:

1. prompt recovery
2. session recovery
3. provider/session handoff recovery

Prompt recovery means:

- re-read `AGENTS.md`
- re-check inbox
- re-check queue truth
- continue from durable state, not memory

Session recovery means:

- attach again
- inspect activity
- inject a recovery prompt
- use handoffs if needed

Provider/session recovery means:

- preserve context in a handoff artifact
- resume cleanly rather than improvising from memory

Jeffrey’s ecosystem explicitly leans on these ideas through things like PCR and
CASR. You should adopt the same mindset even where the exact tooling differs.

---

## Compaction Is Not A Corner Case

Compaction is not rare. It is normal once sessions get long.

Do not treat it as an accident. Design for it.

The operator should assume:

- long sessions will compact
- compacted agents lose local sharpness
- rules and mission framing need to be restored

That is why recovery prompts and reminders matter.

If Claude is in the mix, a PCR-style reminder is worth using. Even where you do
not have a hook, your swarm doctrine should treat "re-read the rules and resume
from durable state" as standard behavior.

---

## Watch The Machine, Not Just The Logs

A good operator watches for these failure modes:

- agents idling silently
- many agents doing overlapping work
- agents overcommunicating and underexecuting
- workers rediscovering the same context repeatedly
- workers acting from stale mental state
- the frontier becoming ambiguous
- rebroadcasts weakening prompt quality

Do not ask "are the panes active?"

Ask:

- is the swarm still pointed at the right frontier?
- are prompts still helping?
- do I need to widen, narrow, or redirect the swarm?

That is a much more useful monitoring question.

---

## Your Prompt Library Should Evolve

Treat strong prompts as assets.

When you discover a prompt that consistently improves behavior:

1. save it
2. name it clearly
3. group it into the palette
4. reuse it
5. refine it after real runs

That is how operator skill becomes infrastructure.

The palette should grow from real usage, not abstract brainstorming.

---

## Suggested Operating Loop

For a normal coding swarm:

1. decide mission and mode
2. spawn the initial session with the right agent mix
3. seed strong new-agent orientation
4. monitor with `ntm activity` and `ntm status`
5. use `ntm palette` to inject mode-specific prompts
6. add specialist agents when needed
7. use handoff/recovery tools when interrupted
8. stop or shrink the swarm once the leverage drops

You are not trying to maximize the number of prompt injections.
You are trying to maximize compounded useful work.

---

## Suggested Direct Commands

Launch:

```bash
ntm spawn roger-reviewer --cod=4 --no-user --auto-restart
```

Use the palette:

```bash
ntm palette roger-reviewer
```

Monitor:

```bash
ntm activity roger-reviewer --watch
ntm status roger-reviewer
```

Add a specialist:

```bash
ntm add roger-reviewer --persona=reviewer --prompt "Review the current changes deeply and identify real risks"
```

Inject a recovery prompt:

```bash
ntm send roger-reviewer --cod --file docs/swarm/command_palette.md
```

That exact last command is usually not how you will use recovery in practice.
The point is that recovery should be something you can trigger deliberately, not
something you vaguely hope happens.

Stop the swarm:

```bash
ntm interrupt roger-reviewer
ntm kill --project roger-reviewer --force
```

---

## What "Good" Looks Like

You are operating well when:

- you use the palette as a normal part of swarm steering
- you think in prompt families rather than one default prompt
- you add specialists rather than overloading generalists
- you recover intentionally after interruption or compaction
- your swarm’s prompts get better over time because you keep the good ones
- you understand direct `ntm` well enough that wrappers do not hide the machine from you

That is the real competency target.

Not "memorize Jeffrey’s exact prompts."

The target is:

- understand the operating model
- build the right local prompt assets
- use the swarm like an operator

---

## Final Principle

The operator should always be reducing ambiguity.

If the swarm is unclear:

- clarify the mission
- clarify the prompt
- clarify the frontier
- clarify who should do what
- clarify how to recover if the context degrades

Every good intervention does one of those things.

That is the practical heart of the flywheel approach.
