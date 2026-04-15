# Root-Level Flow And Architecture Diagrams

Status: Visual support doc for `0.1.x`  
Audience: operators, maintainers, and implementers who need a fast, truthful
top-level map of Roger Reviewer before diving into the canonical plan or
support contracts  
Scope: README-grade core review flow, software-architecture hierarchy, and the
elevation ladder that separates read-mostly review work from explicit outbound
mutation

---

## Purpose

This document is the small top-level diagram pack for Roger Reviewer.

It exists to answer three questions quickly:

1. what the core review loop is
2. how the major software layers and boundaries relate to each other
3. where Roger deliberately elevates actions such as approval and posting

This doc is visual support, not a replacement for the canonical plan.

Authority rule:

- if a diagram conflicts with
  [`PLAN_FOR_ROGER_REVIEWER.md`](PLAN_FOR_ROGER_REVIEWER.md) or a narrower
  support contract, the canonical prose wins
- diagrams here should compress stable truths that already exist elsewhere, not
  invent new product semantics

---

## What belongs at root level

Not every Roger flow benefits from a root-level Mermaid diagram.

| Concern | Mermaid value | Root-level now? | Why |
|---|---|---|---|
| Core review lifecycle | High | Yes | This is the main README-visible story: launch, review, triage, draft, approve, post. |
| Architecture hierarchy | High | Yes | Roger is defined by boundaries and ownership splits more than by package lists. |
| Elevation ladder | High | Yes | Approval and posting are explicit product constraints, not footnotes. |
| Browser setup and doctor | Medium | Later | Important, but secondary to the main product story and better kept near extension docs. |
| Worktree/isolation topology | Medium | Later | Useful, but belongs closer to config and preflight docs than to the root-level narrative. |
| Attention-state taxonomy | Medium | Later | Important for surfaces, but too detailed for the first-look diagram pack. |
| Prompt/worker invocation lifecycle | Medium | Later | Valuable for implementation docs, but not the first thing most readers need. |
| Update/install mechanics | Low at root level | No | Important operationally, but not part of Roger's core review identity. |

Chosen direction:

- one restrained README hero diagram for the core review loop
- two companion diagrams in docs: architecture hierarchy and elevation ladder
- explicit gates use a warmer accent
- repair and demoted paths stay visually muted rather than competing with the
  main product path

---

## Diagram 1: Core Review Loop

This is the most important diagram in the repo. The README should carry this
exact flow as a first-class artifact because it captures the core product
promise without collapsing into setup or implementation detail.

```mermaid
flowchart TD
    classDef entry fill:#EAF2FF,stroke:#4F7CFF,color:#102033,stroke-width:1.4px;
    classDef gate fill:#FFF3D9,stroke:#C68A00,color:#5B3A00,stroke-width:1.7px;
    classDef core fill:#ECFDF3,stroke:#2F855A,color:#173A28,stroke-width:1.4px;
    classDef data fill:#EEF8F6,stroke:#4C7A78,color:#102322,stroke-width:1.2px;
    classDef blocked fill:#FFF0F0,stroke:#CB3A3A,color:#6B1F1F,stroke-width:1.6px;
    classDef external fill:#F4F0FF,stroke:#7B61FF,color:#2D225E,stroke-width:1.3px;

    ENTRY["Entry surfaces<br/>CLI / TUI / extension"]:::entry
    INTAKE["Review intake<br/>repo + PR + baseline + prompt"]:::entry
    PREFLIGHT{"Preflight safe<br/>and unambiguous?"}:::gate
    ATTEMPT["LaunchAttempt<br/>recorded durably"]:::gate
    VERIFY{"Real provider session<br/>verified?"}:::gate
    WORKER["Review worker gets<br/>bounded task + context"]:::core
    PACK["Structured findings pack<br/>plus raw output"]:::core
    NORMALIZE["Roger normalizes findings,<br/>attention, and lineage"]:::core
    INSPECT["TUI / CLI inspect, triage,<br/>clarify, and reconcile"]:::core
    DRAFT["Draft comments locally"]:::core
    APPROVE{"Explicit human approval?"}:::gate
    POST["GitHub adapter posts"]:::external
    AUDIT["PostedAction audit trail"]:::data
    STORE["SQLite + artifacts + search"]:::data
    BLOCK["Fail closed<br/>status / doctor / repair guidance"]:::blocked

    ENTRY --> INTAKE --> PREFLIGHT
    PREFLIGHT -- no --> BLOCK
    PREFLIGHT -- yes --> ATTEMPT
    ATTEMPT --> STORE
    ATTEMPT --> VERIFY
    VERIFY -- no --> BLOCK
    VERIFY -- yes --> WORKER --> PACK --> NORMALIZE --> INSPECT
    NORMALIZE --> STORE
    INSPECT -- follow-up / automatic reconciliation --> WORKER
    INSPECT --> DRAFT --> APPROVE
    APPROVE -- not yet --> INSPECT
    APPROVE -- approved --> POST --> AUDIT --> STORE
```

