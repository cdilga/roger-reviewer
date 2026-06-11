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
| Worktree / parallel session isolation (worktree-manager crate) | **Copilot app** (desktop): one managed worktree per session, fully automated lifecycle; sessions can also run in-place (no worktree) or in cloud sandboxes; "My Work" dashboard | **Mostly dissolved** — but worktree location is a non-configurable global path, there are no resource controls or documented system requirements, so a capacity-aware setup skill is needed (see §5.2 skill 8). |
| Session continuity (ResumeBundle, locator reopen, `rr return`, Tier A/B/C) | Copilot CLI `--resume`, auto-compaction, desktop session persistence; cloud agent remembers prior session context on `@copilot` mentions | **Mostly dissolved.** Weaker guarantees than roger's transactional model, but good enough in practice. |
| Durable, structured, searchable findings (SQLite + Tantivy + fingerprints) | Nothing native. Copilot reviews are ephemeral PR comments. | **Keep — as a skill + a tiny file-based ledger** (~300 lines of script, not 7K LOC of Rust). |
| Explicit approval gates before posting (draft → approve → post, fail-closed) | Nothing native. Copilot code review is comment-only and can never approve/block; agents post directly. | **Keep — as a skill-enforced protocol.** This is roger's genuinely original idea and your differentiator as a *trusted* reviewer: **you** hold the approve/request-changes verdict; no native agent ever gets it. |

**Recommendation:** Stop building the harness. Extract roger's two durable ideas — the findings ledger and the approval gate — into a suite of ~8 fat skills (each carrying full doctrine + small scripts), run them on whatever harness is at hand (Copilot CLI, Claude Code, Copilot app, cloud agent), and layer the native machinery (auto-review rulesets, Agent Merge, "Fix with Copilot") around them. Two of the eight skills exist specifically because the native tooling has setup gaps: **repo onboarding** (bootstrapping the pipeline per repository) and **machine-capacity probing** (the Copilot app documents zero system requirements, has no session caps, and puts worktrees in a non-configurable global path — weak machines need an explicit strategy). Estimated effort: **2–3 days** vs. roger's ~12 weeks to v0.2.0.

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

### 3.4 GitHub Copilot app (desktop — technical preview May 14, 2026; expanded to all paid plans at Build, Jun 2)

*This section was re-verified against primary sources only (github.blog, docs.github.com, github/app repo) on 2026-06-10. Verdicts noted where the docs are silent.*

**Confirmed:**
- Desktop control center built **on top of Copilot CLI** (shared session truth: CLI sessions appear in My Work). macOS / Linux / Windows (incl. arm64). Pro, Pro+, **Max**, Business, Enterprise (orgs must enable preview features + Copilot CLI).
- **Parallel agent sessions, one managed git worktree per session** — "the app handles every worktree for you: no manual setup, no cleanup, no branch juggling." Sessions can alternatively run **in-place in your local repository (no worktree)** or **in a cloud sandbox** — chosen at session start. Deleted sessions force-remove worktrees but snapshot uncommitted work to a recovery ref.
- **"My Work" dashboard** with a default **Review requests** section, editable filter sections (`label:bug` etc.), sessions/issues/PRs/automations across repos.
- **Full in-app PR review loop:** click a PR → overview (summary, CI checks, review activity) → Files changed diff → **Create session** → leave inline review comments or ask the agent to change things → **submit a Review** — plus per-comment **Fix** and **Fix failing checks** buttons.
- **Agent Merge:** "prompts the workspace's Copilot session to read your pull request, fix what is blocking it, and merge it **as soon as GitHub allows**. It runs in the background, survives app restarts, and turns itself off once your pull request is merged."
- **Session modes** Interactive / Plan / Autopilot, per-session model + reasoning-effort selection; pause/resume.
- **Skills (SKILL.md), MCP servers, and global instructions configurable in app Settings**; repo/CLI-configured skills and MCP servers are inherited automatically; respects `AGENTS.md`/`CLAUDE.md`.
- **Automations:** scheduled/on-demand; with "Run in the cloud" they execute in GitHub-hosted cloud sandboxes "even when your computer is off." Cloud sandboxes are metered (Azure Container Apps-based); local sandboxing is free, with restricted FS/network.
- Review-adjacent extras: **rubber duck agent** (cross-model critic of plans/implementations), **`/security-review`** skill, **quick chats** (explore a PR with *no* worktree), **`/chronicle`** (query your session history — e.g. "summarize this week's reviews"), agentic integrated browser for verifying UI changes.

