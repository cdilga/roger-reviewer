# Roger Reviewer vs. GitHub Copilot Native — and a Fat-Skills, Thin-Harness Replacement Design

**Date:** 2026-06-10
**Status:** Strategy report for review — not a commitment
**Question answered:** Given what GitHub + Copilot ship natively today (including the new Copilot desktop app), what is the minimum set of skills, roger fragments, and tools that gets equivalent-or-better "trusted reviewer managing a high review load" functionality?

---

## 1. Executive summary (TLDR)

Roger set out to solve five problems. As of June 2026, GitHub has natively shipped credible answers to **three of them**, and the remaining **two are now expressible as portable agent skills** rather than a 92K-LOC Rust harness:

| Roger's problem | Native answer (June 2026) | Verdict |
|---|---|---|
| Provider parity / harness abstraction (5 session adapter crates, tier system) | **agentskills spec (SKILL.md)** adopted by Copilot CLI, code review, cloud agent, desktop app, VS Code — and already used by Claude Code, Codex, Cursor, Gemini CLI | **Dissolved.** Skills *are* the portability layer. The problem roger spends the most code on no longer needs solving. |
| Worktree / parallel session isolation (worktree-manager crate) | **Copilot desktop app**: managed git worktrees, ~10 parallel sessions/repo, fully automated lifecycle; "My Work" dashboard | **Dissolved.** Native is better than roger's. |
| Session continuity (ResumeBundle, locator reopen, `rr return`, Tier A/B/C) | Copilot CLI `--resume`, auto-compaction, desktop session persistence; cloud agent remembers prior session context on `@copilot` mentions | **Mostly dissolved.** Weaker guarantees than roger's transactional model, but good enough in practice. |
| Durable, structured, searchable findings (SQLite + Tantivy + fingerprints) | Nothing native. Copilot reviews are ephemeral PR comments. | **Keep — as a skill + a tiny file-based ledger** (~300 lines of script, not 7K LOC of Rust). |
| Explicit approval gates before posting (draft → approve → post, fail-closed) | Nothing native. Copilot code review is comment-only and can never approve/block; agents post directly. | **Keep — as a skill-enforced protocol.** This is roger's genuinely original idea and your differentiator as a *trusted* reviewer: **you** hold the approve/request-changes verdict; no native agent ever gets it. |

**Recommendation:** Stop building the harness. Extract roger's two durable ideas — the findings ledger and the approval gate — into a suite of ~6 fat skills (each carrying full doctrine + small scripts), run them on whatever harness is at hand (Copilot CLI, Claude Code, Copilot desktop, cloud agent), and layer the native machinery (auto-review rulesets, Agent Merge, "Fix with Copilot", mission control) around them. Estimated effort: **1–2 days** vs. roger's ~12 weeks to v0.2.0.

---

## 2. What roger is aiming at (distilled)

From `docs/PLAN_FOR_ROGER_REVIEWER.md` and the contract docs, roger's thesis:

1. **Local state is authoritative** — findings live in SQLite, GitHub is a posting target, not the source of truth.
2. **Findings are first-class objects** — fingerprinted, code-anchored, triage-stated (`new → accepted/ignored/needs-follow-up/resolved/stale`), reconcilable across re-reviews.
3. **Nothing posts without explicit human approval** — `draft → awaiting_approval → approved → posted`, with approval bound to an exact payload hash, invalidated on PR drift.
4. **Sessions are durable and provider-portable** — Tier A/B/C harness contracts, ResumeBundle fallback, bare-harness dropout and return.
5. **Truthfulness over convenience** — no claimed support without proof, fail-closed everywhere, full audit trail.

Current state: ~92K Rust LOC across 14 crates, 511/514 beads closed, OpenCode at Tier B, Copilot CLI feature-gated behind `RR_ENABLE_COPILOT_PROVIDER=1`, TUI and extension partially landed.

**The 20% that delivers 80% of reviewer value** (from the architecture audit): durable findings (≈50%), structured findings + evidence anchors (≈20%), approval gates (≈15%). The session adapters, bridge, extension, TUI, and semantic search are the other ~80% of the *code* delivering ~15% of the *value*.

