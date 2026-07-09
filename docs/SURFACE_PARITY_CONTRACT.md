# Surface Parity Contract (CLI / TUI / Extension)

Status: active implementation contract
Class: cross-surface parity authority
Audience: agents building the CLI, TUI, and browser-extension surfaces

Authority: `AGENTS.md` critical constraints govern; this contract defines how
the three operator surfaces reach feature parity *within* those constraints.
The canonical operation set is the `rr` CLI (audited 2026-07-09). The TUI
contract is `docs/TUI_WORKSPACE_AND_OPERATOR_FLOW_CONTRACT.md`.

## Principle

The CLI, the TUI (`rr open`), and the GitHub browser extension are three
adapters over one review core. An operator should be able to run the full
review loop on any of them, and the same operation must invoke the same
fail-closed domain logic on every surface — surfaces render and route, they do
not reimplement domain rules (ports-and-adapters, `AGENTS.md`).

Parity does NOT mean identical UX or an identical operation set on every
surface. It means: for every operator-facing review operation, each surface
either exposes it, or omits it for a **documented, principled reason** (a
security boundary, an interactivity constraint, or a deliberate bounded-mirror
scope) — never by accident or half-implementation.

## Deliberate, documented asymmetries

These are the ONLY places a surface may lack a CLI operation. Everything else
is a parity gap to close.

1. **The final GitHub post stays out of the extension.** `AGENTS.md`: the
   extension must not post to GitHub directly, click GitHub submit controls, or
   use raw `gh`. The extension may stage up to and including *drafting and
   clarification*, but **approval and posting remain Roger-mediated and visibly
   elevated through the TUI or CLI**. The extension hands off posting with an
   exact copyable command; it never posts.
2. **Worker transport (`rr agent worker.*`) is not an operator surface.** It is
   the in-session provider-worker protocol; it belongs on none of the three
   operator surfaces as an operator action.
3. **Dev/repair plumbing** (`rr bridge *`, contract export/verify) is CLI-only
   maintainer tooling, out of the operator parity set.
4. **Setup/install/update** operations are launched from the CLI (and the
   extension bootstrap guidance points at them); the TUI does not re-expose
   install/update because those mutate the binary/host outside a review.

Every other operator operation must be reachable on all three surfaces (the
extension in bounded/local form, mutations visibly elevated).

## The parity matrix (operator operations)

Legend: ✅ present · ⛔ deliberate asymmetry (numbered above) · ▲ gap to close.

Matrix state audited against source 2026-07-10 (`packages/tui`, `packages/cli`,
`packages/bridge`, `apps/extension`). Rows still marked ▲ are real, open gaps —
do not read the v2026.07.09 changelog's "the deferred surface is closed" as
covering them; it closed the *review-loop* legs (draft, post, clarify, evidence,
launch/resume), not the *entry* legs (doctor, queue, start, reuse-or-fresh).

| Operation | CLI | TUI | Extension |
| --- | --- | --- | --- |
| doctor / preflight | ✅ | ▲ (posture line only) | ⛔4 (guidance) |
| queue (open PRs) | ✅ | ▲ | ▲ (listing rows are launch, not a queue) |
| start review (fresh `--pr`) | ✅ | ▲ (empty-store hint only) | ✅ |
| reuse-or-fresh | ✅ | ▲ | ✅ |
| resume review | ✅ | ✅ (`o` → suspend/run/return) | ✅ |
| return (dropout handoff) | ✅ | ✅ (auto on child exit, reloads truth) | ▲ |
| open cockpit | ✅ | (is itself) | ✅ (copyable cmd) |
| view findings | ✅ | ✅ | ✅ (read-only mirror) |
| triage finding | ✅ | ✅ | ✅ (bounded elevated) |
| inspect evidence excerpt | ▲ (locations, no text) | ✅ (bounded excerpt) | ▲ (count) |
| create draft from findings | ✅ | ✅ (`d`) | ▲ (bounded) |
| edit draft body | ✅ | ✅ | ✅ (edit-as-revision) |
| approve batch | ✅ | ✅ (elevated, word `approve`) | ⛔1 (hands off) |
| post batch to GitHub | ✅ | ✅ (elevated, distinct word `post`) | ⛔1 (hands off) |
| search prior reviews | ✅ | ✅ | ✅ (read-only mirror) |
| memory: accept/reject candidate | ✅ (`rr memory review/accept/reject`) | ✅ | ⛔1 (review is elevated) |
| clarification / follow-up | ✅ (`rr clarify`, durable) | ✅ (bounded composer) | ✅ (bounded kickoff) |
| list / pick sessions | ✅ | ✅ | ✅ |
| status snapshot | ✅ | ✅ | ✅ (bounded) |
| timeline | ✅ (`rr timeline`) | ✅ | ✅ (read-only mirror) |
| semantic asset posture | ✅ | ✅ | ▲ (display) |

