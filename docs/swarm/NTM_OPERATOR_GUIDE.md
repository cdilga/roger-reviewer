# NTM Operator Guide

This is a concise guide for driving `ntm` directly instead of hiding behind repo wrappers.

## Palette

`ntm palette` is integrated into NTM itself. It reads prompts from:

```bash
~/.config/ntm/command_palette.md
```

NTM also recognizes a project-local:

```bash
./command_palette.md
```

This repo keeps its shared palette at:

```bash
docs/swarm/command_palette.md
```

Install it into NTM with:

```bash
./scripts/swarm/install_ntm_palette.sh
```

That script symlinks the repo palette into `~/.config/ntm/command_palette.md`, so edits in the repo show up in `ntm palette` immediately.

## Direct Workflow

The direct operator loop should be:

```bash
ntm spawn roger-reviewer --cod=4 --no-user --auto-restart
ntm palette roger-reviewer
ntm activity roger-reviewer --watch
ntm status roger-reviewer
```

Use `ntm palette` for battle-tested prompt injections instead of copying raw prompts into panes manually.

## High-Value Commands

Use these directly:

```bash
ntm spawn roger-reviewer --cod=4 --no-user --auto-restart
ntm add roger-reviewer --persona=reviewer --prompt "Review the active changes deeply"
ntm palette roger-reviewer
ntm send roger-reviewer --cod --file some-prompt.md
ntm controller roger-reviewer
ntm coordinator status roger-reviewer
ntm handoff create roger-reviewer --auto
ntm interrupt roger-reviewer
ntm kill --project roger-reviewer --force
```

## Prompt Strategy

Use prompt families instead of one generic marching-orders prompt:

- `default_new_agent`
- `read_agents_and_investigate`
- `next_bead`
- `frontier_widening`
- `fresh_review`
- `recovery_continue`
- `recovery_exhausted_queue`

These are seeded in [command_palette.md](/Users/cdilga/Documents/dev/roger-reviewer/docs/swarm/command_palette.md).

## Built Rebroadcast

If you need to rebroadcast the repo’s default worker prompt, use:

```bash
./scripts/swarm/broadcast_marching_orders.sh --session roger-reviewer
```

This script now rebuilds the per-pane prompt with `scripts/swarm/build_prompt.sh` and sends it via `ntm send --pane ... --file ...`.

It no longer pastes the raw markdown file directly into tmux panes.

## Suggested NTM Skill

An `ntm-operator` skill should teach:

1. when to `spawn` vs `add`
2. when to use `palette` vs raw `send`
3. how to add specialist agents with `--persona` and a mission prompt
4. how to use `handoff` and recovery prompts after interruption
5. how to keep prompt libraries in `command_palette.md`
6. how to rebroadcast built prompts instead of raw files

## Competency Goal

You are operating at a strong NTM level when you can:

1. spawn a session with the right mix
2. use the palette as your normal operator UI
3. add specialist agents mid-session
4. inject prompts to subsets of agents, not just everyone
5. use handoffs/recovery intentionally
6. stop relying on repo wrappers to understand what NTM is doing