**Documented-by-silence gaps (these matter for adoption):**
- **No system requirements anywhere** — prerequisites are literally "Git installed" + a Copilot subscription. No RAM/disk/CPU guidance; the app's own changelog patched "sluggishness when multiple concurrent sessions are streaming."
- **Worktree location is a centralized global path and is NOT configurable** — open, unanswered feature requests (github/app #407, #482, #734). On small disks or monorepos, N sessions = N full working copies in a place you don't choose.
- **No concurrent-session cap or resource controls.** The widely-cited "~10 sessions per repo" figure appears only in third-party blogs — unverified.
- **Local sessions surviving window close: undocumented.** Only the cloud paths (cloud sessions/automations, Agent Merge) are documented as machine-independent or background-persistent.
- **No documented protected-branch guarantee for Agent Merge** ("as soon as GitHub allows" implies rule compliance but is never stated outright).
- **No documented link to mission control / Agent HQ** — the app does not (per docs) steer cloud *coding agent* sessions; its session model is the CLI's. `.github/agents` custom agents are documented for the cloud agent, not the app.

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

### 5.2 The skill suite (8 skills)

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

#### 7. `review-onboard` — bootstrap the pipeline for a repo (especially a new one)
- **Trigger:** "set me up to review in this repo", first review request from an unfamiliar repo, `/review-onboard`.
- **Does (idempotent — safe to re-run, reports what's already in place):**
  1. **Preflight (the `rr doctor` idea, as a skill):** `gh auth status` + scopes, `copilot --version`, git identity, your permission level on the repo (`gh api repos/{owner}/{repo} --jq .permissions`), whether you're in CODEOWNERS and for which paths, whether Copilot code review auto-request rulesets exist, whether `.github/copilot-instructions.md` / `instructions/` / `skills/` / `AGENTS.md` exist.
  2. **Ledger bootstrap:** create `~/reviews/<org>/<repo>/`, seed `memory/<org>/<repo>.md` from a quick repo survey (language/framework, test command, CI shape, hot paths, recent churn, who the frequent authors are).
  3. **Repo profile:** write `~/reviews/<org>/<repo>/profile.json` — clone size, monorepo or not, build/test cost, flaky-CI notes, default review tier (skim/full), which checklists from `pr-review` apply. `pr-review` and `review-queue` read this so per-repo behavior is data, not re-derivation.
  4. **Propose (never auto-apply) repo-side improvements:** a PR adding `pr-review` doctrine to `.github/skills/`, an auto-review ruleset, a starter `copilot-instructions.md` — each going through your normal approval. For repos where you lack admin, it generates the ask for whoever does.
  5. **Wire the app/CLI:** add the repo to Copilot app projects (or `gh repo clone` + `/add-dir` for CLI), confirm skills/MCP inheritance, run one smoke `pr-review` on a recent closed PR and diff its findings against the human review that actually happened — a calibration check before you trust it live.
- **Fat content:** the full preflight checklist with remediation steps per failure, the repo-survey question list, the calibration protocol, and templates for the repo-side PRs.

#### 8. `review-capacity` — machine-capability probing and worktree/session strategy
- **Why this exists:** the Copilot app documents **no system requirements**, offers **no session cap or resource controls**, and puts every session's worktree in a **non-configurable centralized path** (open issues github/app #407/#482/#734). A monorepo on a 256 GB laptop will fall over before any documented limit does. Roger's `worktree-manager` was solving a real problem; natively, *you* are the resource governor — so encode that governance in a skill.
- **Trigger:** during `review-onboard`, "can this machine handle parallel reviews?", before fanning out sessions, `/review-capacity`.
- **Does:**
  1. **Probe:** free disk on the volume holding the app's worktree path, RAM (total/free), CPU cores, repo working-copy size (`du` of `.git` + checkout), whether the repo needs heavy toolchains to be useful in review (node_modules, target/, virtualenvs — a worktree that can't run tests is half a worktree).
  2. **Compute a session budget:** e.g. `min(disk_free / (checkout_size × safety 2.5), max(1, RAM_GB / 4), cores − 2)` — formula lives in the skill and is tuned by experience; writes the result to `profile.json` as `max_parallel_sessions` and `preferred_isolation`.
  3. **Pick the strategy ladder per machine class:**
     - **Strong machine:** worktree sessions in the app, up to the budget; periodic worktree-debt audit (`git worktree list` across repos + stale-session cleanup nudges, since the app exposes no cleanup command).
     - **Modest machine:** 1 worktree session + **in-place (no-worktree) sessions** and **quick chats** for the rest — both confirmed app features; serialize full reviews via `review-queue` order instead of parallelizing.
     - **Weak machine / huge monorepo:** push parallelism to the **cloud** — cloud sandbox sessions (`copilot --cloud`), cloud automations for the scheduled digest, cloud agent for delegate-tier reviews; local machine only triages ledgers and approves. Note cloud sandboxes are metered — the skill states the rates and asks before fan-out.
     - **Shared/partial-clone tricks** for monorepos: `git clone --filter=blob:none` review copies, sparse-checkout worktrees created manually (outside the app) when you need tests but can't afford full checkouts.
  4. **Background-work honesty:** local-session persistence after window close is *undocumented* — so anything that must survive (overnight delegate reviews, Agent Merge babysitting, scheduled digests) gets routed to the documented background paths: cloud automations, Agent Merge, or a scheduled `copilot -p` via cron/launchd. The skill never claims background behavior the platform doesn't document (roger's truthfulness rule, kept).
