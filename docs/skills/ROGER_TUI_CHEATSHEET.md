# Roger TUI Cheatsheet

Status: reusable Roger skill.

Purpose:
Exact keys and screens for Roger's local review cockpit (`rr open` /
`rr tui`) — Home/finder, PR Queue launch lane, Picker, SessionHome,
Findings triage/draft, Drafts approve/edit/post, Timeline, Search, and the
help overlay. No aspirational keys, only what `packages/tui` actually
implements.

Open the cockpit with `rr open` (compat name: `rr tui`). Every key mapping
below is taken directly from `packages/tui/src/model.rs` key handlers and
`packages/tui/src/view.rs` key hints/help overlay — nothing here is
aspirational.

The cockpit drives the same gated command paths as the CLI: queue listing,
review launch/resume, draft materialization, and GitHub posting all dispatch
the `rr queue` / `rr review` / `rr resume` / `rr send draft` / `rr send post`
handlers in-process, so every provider, reuse, stale-state, and approval gate
applies identically across surfaces.

Global, from any screen: `q` quit, `?` toggle the help overlay. Navigation
keys are case-insensitive. Text-input modes (session filter, finding filter,
search query, elevation confirmations) capture raw characters until you leave
them.

## Picker

Shown only when Roger hands you disambiguation candidates for reentry.

- `j`/`k` (or arrows): move
- `Enter`: open the highlighted candidate
- `Esc`: dismiss the picker and fall back to the full session finder (Home)

## Home (session finder)

- `j`/`k`: move
- `Enter`: open the selected session (SessionHome)
- `n`: open the PR Review Queue to start a new review
- `/`: enter the session filter (type to filter, `Enter` keeps it, `Esc`
  clears, `Backspace` deletes)
- `r`: reload sessions
- `Esc`: clears the session filter if one is active

## PR Review Queue

Opened with `n` from Home. Lists the repo's open pull requests joined with
local Roger state (the `rr queue` projection). Loading and launching block
the cockpit briefly while the underlying command runs.

- `j`/`k`: move
- `Enter`: start a review for the highlighted PR — reuses an existing
  non-terminal session covering the same repo/PR (identical reuse-or-new
  semantics to `rr review --pr <n>`)
- `f`: force a fresh session (`rr review --fresh`)
- `r`: reload the queue
- `Esc`: back to Home

A successful launch refreshes the session finder and lands you on the new
session's overview; a blocked launch surfaces the same message and repair
action the CLI would print.

## SessionHome

- `f`: Findings, `d`: Drafts, `t`: Timeline, `s`: Search
- `r`: resume this session in place — same continuity gates as
  `rr resume --session <id>`
- `c`: clarify hint — shows "clarification requires an active worker
  session — run rr review/resume"; this is advisory text, not a live
  clarify action
- `Esc`: back to Home (reloads sessions)

## Findings

- `j`/`k`: move
- `space`: toggle multi-select on the current finding
- `a`/`i`/`n`/`r`: triage the current selection (or the cursor row if nothing
  is selected) to accepted/ignored/needs_follow_up/resolved
- `b`: materialize the current selection (or cursor row) into an outbound
  draft batch — the same fail-closed `rr send draft` path, including its
  stale-state and missing-target gates
- `s`: cycle sort key — stored → severity → triage → outbound → file →
  stored
- `g`: cycle group key — none → severity → triage → outbound → file → none
- `/`: enter the finding filter
- `c`: same clarify hint as SessionHome
- `Esc`: clears the multi-select if any is active, else clears the finding
  filter if one is active, else returns to SessionHome — checked in that
  order

## Drafts

- `j`/`k`: move
- `Enter`: inspect the highlighted batch's items (DraftItems)
- `a`: open the elevated approve confirmation for the highlighted batch —
  only valid when its state is `awaiting_approval`. It shows the exact
  `rr approve --batch <id>` CLI equivalent and requires typing the
  confirmation word `approve` then `Enter` to execute in-process; `Esc`
  cancels. An already-approved batch points you at `p` instead; any other
  state reports why it isn't approvable right now.
- `p`: open the elevated post confirmation for the highlighted batch — only
  valid when its state is `approved`. It shows the exact
  `rr post --batch <id>` CLI equivalent and requires typing the confirmation
  word `post` then `Enter`; this POSTS the approved batch to GitHub through
  the same fail-closed path as `rr post`, with the same audit trail. An
  unapproved batch is pointed at `a` first.
- `Esc`: back to SessionHome

## DraftItems

- `j`/`k`: move
- `e`: edit the focused draft's body in `$EDITOR` — suspends the TUI, then
  persists the revision through the same storage-revision path as
  `rr send edit`
- `Esc`: back to Drafts

## Timeline

- `j`/`k`: move across runs, stage results, and posted actions
- `Esc`: back to SessionHome

## Search

- `j`/`k`: move across hits
- `/`: enter the query input (type, `Enter` runs the search, `Esc` leaves the
  input, `Backspace` deletes)
- `m`: toggle to the pending memory-review lane (`a` accept, `x` reject,
  `m` back to hits)
- `Esc` (not editing the query): back to SessionHome
- The status strip and this screen both show a posture line — retrieval
  mode plus semantic-asset state — so degraded/promoted results are visibly
  distinct from tentative ones.

## Help overlay

`?` opens it from any screen; `Esc`, `?`, or `q` closes it. It reprints this
same key reference by screen, plus the mutation-safety lines: triage, draft,
and launch run the same gated paths as the `rr` CLI; approval requires typing
`approve` and posting requires typing `post` in their elevated prompts.

## Boundaries that still hold

- GitHub posting only ever runs through the elevated post confirmation —
  a typed word, an exact batch id, and the same fail-closed `rr post` path.
  Nothing posts as a side effect of any other key.
- Clarify (`c`) is an advisory hint, not a live chat lane, until a worker
  session actually exists.
- Robot/non-TTY callers still fail closed toward `rr status --robot` /
  `rr findings --robot`; the cockpit stays interactive-only.