Why this stays small:

- it shows explicit launch truth, not synthetic success
- it keeps the worker bounded and Roger-owned
- it makes approval a first-class gate
- it shows that GitHub posting is downstream of local review, not parallel to it

---

## Diagram 2: Architecture Hierarchy

This diagram answers the software-architecture question: who owns what, and how
the major surfaces and boundaries relate.

```mermaid
flowchart LR
    classDef surface fill:#F6F4EF,stroke:#6A5F52,color:#1D1B18,stroke-width:1.1px;
    classDef intake fill:#EEF5FF,stroke:#5B7EA6,color:#10233C,stroke-width:1.2px;
    classDef core fill:#EEF9F1,stroke:#4D7D5F,color:#102013,stroke-width:1.2px;
    classDef data fill:#F1F7F7,stroke:#5D8080,color:#102222,stroke-width:1.1px;
    classDef gate fill:#FFF3D9,stroke:#C68A00,color:#5B3A00,stroke-width:1.3px;
    classDef external fill:#F3EFFF,stroke:#7A63B8,color:#1F153D,stroke-width:1.1px;
    classDef blocked fill:#FFF0F0,stroke:#B65A5A,color:#331111,stroke-width:1.2px;

    PRPAGE["GitHub PR page"]:::external
    GHTHREADS["GitHub review threads"]:::external

    subgraph SURFACES["Surfaces"]
        direction TB
        CLI["CLI<br/>review / resume / status"]:::surface
        TUI["TUI<br/>dense review cockpit"]:::surface
        EXT["Extension<br/>launch only"]:::surface
        ROBOT["rr --robot"]:::surface
    end

    subgraph INTAKE["Canonical intake"]
        direction TB
        RI["ReviewIntake v1"]:::intake
        ROUTE["Resolved baseline<br/>profile / instance / worktree"]:::gate
        LAUNCH["LaunchAttempt"]:::gate
    end

    subgraph CORE["Roger core"]
        direction TB
        MANAGER["Review manager<br/>sessions / runs / findings / prompts / policy"]:::core
        STATUS["Attention + status"]:::core
        APPROVAL["Draft + approval gate"]:::core
    end

    subgraph STATE["Canonical state"]
        direction TB
        DB["SQLite ledger"]:::data
        ART["Artifacts"]:::data
        SEARCH["Search + recall"]:::data
    end

    subgraph WORKER["Worker boundary"]
        direction TB
        TASK["ReviewTask"]:::gate
        CONTEXT["WorkerContextPacket"]:::core
        RESULT["WorkerInvocation + result"]:::core
    end

    subgraph HARNESS["Harness / provider boundary"]
        direction TB
        HOST["Harness session host"]:::external
        PROVIDERS["Copilot / OpenCode / Codex / Gemini / Claude"]:::external
    end

    ADAPTER["GitHub adapter"]:::external
    NO_BYPASS["No direct GitHub write<br/>from extension, worker, or provider"]:::blocked

    PRPAGE --> EXT
    CLI --> RI
    TUI --> RI
    EXT --> RI
    ROBOT --> RI

    RI --> ROUTE --> LAUNCH --> MANAGER
    MANAGER <--> STATUS
    MANAGER <--> DB
    MANAGER <--> ART
    MANAGER <--> SEARCH

    MANAGER --> TASK --> CONTEXT --> HOST --> PROVIDERS
    PROVIDERS --> RESULT --> MANAGER

    MANAGER --> APPROVAL --> ADAPTER --> GHTHREADS
    NO_BYPASS -. enforced by Roger .-> APPROVAL
```

