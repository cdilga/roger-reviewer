# AGENTS.md — Roger Reviewer

This file is the single source of truth for any agent (Claude Code, Codex, or
other) working in this repository. Read it before doing anything else.

---

## What is Roger Reviewer?

Roger Reviewer is a local-first pull request review system. It combines a
session-aware CLI (`rr`), a TUI-first review interface (FrankenTUI), and a
GitHub browser extension. Its purpose is to drive high-quality review loops,
keep review state durable and searchable, and require explicit human approval
before anything is sent back to GitHub.

The core differentiator is continuity. Every finding, prompt pass, artifact, and
follow-up maps back to a durable local session that can be resumed in plain
OpenCode even if Roger-specific layers are unavailable or compacted.

---

## Critical Constraints — Non-Negotiable

These are not preferences. Violating them is a bug.

- **No automatic GitHub posting.** Outbound actions require explicit human
  approval through the TUI or CLI approval flow.
- **No automatic bug-fixing.** Review and suggest only, unless the user
  explicitly enables fix mode.
- **No writes to dev/test environments** unless the user explicitly enables
  them. Default to read-mostly.
- **No long-running daemon as the architecture center.** The system must be
  daemonless in steady state.
- **OpenCode fallback must stay real.** Every Roger review session must map to
  an underlying OpenCode session that can be resumed directly.
- **Mutation-capable flows must be visibly elevated**, not implicit.

---

## Tech Stack (Confirmed)

**Split-language architecture** — FrankenTUI has been explored and confirmed
Rust-only. See `_exploration/frankentui`.

| Layer | Language | Notes |
|-------|----------|-------|
| TUI | Rust | FrankenTUI `Model` trait, talks to app-core via Unix socket + JSON |
| CLI (`rr`) | TypeScript | Session-aware commands, prompt engine, GitHub adapter |
| App core | TypeScript | Domain logic, storage, finding lifecycle |
| Browser extension | TypeScript/JS | WebExtension; v1 = custom URL protocol only |
| Search | Rust | Tantivy + FastEmbed hybrid from day one (CASS pattern, lives in TUI binary) |

## Repo Layout (Planned)

```
.
├── apps/
│   ├── cli/           # session-aware rr CLI (TypeScript)
│   ├── extension/     # Chrome/Brave GitHub extension (TypeScript/JS)
│   └── tui/           # FrankenTUI shell (Rust)
├── packages/
│   ├── app-core/      # shared domain and orchestration (TypeScript)
│   ├── config/        # layered config model (TypeScript)
│   ├── github-adapter/  # gh CLI wrapper (TypeScript)
│   ├── prompt-engine/ # staged review prompt pipeline (TypeScript)
│   ├── session-opencode/  # OpenCode session wrapper (TypeScript)
│   ├── storage/       # SQLite + FTS → Tantivy upgrade path (TypeScript/Rust)
│   └── worktree-manager/ # (TypeScript)
├── _exploration/      # reference repos (do not import as dependencies)
│   ├── frankentui/    # FrankenTUI source — TUI architecture reference
│   ├── asupersync/    # async runtime — v2 extension bridge reference
│   └── cass/          # CASS search — search layer reference implementation
├── docs/              # planning and architecture docs
└── AGENTS.md          # this file
```

---

## Planning Documents

Read these to understand the full plan before touching code.

| Document | Purpose |
|----------|---------|
| [`docs/PLAN_FOR_ROGER_REVIEWER.md`](docs/PLAN_FOR_ROGER_REVIEWER.md) | Canonical product plan — architecture, workflows, rollout phases, risks |
| [`docs/BEAD_SEED_FOR_ROGER_REVIEWER.md`](docs/BEAD_SEED_FOR_ROGER_REVIEWER.md) | Bead graph seed — epics, acceptance criteria, dependency structure |
| [`docs/CRITIQUE_ROUND_01_FOR_ROGER_REVIEWER.md`](docs/CRITIQUE_ROUND_01_FOR_ROGER_REVIEWER.md) | First adversarial critique and integration round |
| [`docs/PLANNING_WORKFLOW_PROMPTS.md`](docs/PLANNING_WORKFLOW_PROMPTS.md) | Prompts for future critique, integration, and bead polishing rounds |
| [`docs/ALIEN_ARTEFACTS_FOR_ROGER_REVIEWER.md`](docs/ALIEN_ARTEFACTS_FOR_ROGER_REVIEWER.md) | Compact artifact pack for external critique sessions |
| [`roger-reviewer-brain-dump.md`](roger-reviewer-brain-dump.md) | Original raw brain dump — source of intent, not specification |

