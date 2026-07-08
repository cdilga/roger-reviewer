# Roger TUI Cheatsheet

Status: reusable Roger skill.

Purpose:
Exact keys and screens for Roger's local review cockpit (`rr open` /
`rr tui`) — Home/finder, Picker, SessionHome, Findings triage, Drafts
approve/edit, Timeline, Search, and the help overlay. No aspirational keys,
only what `packages/tui` actually implements.

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
- `c`: clarify hint — shows "clarification requires an active worker
  session — run rr review/resume"; this is advisory text, not a live
  clarify action
- `Esc`: back to Home (reloads sessions)

## Findings

- `j`/`k`: move
- `space`: toggle multi-select on the current finding
- `a`/`i`/`n`/`r`: triage the current selection (or the cursor row if nothing
  is selected) to accepted/ignored/needs_follow_up/resolved
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
  cancels. An already-approved batch instead reports that posting stays
  CLI-only (`rr post --batch <id>`); any other state reports why it isn't
  approvable right now.
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
- `Esc` (not editing the query): back to SessionHome
- The status strip and this screen both show a posture line — retrieval
  mode plus semantic-asset state — so degraded/promoted results are visibly
  distinct from tentative ones.

## Help overlay

`?` opens it from any screen; `Esc`, `?`, or `q` closes it. It reprints this
same key reference by screen, plus two safety lines: a mutation note
("triage writes the same storage state as `rr triage`"; "approval requires
typing approve in the elevated prompt") and the elevation hint
`POSTING STAYS CLI-ONLY: rr post --batch <id>`.

## What the TUI deliberately does not do

- It never posts to GitHub. `rr send post` / `rr post` stay CLI-only in
  every case — the Drafts screen only ever shows you the equivalent command,
  it never runs it for posting.
- It does not launch providers from inside the TUI in this slice — launching
  is deferred, and the TUI shows the exact `rr` command to run in a shell
  instead of running it for you.
- Clarify (`c`) is an advisory hint, not a live chat lane, until a worker
  session actually exists.