---

## 3. What GitHub + Copilot natively provide (June 2026 snapshot)

Sources: GitHub docs/blog/changelog; dates noted. Key recent events: Agent HQ (Oct 2025), SKILL.md adoption (Dec 18, 2025), AI Credits billing (Jun 1, 2026), **Copilot desktop app + skills/MCP in code review (Build, Jun 2, 2026)**.

### 3.1 Copilot code review (the native PR reviewer)
- Request like a human reviewer (`gh pr edit N --add-reviewer @copilot`) or **auto-request via repo/org rulesets** (independent rule since Sep 2025; re-review on push, draft PRs optional).
- Inline comments with one-click suggested changes; reviews in <30s; any language.
- **Customization (this is the big shift):** `.github/copilot-instructions.md` (read from base branch), path-scoped `.github/instructions/*.instructions.md`, and — since Jun 2, 2026 — **agent skills (SKILL.md), MCP servers, and an effort tier** routing complex PRs to a higher-reasoning model.
- **Hard limit: comment-only.** It can never approve, request changes, or satisfy required-review counts. *You* remain the trusted reviewer of record — by design, this is your moat, not a gap.
- Findings can be batch-handed to the cloud agent ("Fix with Copilot" / "Fix batch with Copilot", May 2026) so the *author's* agent fixes what the reviewer flagged.

### 3.2 Copilot cloud agent (coding agent)
- Runs in ephemeral Actions environments; invoked by issue assignment, `@copilot` mention on **any** PR (Mar 2026), agents panel, schedules.
- Responds to review comments ("@copilot address this feedback"), remembers prior session context, pushes commits.
- Customization: `AGENTS.md` (also reads `CLAUDE.md`), `copilot-setup-steps.yml`, MCP servers, **custom agents** (`.github/agents/*.md`), **agent skills**, hooks.
- Limits: 59-min sessions, one repo/branch per session, no formal review verdicts.