---

## Planning Stage Status

The project is in the **planning and bead-polishing stage**. No implementation
work has started. The beads workspace has 25 issues loaded into `.beads/beads.db`.

Planning phase checklist:

- [x] Initial plan written
- [x] Critique Round 01 completed and integrated
- [ ] Critique Round 02 (open concerns from Round 01 below)
- [ ] Bead polishing (see prompt in `PLANNING_WORKFLOW_PROMPTS.md`)
- [ ] Readiness review before implementation begins

Open concerns for Critique Round 02:
- FrankenTUI runtime and packaging implications
- The specific OpenCode integration boundary available in practice
- What `FPs` and `SA` mean in the brain dump
- Whether named-instance state sharing should be reduced for v1

**Do not begin implementation until the readiness review passes.**

---

## Working with Beads

All implementation work is tracked as beads in `.beads/beads.db`. Use `br`
(beads_rust) to interact with them.

Common commands:

```sh
br info              # workspace summary
br list              # all beads with status
br list --status todo
br show <id>         # full bead detail
br start <id>        # mark a bead in-progress
br done <id>         # mark a bead complete
br doctor            # workspace health check
```

`br` version: `0.1.29`. The DB passes integrity check as of 2026-03-20.

### How to pick your next bead

1. Run `br list --status todo` to see available work.
2. Respect the dependency graph — do not start a bead whose dependencies are
   not yet `done`.
3. Mark the bead `in-progress` before starting work.
4. Complete all acceptance criteria before marking `done`.
5. If you discover a blocker or ambiguity, add a note to the bead rather than
   guessing.

### Critical dependency spine (v1)

Repo foundation → domain schema → storage → OpenCode linkage → prompt pipeline
→ structured findings → session-aware CLI → TUI findings workflow → outbound
draft model → explicit posting flow → GitHub adapter → extension bridge and UI

The extension is intentionally last. Do not start extension work before the
local review core is real.

---

## Domain Model Summary

First-class entities:

- `ReviewSession` — top-level container for a review, linked to an OpenCode session
- `ReviewRun` — a single pass within a session
- `Finding` — structured result with evidence links, not free-form text
- `FindingFingerprint` — deterministic ID for stable identity across reruns
- `FindingState` — triage (`new`, `accepted`, `ignored`, `needs-follow-up`, `resolved`, `stale`) and outbound (`not-drafted`, `drafted`, `approved`, `posted`, `failed`) tracked separately
- `PromptStage` — exploration, deep review, follow-up
- `Artifact` / `ArtifactDigest` — stored content with content-addressed identity
- `OutboundDraft` — local representation of a proposed GitHub action
- `PostedAction` — immutable audit record after posting
- `ConfigLayer` — one layer of the additive config stack

---

## Architecture Principles

- **Ports and adapters.** The review domain owns findings, sessions, prompt
  stages, and approval state. UI surfaces (TUI, extension, CLI) are adapters.
  They do not reimplement domain rules.
- **Local-first.** Local SQLite is the source of truth. GitHub is a target
  surface.
- **Daemonless.** No always-on background service. The extension bridge must
  use a one-shot or on-demand launch mechanism.
- **Additive config.** Later layers override or extend explicitly. Hidden
  replacement is not acceptable.
- **Approval gates.** Outbound drafts are materialized locally. The posting
  step is explicit and auditable.

---

## Rollout Phase Summary

| Phase | Focus |
|-------|-------|
| 0 | Scope and unknown convergence |
| 0.5 | Architecture risk spikes (OpenCode boundary, browser bridge, artifact storage) |
| 1 | Repo structure, domain schema, storage, OpenCode session linkage |
| 2 | Session-aware CLI, prompt pipeline, structured findings |
| 3 | TUI shell, findings workflow, outbound draft approval |
| 4 | GitHub adapter, daemonless bridge, **v1 extension = "Launch Roger" button only** |
| 4.5 (v2) | Deep extension: status indicator, PR-aware dropdown, keybindings, live sync |
| 5 | Full-text search, semantic search, multi-instance hardening |

---

## Validation Gates

Do not advance phases without meeting the gate.

- **Gate A (Domain viability):** schema exists, session/finding lifecycle is
  explicit, OpenCode linkage works, finding identity prevents duplicate
  explosions.
- **Gate B (Core review loop):** CLI can start and resume, prompt stages persist
  outputs, findings survive restart.
- **Gate C (TUI usability):** user can triage findings, outbound drafts are
  reviewable locally.