- **Fat content:** the probe script (~80 lines), the budget formula with rationale, the strategy ladder, the metered-cost table, and the worktree-debt audit script.

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
4. **Full tier:** open parallel Copilot app sessions — worktree, in-place, or cloud-sandbox per your `review-capacity` budget — each running `pr-review`; triage their findings ledgers as they land; `review-post` per PR.
5. **Delegate tier:** `review-delegate` sends 2 PRs to cloud sessions / a Claude second-opinion run; results land as ledger findings for later triage.
6. **Rework loop:** request-changes verdicts trigger "Fix with Copilot" handoffs to authors; when they push, `re-review` shows you only the delta and resolved/stale bookkeeping.
7. **Tail:** Agent Merge babysits approved PRs through CI and merge queue.
8. **Friday:** `review-memory` proposes two new instruction-file rules from this week's repeated findings; you approve the PR; the native reviewer is permanently smarter.

Net effect vs. roger-as-planned: same safety invariants (nothing posts unapproved, findings durable, reconciliation honest), strictly more leverage (cloud fan-out, author-side fix delegation, org-wide compounding memory), ~1% of the maintenance surface.

---

## 7. Build order (suggested)

1. **Day 1 morning:** `findings-ledger` (schema + fingerprint + ledger scripts) and `pr-review` v1 — port the prompt-engine stage text and the alien-artifact decision rules. Test on one real PR via Copilot CLI and via Claude Code (portability check).
2. **Day 1 afternoon:** `review-post` with the halt-for-approval protocol + `gh api` batched-review script + drift refusal. This is the piece to get exactly right.
3. **Day 2:** `review-onboard` (preflight + ledger bootstrap + repo profile + calibration run — this is also how every later skill gets a consistent per-repo footing) and `review-capacity` (probe script + session budget written into the repo profile). Run both against this machine and one work repo before anything else depends on them.
4. **Day 3:** `re-review` (delta + reconcile), `review-queue` (gh search + your heuristics, reading `profile.json` for per-repo tiering).
5. **Week 1, at work:** `review-onboard` your top 3 repos; drop `pr-review` doctrine into a pilot repo's `.github/skills/`, enable the auto-review ruleset, compare native first-pass quality before/after.
6. **Week 2:** `review-memory` + `review-delegate`; install the Copilot app technical preview, let `review-capacity` decide worktree vs. in-place vs. cloud strategy for it; wire the scheduled morning digest (cloud automation if your machine shouldn't carry it).
7. **Ongoing:** archive roger active development; keep the repo as the doctrine source and a fallback if GitHub regresses (see risks).

---

## 8. Risks and honest caveats

- **Preview churn.** Skills-in-code-review, the Copilot app, and Agent Merge are all ≤6 months old, some in technical preview. Mitigation: everything load-bearing (ledger, gate, doctrine) lives in *your* files and the open agentskills spec — if GitHub regresses, the skills still run in Claude Code/OpenCode against `gh`. This is roger's "real harness fallback" principle, kept.
- **The Copilot app is resource-blind (verified 2026-06-10).** No documented system requirements, no concurrent-session cap or controls, worktrees in a centralized non-configurable path (open issues github/app #407/#482/#734), local background execution undocumented, and the unverified "~10 sessions" figure floating around third-party blogs should not be trusted. Mitigation is exactly skill 8 (`review-capacity`): probe, budget, and fall back to in-place sessions, quick chats, or cloud sandboxes (metered) on constrained machines.
- **Agent Merge's protected-branch behavior is implied, not guaranteed.** Docs say it merges "as soon as GitHub allows" and tracks required reviewers, but never state it cannot bypass approval rules. Treat branch protection rules — not Agent Merge's manners — as the actual guarantee: keep required-review counts on, and only enable Agent Merge on branches where the rules already encode your policy.
- **Skill-level gates are advisory unless tool-gated.** A skill saying "don't post" is weaker than roger's compiled state machine. Mitigation: `--excluded-tools` / permission profiles for review sessions, and the posting script as the only path (it checks for the approval marker). Accept that this is 95% of the guarantee for 1% of the code.
- **Cost.** AI Credits billing (Jun 2026): each auto-review burns credits + Actions minutes; cloud fan-out multiplies it. Watch the org meter the first month; the effort-tier config helps.
- **Work-org constraints.** Org policies gate skills/MCP for code review, the desktop preview, and Claude/Codex on Agent HQ (Pro+/Enterprise). The local-CLI lane (skills 1–5) needs nothing org-approved beyond a Copilot seat — start there.
- **Base-branch gotcha.** `copilot-instructions.md` and skills are read from the PR's **base branch** — instruction changes only take effect after merging to main.
- **What you give up from roger:** transactional launch verification, the locator/return machinery, semantic search over review history, the TUI. At personal/team scale, ledger-in-git + agent grep covers search; the rest was scaffolding for a multi-provider world the agentskills spec just standardized away.

---

## 9. Bottom line

Roger was right about *what matters* — durable findings, explicit approval, honest reconciliation, provider portability — and GitHub spent 2025–2026 shipping the plumbing roger was hand-building. The winning move is to declare victory on the doctrine, ship it as fat skills riding GitHub's thin (free, maintained, improving) harness, and keep your uniquely human asset — the trusted Approve button — wrapped in roger's gate protocol. Two days of skill-writing buys what twelve more weeks of Rust was going to.