### Still open after v2026.07.09

The review loop reaches parity once a session exists. Getting *into* a session
is still CLI-first, and the parity guard (a source-presence grep) cannot see
this:

- **TUI**: `doctor`, `queue`, `start review --pr`, `reuse-or-fresh`. The empty
  cockpit prints `run rr review --pr <number> to start` instead of starting one.
- **Extension**: `return`, evidence-excerpt text, a real PR queue, posture display.

## Gaps to close, by surface

> **Historical work order — delivered in v2026.07.09.** Everything in this
> section shipped: the shared `roger-review-ops` crate exists, and the TUI,
> extension, and CLI legs listed below are wired and covered by tests. It is
> kept as the rationale record for *why* each leg looks the way it does. For
> what is still open, see **Still open after v2026.07.09** above — do not
> re-implement from this list.

### Shared domain foundation (do first)

The draft-materialization, posting, triage, and memory-resolution logic
currently live inside the CLI `handle_*` functions. Extract each into a shared
domain operation (app-core or a shared `review_ops` module over `RogerStore`)
that returns a typed result, and refactor the CLI handlers to call it. The TUI
backend and the bridge then call the SAME function. No surface reimplements a
fail-closed check.

Operations to extract: `materialize_draft_batch(session, finding_ids|all)`,
`approve_batch(session, batch)`, `post_batch(session, batch)` (GitHub adapter
stays behind it), `set_triage(finding_ids, state)`,
`resolve_memory_review(id, accept)`, `create_clarification(finding, body)`.

### TUI (target: full operator parity)

- **create draft from findings** — Findings screen key (`d`) materializes a
  batch from the selection/accepted set via the shared op; land on Drafts.
- **post batch, elevated** — Drafts screen: an approved batch gains a second
  elevation prompt (type `post`) that runs the shared post op. Posting stays
  visibly elevated (distinct gate, distinct confirm word) — the "CLI-only" hint
  is removed. Matches the mutation-visibility contract.
- **clarification / follow-up composer** — the bounded local composer the TUI
  contract already specifies; materializes a durable clarification linked to
  finding lineage (see CLI durable-clarification gap), not a UI-only note.
- **evidence excerpt** — inspector renders the code at the anchor (read via the
  session's repo binding; fail honestly when unavailable).
- **launch / resume / return from the TUI** — use the proven suspend-spawn-
  resume runtime mechanism (already shipped for `$EDITOR` draft editing):
  suspend the TUI, run the provider launch, return. Dropout is a visible action.

### Extension (target: bounded local parity; posting/approval hand off)

- **triage finding** — labeled, visibly-elevated per-finding triage buttons via
  a new `triage_finding` bridge action → shared op.
- **draft view + edit-as-revision** — render draft batches/items; a local
  editor saves a revision via a `revise_draft` bridge action → shared op. Never
  posts.
- **clarification kickoff** — a `request_clarification` bridge action creating a
  durable clarification.
- **search** (read-only), **timeline mirror** (read-only), **semantic posture
  display** — bounded read surfaces over existing robot output.
- **approve/post** — NOT in the extension; render the exact `rr send approve` /
  `rr send post` handoff commands (copyable) instead.

### CLI (target: close reverse-parity gaps the TUI already has)

- **memory accept/reject command** — `rr memory review` (list) + `rr memory
  accept|reject --request <id>` over the shared `resolve_memory_review` op.
- **timeline command** — `rr timeline [--session]` (or `rr findings
  --timeline`) rendering the same run→stage→posted view the TUI shows.
- **durable clarification** — `worker.request_clarification` and an operator
  `rr clarify` path persist a clarification row (today they only echo); this is
  the shared op the TUI/extension composers also use.

## Validation

- Every shared domain op gets unit tests at the storage/app-core layer.
- Every surface wiring gets its cheapest-truthful test: CLI smoke, TUI
  FakeBackend model tests, extension jsdom tests.
- A parity guard test asserts the matrix: for each operator op, the surfaces
  that should expose it do (by presence of the command/key/action), and the
  deliberate asymmetries stay asymmetric (extension has no post/approve action).
  Note its limit: `packages/cli/tests/surface_parity_guard.rs` is a
  **source-presence grep**, not a behavioral test. It catches a rename or a
  deletion that breaks a parity leg; it cannot tell a wired leg from a stub, and
  it does not assert the ▲ rows are absent. A green guard is not evidence that
  the matrix above is complete — only that nothing it names has regressed.
- Mutations stay visibly elevated on every surface (TUI elevation prompts;
  extension labeled buttons + confirm for heavier ops); posting is elevated and
  never implicit.

## Non-goals

- Identical visual layout across surfaces.
- Extension posting or approval (deliberate asymmetry 1).
- Exposing the worker transport as an operator action (asymmetry 2).