- **Gate D (Bridge realism):** extension "Launch Roger" button invokes a local
  review without a persistent daemon; clipboard fallback exists; v1 makes no
  live-sync claims.
- **Gate E (Safe outbound):** nothing posts without approval; posted outputs
  are tracked back to findings.

---

## If You Are Running a Planning Critique Round

Use the prompts in [`docs/PLANNING_WORKFLOW_PROMPTS.md`](docs/PLANNING_WORKFLOW_PROMPTS.md).

The adversarial review loop:
1. Take the current `docs/PLAN_FOR_ROGER_REVIEWER.md` to a frontier model
   (GPT Pro Extended Reasoning or Claude Opus 4.5 in the web app) using
   Prompt #2 (Plan Review).
2. Bring the output back to Claude Code and use Prompt #3 (Integration) to
   merge revisions in-place.
3. Record the outcome in a new `docs/CRITIQUE_ROUND_NN_FOR_ROGER_REVIEWER.md`.
4. Repeat until suggestions become incremental (typically 4–5 rounds total).
5. Then run Prompt #6 (Bead Polishing) and Prompt #7 (Readiness Review) before
   starting implementation.

---

## If You Are Running an Implementation Bead

1. Read `docs/PLAN_FOR_ROGER_REVIEWER.md` to understand the architectural
   context.
2. Read the bead in full with `br show <id>`.
3. Implement exactly what the acceptance criteria require. No more.
4. Do not touch GitHub write paths, posting flows, or mutation-capable code
   without the approval model in place.
5. Run any smoke tests specified in the bead before marking done.
6. If you discover a dependency is incomplete, stop and flag it rather than
   working around it.

---

## Open Questions (as of 2026-03-20)

These are known unknowns. Do not silently bake in assumptions.

- **OpenCode boundary**: CLI/file boundary only, or is internal coupling
  acceptable? Needs a spike before session-orchestration work starts.
- **TUI ↔ app-core protocol**: Exact JSON schema for the Unix socket message
  protocol between Rust TUI and TypeScript app-core. Define before either side
  starts implementation.

### Resolved
- ~~FrankenTUI runtime~~ → Rust-native confirmed. Split-language architecture adopted.
- ~~Browser bridge mechanism~~ → v1: `roger://` custom URL protocol. v2: Native Messaging.
- ~~Semantic search slice~~ → Tantivy + FastEmbed hybrid from day one (in Rust binary, CASS pattern).
- ~~Credential flows~~ → Non-issue. `gh` CLI owns GitHub auth. No Keychain work needed.
- ~~`FPs` / `SA`~~ → Irrelevant to Roger architecture.

<!-- bv-agent-instructions-v1 -->

---

## Beads Workflow Integration

This project uses [beads_viewer](https://github.com/Dicklesworthstone/beads_viewer) for issue tracking. Issues are stored in `.beads/` and tracked in git.

### Essential Commands

```bash
# View issues (launches TUI - avoid in automated sessions)
bv

# CLI commands for agents (use these instead)
bd ready              # Show issues ready to work (no blockers)
bd list --status=open # All open issues
bd show <id>          # Full issue details with dependencies
bd create --title="..." --type=task --priority=2
bd update <id> --status=in_progress
bd close <id> --reason="Completed"
bd close <id1> <id2>  # Close multiple issues at once
bd sync               # Commit and push changes
```

### Workflow Pattern

1. **Start**: Run `bd ready` to find actionable work
2. **Claim**: Use `bd update <id> --status=in_progress`
3. **Work**: Implement the task
4. **Complete**: Use `bd close <id>`
5. **Sync**: Always run `bd sync` at session end

### Key Concepts

- **Dependencies**: Issues can block other issues. `bd ready` shows only unblocked work.
- **Priority**: P0=critical, P1=high, P2=medium, P3=low, P4=backlog (use numbers, not words)
- **Types**: task, bug, feature, epic, question, docs
- **Blocking**: `bd dep add <issue> <depends-on>` to add dependencies

### Session Protocol

**Before ending any session, run this checklist:**

```bash
git status              # Check what changed
git add <files>         # Stage code changes
bd sync                 # Commit beads changes
git commit -m "..."     # Commit code
bd sync                 # Commit any new beads changes
git push                # Push to remote
```

### Best Practices

- Check `bd ready` at session start to find available work
- Update status as you work (in_progress → closed)
- Create new issues with `bd create` when you discover tasks
- Use descriptive titles and set appropriate priority/type
- Always `bd sync` before ending session

<!-- end-bv-agent-instructions -->