### 3.3 Copilot CLI (`copilot`)
- Fully agentic; built-in **Code-review agent** and `/security-review` skill; custom agents at user/repo/org level; reads `AGENTS.md`; MCP (GitHub MCP pre-configured); **agent skills with `gh skill` management** (Apr 2026).
- **Programmatic mode:** `copilot -p "<prompt>"` with `--silent`, `--available-tools`/`--excluded-tools`, per-tool permission gating — i.e., scriptable review pipelines with tool allowlists (the same idea as roger's `review_readonly` policy profile, natively).
- Scheduled tasks (`/every`, `/after`), session `--resume`.

### 3.4 Copilot desktop app (technical preview, Jun 2, 2026)
- Desktop **control center for parallel agent sessions in managed git worktrees** (~10/repo, automated lifecycle) — Windows/macOS/Linux, Pro and up.
- **"My Work" dashboard:** sessions, issues, PRs, automations across repos; start a session from a PR.
- **Agent Merge:** watches CI + required reviewers, addresses review comments and failing checks, merges when your conditions are met — explicitly will not bypass human approval on protected branches.
- Local + cloud sandboxes; Plan / Interactive / Autopilot modes; same session model as mission control on github.com.

### 3.5 Agent HQ / mission control
- Multi-agent orchestration on github.com: **Claude and Codex run natively** (public preview Feb 2026, Pro+/Enterprise); assign one issue to several agents and compare PRs; Slack/Linear/Jira integrations.

### 3.6 Non-AI load management
- CODEOWNERS + team round-robin/load-balance routing; scheduled Slack reminders; **merge queue**; `gh search prs --review-requested=@me --state=open` and friends for queue scripting.

---

## 4. Capability matrix: roger vs. native vs. proposed skill

| Capability | Roger today | GitHub native (Jun 2026) | Proposed (fat skill / thin part) |
|---|---|---|---|
| First-pass automated review | `rr review` via provider, prompt-engine stages | Copilot code review on ruleset auto-request, now with skills + MCP + effort tiers | **Native**, with shared `pr-review-doctrine` skill in `.github/skills/` so its taxonomy matches yours |
| Deep agentic review with repo context | Provider session in worktree | Copilot CLI code-review agent / Claude Code; desktop app sessions | **Skill `pr-review`** run in any harness |
| Structured durable findings | `packages/storage` (7K LOC, SQLite v17, Tantivy) | ✗ (ephemeral PR comments) | **Skill `findings-ledger`**: JSONL per PR under `~/reviews/`, fingerprint script (~150 lines) |
| Triage states + reconciliation after push | Finding fingerprints, `rr resume` reconcile | ✗ (re-reviews may even repeat dismissed comments) | **Skill `re-review`**: delta diff vs. ledger, carry-forward/stale marking |
| Draft → approve → post gate | OutboundDraft state machine, approval tokens, payload-hash binding | ✗ (agents post directly; Copilot review posts comments immediately) | **Skill `review-post`**: drafts as files, explicit human "approve" step, post via `gh api` as a real review (you can Approve/Request-changes — which no Copilot agent can) |
| Audit trail | PostedAction ledger | PR timeline only | Ledger entries + the posted review itself; `git`-version the reviews dir |
| Worktree isolation / parallel reviews | `worktree-manager` crate | **Copilot desktop app managed worktrees** | Native (desktop app), or 20-line `worktree` step inside `pr-review` for CLI use |
| Session resume / return | Tier B contracts, ResumeBundle, `rr return` | `copilot --resume`, desktop session persistence, cloud agent session memory | Native + ledger (the ledger makes *findings* durable even when the *session* isn't — which was the point) |
| Provider parity | 5 session adapter crates + tier discipline | **agentskills spec** — same SKILL.md runs in Copilot CLI/cloud/review/desktop, Claude Code, Codex, Cursor | Native (the spec) |
| Review queue management | `rr prs` (partial) | `gh search prs`, scheduled reminders, CODEOWNERS routing | **Skill `review-queue`**: ranked triage of `--review-requested=@me` |
| Cross-review memory ("this team always gets X wrong") | Tantivy/FastEmbed semantic memory (experimental) | Copilot Spaces; instructions files | **Skill `review-memory`**: promote recurring findings into `.github/instructions/*.instructions.md` → the *native auto-reviewer* learns them, compounding for free |
| Delegating fixes back to authors | ✗ (explicitly out of scope) | **"Fix with Copilot"** batch handoff; `@copilot address this` | Native — and better than anything roger planned |
| Approval-to-merge tail (CI babysitting) | ✗ | **Agent Merge** (desktop app), merge queue | Native |
| Browser extension / TUI | Partial | github.com mission control, desktop app dashboard | Drop both |

**Reading of the matrix:** roger's losing battles (harness parity, worktrees, session plumbing, UI surfaces) are exactly what GitHub industrialized. Roger's winning ideas (ledger, gates, reconciliation, memory promotion) are exactly what's still missing natively — and all four fit in skills.

---

## 5. The design: fat skills, thin harness

**Thin harness** = whatever agent runtime is in front of you (Copilot CLI, Copilot desktop session, Claude Code, cloud agent) + `gh` + `git` + the filesystem. Zero daemons, zero crates, zero native-messaging bridges. The harness provides tokens, tools, and a loop; it carries no review knowledge.

**Fat skills** = each skill ships the *entire* doctrine: schemas, taxonomies, decision rules, templates, and small executable scripts (the agentskills spec allows scripts/resources inside the skill folder). The "minimal parts of roger" survive as ~300–500 lines of Python/shell *inside the skills*, not as a separate product.

### 5.1 Shared state convention (replaces `packages/storage`)

```
~/reviews/                          # a git repo — that's your audit trail & backup
  <org>/<repo>/pr-<n>/
    meta.json                       # head SHA reviewed, base, timestamps, verdict history
    findings.jsonl                  # one finding per line (schema below)
    drafts/                         # rendered, unposted review comments
      001-race-in-flush.md
    posted/                         # moved here only after posting, with remote IDs
  memory/
    <org>/<repo>.md                 # recurring patterns, promoted to instructions files
```

Finding schema (direct descendant of roger's domain model, minus the ceremony):

```json
{"id": "f-...", "fingerprint": "sha256(rule + normalized_path + hunk_anchor)",
 "severity": "blocker|major|minor|nit|question",
 "title": "...", "body": "...",
 "evidence": {"path": "src/x.rs", "line": 142, "head_sha": "abc123", "excerpt": "..."},
 "state": "new|accepted|ignored|needs-follow-up|resolved|stale",
 "source": "local|copilot-code-review|claude", "created_at": "...", "updated_at": "..."}
```

Fingerprints use rule + path + *hunk-relative* anchor (not absolute line) so they survive rebases — same trick as roger's, ~40 lines of Python.

### 5.2 The skill suite (6 skills)

#### 1. `review-queue` — morning triage
- **Trigger:** "what should I review", start of day, `/review-queue`.
- **Does:** `gh search prs --review-requested=@me --state=open` (+ team queues), enriches each PR (size, age, author, CI state, whether Copilot code review already ran, whether it touches CODEOWNERS paths you own), ranks by **risk × staleness**, and proposes a batch plan: which PRs get a 2-minute skim-and-approve, which get a full `pr-review`, which get delegated to a cloud/desktop session.
- **Fat content:** your personal ranking heuristics (e.g., migrations and auth changes always full-review; docs-only auto-skim; >800-line PRs get split-request comment template).

#### 2. `pr-review` — the core review doctrine (the fattest skill)
- **Trigger:** "review PR 123", desktop session started from a PR, `/pr-review`.
- **Does:** staged review pipeline (ports `packages/prompt-engine`'s stages):
  1. **Context:** PR description, linked issues, CI status, *and prior findings from the ledger* + `memory/<repo>.md`.
  2. **Ingest native first pass:** pull Copilot code review's existing comments via `gh api`, convert to ledger findings tagged `source: copilot-code-review` — dedupe, don't repeat its work; your job is what it missed.
  3. **Diff sweep:** full diff read with the severity taxonomy and per-language checklists.
  4. **Deep dives:** for risky hunks, read surrounding code, run tests in a worktree (desktop-managed, or `git worktree add` fallback).
  5. **Write findings** to `findings.jsonl` via the ledger script (fingerprint-deduped).
  6. **Stop.** Never posts. Hands off to `review-post`.
- **Fat content:** severity definitions with examples, evidence requirements ("no finding without a path:line anchor and an excerpt"), the roger-alien-artifact decision rules for elevate/suppress under uncertainty, language/framework checklists, "what the native reviewer is bad at" focus list (cross-file invariants, concurrency, API-contract drift, security).
- **Portability payoff:** drop the same skill into `.github/skills/` and **Copilot code review and the cloud agent run your doctrine too** — the first pass converges on your taxonomy.

#### 3. `review-post` — the approval gate (roger's crown jewel, as protocol)
- **Trigger:** "draft my review", "post my review", `/review-post`.
- **Does (fail-closed by construction):**
  1. Render `accepted` findings into `drafts/*.md` + a verdict proposal (approve / comment / request-changes) with reasoning.
  2. **Halt and present drafts to you.** The skill instructs the agent it may not call any posting tool in this phase.
  3. Only on your explicit per-batch approval ("post drafts 1,2,4 as request-changes"), post via `gh api repos/.../pulls/N/reviews` as **one batched review**, then move drafts to `posted/` with remote IDs, and stamp `meta.json` with head SHA + verdict.
  4. If HEAD moved since drafting → refuse, demand `re-review` (roger's invalidation rule).
- **Hard enforcement (optional, recommended):** run review sessions with `copilot --excluded-tools` denying `gh pr review`/mutation tools, so posting is *only* possible through this skill's script after the approval prompt — the skill-level equivalent of roger's `review_readonly` policy profile.
- **Why this beats native:** Copilot can never approve/request-changes. You can. This skill makes your verdict fast *and* deliberate — the trusted-reviewer position is preserved, the toil is removed.

#### 4. `re-review` — reconciliation after new commits
- **Trigger:** "re-review 123", PR updated after your review.
- **Does:** diff `last_reviewed_sha..HEAD` only; re-anchor existing findings (fingerprint match → carry forward; anchor gone → `stale`; fixed → `resolved`); run the `pr-review` sweep **only on the delta**; report "3 resolved, 1 stale, 2 new" instead of a fresh wall of comments. This is the single biggest time-saver at high load and the thing native re-review is worst at (it repeats dismissed comments).

#### 5. `review-memory` — compounding institutional knowledge
- **Trigger:** end of review session, "remember this", weekly.
- **Does:** scan recent ledgers for repeated fingerprint rules (same class of finding ≥3 times in a repo/team); propose promotions:
  - → `.github/instructions/<area>.instructions.md` (path-scoped) so **native Copilot code review starts catching it automatically**,
  - → `memory/<repo>.md` for your own context loading,
  - → optionally a PR adding the instruction file (itself going through your approval gate).
- **This replaces roger's Tantivy/FastEmbed ambition** with something better: instead of *your* tool remembering, you teach *GitHub's* reviewer, and every PR in the org benefits.

#### 6. `review-delegate` — offloading to cloud + desktop
- **Trigger:** queue overload, "farm these out", risky-PR second opinion.
- **Does:** encodes the routing playbook:
  - **Author-side fixes:** after posting request-changes, comment `@copilot address this feedback` or use "Fix batch with Copilot" so the author's cloud agent does the rework — you only re-review the delta.
  - **Parallel deep reviews:** spawn Copilot desktop sessions (one worktree each) or cloud-agent/Claude sessions from mission control for the 2–3 high-risk PRs, each running `pr-review` against the shared ledger convention; you triage their findings.jsonl output instead of reading whole PRs.
  - **Second opinions:** assign the same risky change to Claude and Codex via Agent HQ, compare findings.
  - **Schedules:** `copilot /every morning` → `review-queue` digest; pairs with GitHub scheduled Slack reminders.

### 5.3 What survives from roger (the "minimal parts")

| Keep | Form |
|---|---|
| Finding schema + triage states | JSON schema in `findings-ledger` skill |
| Fingerprinting algorithm | ~150-line script in the skill folder |
| Approval-gate state machine + invalidation-on-drift rule | Protocol text + ~100-line posting script in `review-post` |
| Prompt-engine stage doctrine | Markdown body of `pr-review` |
| `review_readonly` policy idea | `--excluded-tools` profiles documented in each skill |
| Alien-artifact decision contract | Referenced section in `pr-review` (you already have it as a skill) |
| `roger-copilot-harness` learnings (doctor preflight, gate hygiene) | Folded into skill prerequisites sections |

| Drop | Why |
|---|---|
| 5 session-adapter crates, tier system | agentskills spec is the portability layer now |
| `worktree-manager` | Desktop app does it better |
| Storage crate (SQLite + Tantivy + FastEmbed, 17 migrations) | JSONL in a git repo; `grep`/agent search is enough at personal scale |
| TUI, browser extension, native-messaging bridge | Desktop app + mission control + your terminal |
| Robot envelopes, bridge contract | Skills talk to files and `gh`; no IPC needed |
| `rr` binary as the center | The harness du jour is the center; skills travel |

Roger the *codebase* becomes a donor; roger the *doctrine* ships in ~6 folders totaling maybe 2–3K lines of markdown + scripts.

---

## 6. The operating model at high review load

A day in the loop:

1. **07:00 (automated):** scheduled `copilot -p` run of `review-queue` → digest: "9 PRs awaiting you: 4 skim-approve candidates, 3 full reviews, 2 delegate."
2. **First pass already done:** org ruleset auto-requested Copilot code review on all of them (running *your* skills from `.github/skills/`), so every PR arrives pre-annotated in your taxonomy.
3. **Skim tier:** for the 4 small/low-risk PRs, `pr-review` confirms native findings + delta, `review-post` drafts "approve" — you eyeball drafts, say yes, done in minutes *with a real Approve, which no agent could give*.
4. **Full tier:** open 3 Copilot desktop sessions (managed worktrees, parallel) each running `pr-review`; triage their findings ledgers as they land; `review-post` per PR.
5. **Delegate tier:** `review-delegate` sends 2 PRs to cloud sessions / a Claude second-opinion run; results land as ledger findings for later triage.
6. **Rework loop:** request-changes verdicts trigger "Fix with Copilot" handoffs to authors; when they push, `re-review` shows you only the delta and resolved/stale bookkeeping.
7. **Tail:** Agent Merge babysits approved PRs through CI and merge queue.
8. **Friday:** `review-memory` proposes two new instruction-file rules from this week's repeated findings; you approve the PR; the native reviewer is permanently smarter.

Net effect vs. roger-as-planned: same safety invariants (nothing posts unapproved, findings durable, reconciliation honest), strictly more leverage (cloud fan-out, author-side fix delegation, org-wide compounding memory), ~1% of the maintenance surface.

---

## 7. Build order (suggested)

1. **Day 1 morning:** `findings-ledger` (schema + fingerprint + ledger scripts) and `pr-review` v1 — port the prompt-engine stage text and the alien-artifact decision rules. Test on one real PR via Copilot CLI and via Claude Code (portability check).
2. **Day 1 afternoon:** `review-post` with the halt-for-approval protocol + `gh api` batched-review script + drift refusal. This is the piece to get exactly right.
3. **Day 2:** `re-review` (delta + reconcile), `review-queue` (gh search + your heuristics).
4. **Week 1, at work:** drop `pr-review` doctrine into a pilot repo's `.github/skills/`, enable the auto-review ruleset, compare native first-pass quality before/after.
5. **Week 2:** `review-memory` + `review-delegate`; get Copilot desktop app preview installed; wire the scheduled morning digest.
6. **Ongoing:** archive roger active development; keep the repo as the doctrine source and a fallback if GitHub regresses (see risks).

---

## 8. Risks and honest caveats

- **Preview churn.** Skills-in-code-review, the desktop app, and Agent Merge are all ≤6 months old, some in technical preview. Mitigation: everything load-bearing (ledger, gate, doctrine) lives in *your* files and the open agentskills spec — if GitHub regresses, the skills still run in Claude Code/OpenCode against `gh`. This is roger's "real harness fallback" principle, kept.
- **Skill-level gates are advisory unless tool-gated.** A skill saying "don't post" is weaker than roger's compiled state machine. Mitigation: `--excluded-tools` / permission profiles for review sessions, and the posting script as the only path (it checks for the approval marker). Accept that this is 95% of the guarantee for 1% of the code.
- **Cost.** AI Credits billing (Jun 2026): each auto-review burns credits + Actions minutes; cloud fan-out multiplies it. Watch the org meter the first month; the effort-tier config helps.
- **Work-org constraints.** Org policies gate skills/MCP for code review, the desktop preview, and Claude/Codex on Agent HQ (Pro+/Enterprise). The local-CLI lane (skills 1–5) needs nothing org-approved beyond a Copilot seat — start there.
- **Base-branch gotcha.** `copilot-instructions.md` and skills are read from the PR's **base branch** — instruction changes only take effect after merging to main.
- **What you give up from roger:** transactional launch verification, the locator/return machinery, semantic search over review history, the TUI. At personal/team scale, ledger-in-git + agent grep covers search; the rest was scaffolding for a multi-provider world the agentskills spec just standardized away.

---

## 9. Bottom line

Roger was right about *what matters* — durable findings, explicit approval, honest reconciliation, provider portability — and GitHub spent 2025–2026 shipping the plumbing roger was hand-building. The winning move is to declare victory on the doctrine, ship it as fat skills riding GitHub's thin (free, maintained, improving) harness, and keep your uniquely human asset — the trusted Approve button — wrapped in roger's gate protocol. Two days of skill-writing buys what twelve more weeks of Rust was going to.