Why this matters:

- it makes the manager/worker/provider split legible
- it keeps the extension in its bounded launch role
- it shows canonical Roger state as local and authoritative
- it keeps GitHub write ownership behind the explicit approval lane

---

## Diagram 3: Elevation Ladder

This diagram is not mainly about time. It is about permission, ownership, and
which actions are deliberately elevated.

```mermaid
flowchart TD
    classDef surface fill:#EAF2FF,stroke:#4F7CFF,color:#102033,stroke-width:1.3px;
    classDef core fill:#ECFDF3,stroke:#2F855A,color:#173A28,stroke-width:1.3px;
    classDef gate fill:#FFF3D9,stroke:#C68A00,color:#5B3A00,stroke-width:1.6px;
    classDef data fill:#EEF8F6,stroke:#4C7A78,color:#102322,stroke-width:1.1px;
    classDef external fill:#F4F0FF,stroke:#7B61FF,color:#2D225E,stroke-width:1.2px;
    classDef repair fill:#F4F4F5,stroke:#71717A,color:#27272A,stroke-width:1.1px,stroke-dasharray: 4 3;
    classDef blocked fill:#FFF0F0,stroke:#CB3A3A,color:#6B1F1F,stroke-width:1.4px;

    ENTRY["Launch / resume / open local"]:::surface
    READ["read_query<br/>status, findings, search, history"]:::core
    CLARIFY["clarify<br/>follow-up without mutation"]:::core
    DRAFT["request_draft<br/>local draft queue"]:::core
    APPROVAL["request_approval<br/>explicit human handoff"]:::gate
    POST["post<br/>GitHub adapter only"]:::external
    AUDIT["Posted action + audit trail"]:::data

    EXTENSION["Extension can launch or open local only"]:::surface
    WORKER["Worker may read, clarify,<br/>and request draft/approval only"]:::external
    REPAIR["Demoted repair lane<br/>doctor / extension setup / update / bridge"]:::repair
    BLOCK["Ambiguous target, unsafe topology,<br/>invalidated draft, or missing approval<br/>=> block or degrade"]:::blocked

    ENTRY --> READ --> CLARIFY --> DRAFT --> APPROVAL --> POST --> AUDIT
    APPROVAL -- not approved --> READ

    EXTENSION -. no approval or posting .-> ENTRY
    WORKER -. proposes, never self-approves .-> DRAFT
    BLOCK -. guards .-> APPROVAL
    BLOCK -. guards .-> POST
    REPAIR -. setup and recovery only .-> ENTRY
```

What this diagram protects against:

- treating draft approval as an implementation detail
- making the extension look like it owns review mutation
- letting repair/admin surfaces dominate the product story
- blurring the difference between “the worker proposed it” and “Roger posted it”

---

## Keep These In Sync

If the root-level story changes, update these diagrams when the underlying
canonical truth changes in one of these docs:

- [`PLAN_FOR_ROGER_REVIEWER.md`](PLAN_FOR_ROGER_REVIEWER.md)
- [`ROUND_05_SURFACE_RECONCILIATION_BRIEF.md`](ROUND_05_SURFACE_RECONCILIATION_BRIEF.md)
- [`REVIEW_WORKER_RUNTIME_AND_BOUNDARY_CONTRACT.md`](REVIEW_WORKER_RUNTIME_AND_BOUNDARY_CONTRACT.md)
- [`ATTENTION_EVENT_AND_NOTIFICATION_CONTRACT.md`](ATTENTION_EVENT_AND_NOTIFICATION_CONTRACT.md)
- [`CONFIGURATION_CAPABILITY_AND_DEFAULTING_CONTRACT.md`](CONFIGURATION_CAPABILITY_AND_DEFAULTING_CONTRACT.md)

If a diagram starts needing too many notes, split it into a narrower
surface-specific diagram instead of inflating the root-level pack.
