# Roger Reviewer

> Local-first pull request review with durable sessions, structured findings,
> and explicit human approval before anything goes back to GitHub.

| Status | Stage |
| --- | --- |
| Product maturity | Planning and bead-polishing |
| Release status | In development, not a release |
| First implementation release target | `0.1.0` |
| Planned versioning | `0.1.x` early Roger releases, then CalVer once the product is shipping mature releases |
| Default mode | Review-only, not auto-fix |
| Source of truth | Local state |
| Primary surfaces | CLI, TUI, GitHub launch surface |

Roger Reviewer is not at `v1.0` yet. This repository is still in active
planning and early development, and nothing here should be treated as a stable
release artifact.

Roger Reviewer is a local-first review system built around one core idea:
review quality improves when findings, prompts, evidence, and follow-up survive
beyond a single terminal run.

Instead of treating review as disposable chat output, Roger Reviewer aims to:

- keep review sessions durable and resumable
- preserve a real fallback into plain OpenCode
- make findings first-class objects with state and evidence
- route any GitHub mutation through an explicit approval step

## Why It Exists

Most review tooling is optimized for one-shot output. Roger Reviewer is being
designed for continuity:

- start from the shell or a GitHub PR page
- run staged review passes instead of one monolithic prompt
- triage findings in a TUI-first workflow
- draft outbound comments locally
- post only after explicit human approval

```mermaid
flowchart LR
    GH[GitHub PR page] --> LAUNCH[Launch Roger]
    SH[rr review / rr resume] --> SESSION[Review session]
    LAUNCH --> SESSION
    SESSION --> PASSES[Explore -> Deep review -> Follow-up]
    PASSES --> FINDINGS[Structured findings + artifacts]
    FINDINGS --> TUI[TUI triage and approval]
    TUI --> DRAFTS[Local outbound drafts]
    DRAFTS --> POST[Explicit post to GitHub]
```

## Product Shape

| Surface | Role |
| --- | --- |
| `rr` CLI | Start, resume, inspect, and refresh review sessions |
| Rust TUI | Main workflow for triage, follow-up, and approval |
| Browser extension | GitHub-side launch surface for local review flows |
| Local store | Durable sessions, findings, artifacts, and audit history |

## Non-Negotiable Constraints

- No automatic GitHub posting.
- No automatic bug-fixing by default.
- No hidden daemon at the center of the architecture.
- No fake fallback story: every Roger session must map back to a usable
  underlying OpenCode session.
- Mutation-capable flows must be explicit and visibly elevated.

## Current Repo Contents

The repository is intentionally early. At the moment, it contains planning
artifacts rather than implementation packages.

| Path | Purpose |
| --- | --- |
| [`AGENTS.md`](AGENTS.md) | Operating contract for coding agents in this repo |
| [`docs/PLAN_FOR_ROGER_REVIEWER.md`](docs/PLAN_FOR_ROGER_REVIEWER.md) | Canonical product and architecture plan |
| [`docs/BEAD_SEED_FOR_ROGER_REVIEWER.md`](docs/BEAD_SEED_FOR_ROGER_REVIEWER.md) | Seed structure for the bead graph |
| [`docs/CRITIQUE_ROUND_01_FOR_ROGER_REVIEWER.md`](docs/CRITIQUE_ROUND_01_FOR_ROGER_REVIEWER.md) | First critique and integration round |
| [`docs/CRITIQUE_ROUND_02_FOR_ROGER_REVIEWER.md`](docs/CRITIQUE_ROUND_02_FOR_ROGER_REVIEWER.md) | Second critique round focused on architecture risk |
| [`docs/CRITIQUE_ROUND_03_FOR_ROGER_REVIEWER.md`](docs/CRITIQUE_ROUND_03_FOR_ROGER_REVIEWER.md) | Third critique round focused on Rust-first local architecture and Native Messaging |
| [`docs/ROUND_04_ARCHITECTURE_RECONCILIATION_OUTCOME.md`](docs/ROUND_04_ARCHITECTURE_RECONCILIATION_OUTCOME.md) | Round 04 closeout artifact aligning ADR decisions, canonical docs, and remaining bounded questions |
| [`docs/PLANNING_WORKFLOW_PROMPTS.md`](docs/PLANNING_WORKFLOW_PROMPTS.md) | Prompts for critique, integration, and readiness loops |
| [`docs/REPO_ONBOARDING_AND_DISCOVERY_PROMPTS.md`](docs/REPO_ONBOARDING_AND_DISCOVERY_PROMPTS.md) | Reusable prompt pack for repo onboarding, discovery, and canonicalization |
| [`docs/DATA_MODEL_AND_STORAGE_CONTRACT.md`](docs/DATA_MODEL_AND_STORAGE_CONTRACT.md) | Implementation-facing data, concurrency, and storage contract |
| [`docs/RELEASE_AND_TEST_MATRIX.md`](docs/RELEASE_AND_TEST_MATRIX.md) | Explicit support, release, fixture, and validation matrix for `0.1.0` |
| [`docs/REFERENCE_SOURCES_AND_EXPLORATION_TARGETS.md`](docs/REFERENCE_SOURCES_AND_EXPLORATION_TARGETS.md) | External standards, prior-art sources, and approved exploration targets for future agent review |
| [`docs/DEV_MACHINE_ONBOARDING.md`](docs/DEV_MACHINE_ONBOARDING.md) | Practical machine setup guide for Codex, Agent Mail, and planning workflow access |
| [`docs/adr/README.md`](docs/adr/README.md) | Architecture decision records that narrow the plan into implementable contracts |
| [`.beads/issues.jsonl`](.beads/issues.jsonl) | Tracked planning graph in export form |
| [`roger-reviewer-brain-dump.md`](roger-reviewer-brain-dump.md) | Raw intent source document |

