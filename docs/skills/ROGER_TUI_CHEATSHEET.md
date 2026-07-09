# Roger TUI Cheatsheet

Status: reusable Roger skill.

Purpose:
Exact keys and screens for Roger's local review cockpit (`rr open` /
`rr tui`) — Home/finder, Picker, SessionHome, Findings triage and draft
creation, Drafts approve/elevated-post/edit, the clarification composer,
evidence excerpt, launch/resume, Timeline, Search and memory review, and the
help overlay. No aspirational keys, only what `packages/tui` actually
implements.

Open the cockpit with `rr open` (compat name: `rr tui`). Every key mapping
below is taken directly from `packages/tui/src/model.rs` key handlers and
`packages/tui/src/view.rs` key hints/help overlay — nothing here is
aspirational.

Global, from any screen: `q` quit, `?` toggle the help overlay. Navigation
keys are case-insensitive. Text-input modes (session filter, finding filter,
search query) capture raw characters until you leave them.

## Picker

Shown only when Roger hands you disambiguation candidates for reentry.

- `j`/`k` (or arrows): move
- `Enter`: open the highlighted candidate
- `Esc`: dismiss the picker and fall back to the full session finder (Home)

## Home (session finder)

- `j`/`k`: move
- `Enter`: open the selected session (SessionHome)
- `/`: enter the session filter (type to filter, `Enter` keeps it, `Esc`
  clears, `Backspace` deletes)
- `r`: reload sessions
- `Esc`: clears the session filter if one is active

## SessionHome

- `f`: Findings, `d`: Drafts, `t`: Timeline, `s`: Search
- `c`: open the bounded clarification composer — type the body, `Enter`
  submits it as a durable clarification request through the shared
  `create_clarification` op, `Esc` cancels. Requires an active session.
- `o`: launch/resume the active session in its provider — suspends the
  cockpit, runs `rr review --resume --session <id>`, and returns you here
- `Esc`: back to Home (reloads sessions)

## Findings

- `j`/`k`: move
- `space`: toggle multi-select on the current finding
- `a`/`i`/`n`/`r`: triage the current selection (or the cursor row if nothing
  is selected) to accepted/ignored/needs_follow_up/resolved
- `d`: create a draft batch from the multi-select — or, when nothing is
  selected, from every `accepted` finding
- `s`: cycle sort key — stored → severity → triage → outbound → file →
  stored
- `g`: cycle group key — none → severity → triage → outbound → file → none
- `/`: enter the finding filter
- `c`: open the clarification composer linked to the focused finding
- The inspector pane shows a bounded code excerpt at the focused finding's
  evidence anchor. When the path or cwd cannot be resolved, it says so
  rather than guessing.
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
  cancels.
- `p`: open the elevated **post to GitHub** confirmation — only valid when
  the batch is `approved`. The confirmation word is `post`, deliberately
  distinct from `approve` so posting can never be confirmed by muscle
  memory. Confirming executes in-process through the shared `post_batch`
  op (real GitHub adapter) and reports the posted remote id, or a partial /
  failed / blocked outcome, truthfully. An `awaiting_approval` batch reports
  that it must be approved first; any other state reports why it is not
  postable.
- `Esc`: back to SessionHome

## DraftItems

- `j`/`k`: move
- `e`: edit the focused draft's body in `$EDITOR` — suspends the TUI, then
  persists the revision through the same storage-revision path a
  draft-edit command uses
- `Esc`: back to Drafts

## Timeline

- `j`/`k`: move across runs, stage results, and posted actions
- `Esc`: back to SessionHome

## Search

- `j`/`k`: move across hits
- `/`: enter the query input (type, `Enter` runs the search, `Esc` leaves the
  input, `Backspace` deletes)
- `m`: toggle between prior-review hits and the pending memory
  review-requests queue
- In review-requests mode: `a` accepts the candidate (promoting it into real
  memory), `x` rejects it, `m` returns to hits
- `Esc` (not editing the query): back to SessionHome
- The status strip and this screen both show a posture line — retrieval
  mode plus semantic-asset state — so degraded/promoted results are visibly
  distinct from tentative ones.

## Help overlay

`?` opens it from any screen; `Esc`, `?`, or `q` closes it. It reprints this
same key reference by screen, plus the mutation safety lines: triage writes
the same storage state as `rr triage`; approval requires typing `approve` in
the elevated prompt; posting to GitHub requires typing `post` in a distinct
elevated prompt.

## What the TUI deliberately does not do

- It does not re-expose install/update (`rr install`, `rr update`) or repair
  plumbing. Those mutate the binary and host outside a review, so they stay
  CLI-only.
- It is not the worker transport. `rr agent worker.*` is the in-session
  provider-worker protocol, not an operator action, and appears on no
  operator surface.
- It never bypasses elevation: every GitHub-visible mutation (approve, post)
  requires an exact typed confirmation word, and posting uses a different
  word than approving.
