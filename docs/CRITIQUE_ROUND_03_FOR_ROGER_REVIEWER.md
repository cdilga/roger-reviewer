# Critique Round 03 for Roger Reviewer

Status: historical critique artifact. This document records what changed and
why during Round 03. It is not the current spec. If anything here conflicts
with `AGENTS.md` or `PLAN_FOR_ROGER_REVIEWER.md`, those canonical documents win.

This round re-opens several places where Round 02 intentionally narrowed the
plan, but the product requirements turned out stronger: first-class browser
integration, simultaneous reviewer fan-out, cross-harness support, and day-one
hybrid search.

## Highest-Value Revisions

### 1. Rust-first local app is now the simpler default

FrankenTUI already forces Rust at the TUI layer. Search is also easiest in
Rust, and Native Messaging hosts are naturally small native binaries. Once the
requirements include multi-instance coordination, worktree/runtime setup,
search, and bidirectional browser messaging, the argument for a TypeScript core
gets weaker rather than stronger.

Implication:

- keep the browser extension in TypeScript/JS
- treat the local app as Rust-first unless a later spike proves a concrete
  advantage for a split core
- if a split is retained, justify it against the extra TUI/core, search, and
  bridge seams it introduces

### 2. Native Messaging likely moves into v1

Custom URL schemes remain useful, but only as one-way launch helpers. They
become awkward once the extension needs to:

- choose between active Roger instances
- send structured actions and receive success/error responses
- support two simultaneous reviewers reliably
- offer first-class UX in Edge without pretending the bridge is simpler than it is

Implication:

- if v1 extension scope remains richer than a single launch button, use Native
  Messaging as the primary bridge from the beginning
- keep `roger://` as an optional convenience path, not the only supported core
  workflow
- reject localhost HTTP / WebSocket designs that smuggle a daemon back into the
  architecture center

### 3. Roger needs a harness abstraction it owns

The OpenCode question is no longer just "CLI or internal coupling?" The more
important question is what Roger's stable provider contract looks like so that
OpenCode, Claude, Pi-Agent, GitHub CLI flows, or future harnesses can all fit
under the same local review model.

Recommended shape:

- a Roger-owned `HarnessAdapter` boundary with capability discovery
- provider-specific adapters behind that boundary
- Roger-owned durable session ledger and resume bundles regardless of provider
- permission for deeper provider-specific integrations only behind the adapter,
  never in the review domain itself

### 4. Multi-instance and worktree support need an explicit config contract

The current plan says worktrees matter, but not yet how users make two local
app instances safe to run at once. That gap is now critical because
simultaneous reviewers are a hard requirement.

The plan should define:

- `enable_worktrees`
- copied file list or glob support for files such as `.env` and `.env.local`
- port rewrite strategy
- DB strategy: shared, copied, isolated, or hook-driven
- preflight diagnostics that explain why a repo is or is not safe to fan out

### 5. "Hybrid search from day one" is fine, but it must be real

The plan still mixes two ideas: semantic search as a later enhancement, and
semantic search from day one. If Roger wants both search modes from the start,
the install and packaging story must own that decision rather than assuming a
pretend semantic fallback.

Implication:

- ship or provision a real embedding model from the start
- define index lifecycle, rebuild behavior, and ranking calibration early
- keep the search contract stable enough that the rest of Roger does not care
  whether the provider is Tantivy-only or full hybrid underneath

### 6. Integration tests should be few, slow, and extremely high-value

The right answer is not "many end-to-end tests." It is a very small set of
boundary-heavy tests that prove the architecture works where unit tests cannot.

Recommended initial integration test set:

- one canonical happy-path review flow from launch to durable findings
- one multi-instance/worktree flow proving two reviewers can coexist safely
- one browser bridge contract test covering Native Messaging host behavior
- one hybrid-search fixture test proving both retrieval modes are wired and
  ranked correctly

## Outcome

Round 02's simplifications were useful for cutting scope, but the latest
requirements change the center of gravity. The next integration pass should
therefore focus on:

- Rust-first local architecture
- Native Messaging as the likely v1 bridge
- a Roger-owned harness adapter boundary
- explicit multi-instance/worktree configuration
- real day-one hybrid search packaging and testing