## Document Roles

The docs are not all peers.

- [`AGENTS.md`](AGENTS.md) is the operational contract for agents.
- [`docs/PLAN_FOR_ROGER_REVIEWER.md`](docs/PLAN_FOR_ROGER_REVIEWER.md) is the
  canonical product and architecture plan.
- [`docs/BEAD_SEED_FOR_ROGER_REVIEWER.md`](docs/BEAD_SEED_FOR_ROGER_REVIEWER.md)
  and `.beads/` are the task-decomposition layer derived from the plan.
- [`docs/DATA_MODEL_AND_STORAGE_CONTRACT.md`](docs/DATA_MODEL_AND_STORAGE_CONTRACT.md)
  and [`docs/RELEASE_AND_TEST_MATRIX.md`](docs/RELEASE_AND_TEST_MATRIX.md) are
  implementation-facing support contracts that narrow the canonical plan.
- [`docs/REFERENCE_SOURCES_AND_EXPLORATION_TARGETS.md`](docs/REFERENCE_SOURCES_AND_EXPLORATION_TARGETS.md)
  is the reference index for official external standards, prior-art notes, and
  approved exploration targets.
- `CRITIQUE_ROUND_*` files are historical rationale and integration artifacts,
  not the current spec.
- [`docs/REPO_ONBOARDING_AND_DISCOVERY_PROMPTS.md`](docs/REPO_ONBOARDING_AND_DISCOVERY_PROMPTS.md)
  is the reusable pre-planning discovery workflow.
- [`roger-reviewer-brain-dump.md`](roger-reviewer-brain-dump.md) is raw intent,
  not authority.

If documents disagree, treat `AGENTS.md` and the canonical plan as current
truth, and treat critique rounds as explanation only.

## Current Draft Architecture

```text
.
├── apps/
│   ├── cli/
│   ├── extension/
│   └── tui/
├── packages/
│   ├── app-core/
│   ├── config/
│   ├── github-adapter/
│   ├── prompt-engine/
│   ├── session-opencode/
│   ├── storage/
│   └── worktree-manager/
├── docs/
├── _exploration/
└── .beads/
```

## Near-Term Milestones

1. Sync the live bead graph to the current planning set.
2. Finish bead polishing and readiness review.
3. Start implementation only after the planning gate passes.

## Read Next

- [`docs/PLAN_FOR_ROGER_REVIEWER.md`](docs/PLAN_FOR_ROGER_REVIEWER.md)
- [`docs/ALIEN_ARTEFACTS_FOR_ROGER_REVIEWER.md`](docs/ALIEN_ARTEFACTS_FOR_ROGER_REVIEWER.md)
- [`docs/REFERENCE_SOURCES_AND_EXPLORATION_TARGETS.md`](docs/REFERENCE_SOURCES_AND_EXPLORATION_TARGETS.md)
- [`docs/PLANNING_WORKFLOW_PROMPTS.md`](docs/PLANNING_WORKFLOW_PROMPTS.md)
- [`docs/DEV_MACHINE_ONBOARDING.md`](docs/DEV_MACHINE_ONBOARDING.md)
- [`AGENTS.md`](AGENTS.md)
