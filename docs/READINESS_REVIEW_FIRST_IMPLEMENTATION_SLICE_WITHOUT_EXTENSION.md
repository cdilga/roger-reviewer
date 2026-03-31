# Readiness Review: First Implementation Slice Without The Extension

This document closes `rr-q18`.

It answers one narrow readiness question: can Roger start implementation
truthfully without needing the browser extension immediately?

## Verdict

Yes for the first implementation slice.

The current plan, ADR set, and bead graph support starting the first slice
without extension work, provided Roger stays honest about what that slice is:

- the first slice is the local foundation path identified in prior readiness
  work: `rr-013 -> rr-015 -> rr-014`
- that slice establishes core domain schema, harness/session linkage, and local
  storage
- it does not yet promise a usable browser launch flow or extension-backed
  readback behavior

No hidden extension dependency currently blocks that first slice.

## Evidence

### 1. The canonical plan keeps the extension optional and late

The canonical plan says the browser extension is optional and Roger must remain
coherent when used entirely from the shell and local TUI. It also places
extension work in Phase 4, after:

- Phase 1 foundation and domain work
- Phase 2 CLI and prompt-engine work
- Phase 3 TUI work

That sequencing is consistent with the readiness question. The first slice does
not need extension delivery in order to begin.

### 2. The local product has an explicit install/release lane separate from the extension

The canonical plan and the release matrix both freeze the same rule:

- the blessed one-line install path targets the local `rr` product first
- bridge registration and extension packaging are separate optional lanes
- Roger may ship an honest local CLI/TUI release without claiming browser
  launch support

This removes a common hidden dependency: the extension is not part of the base
bootstrap contract for early implementation.

### 3. The bead graph isolates extension packaging into later work

The remaining extension-packaging bead is `rr-007.1`, and its dependents are
extension- or bridge-facing work such as `rr-021`, `rr-022`, and `rr-025`.

By contrast, the first implementation slice identified by prior readiness work
is:

- `rr-013` Define core domain schema and finding fingerprint model
- `rr-015` Define the Roger-to-harness session linkage
- `rr-014` Implement local storage, migrations, and artifact budget classes

Those beads do not depend on `rr-007.1` or any extension artifact lane.

### 4. The flow and validation docs already treat local-first as the proving path

The review flow matrix defines local entry/resume flows before browser launch:

- `F01` Enter or Resume a Review Locally
- `F01.1` Reinvoke Roger in the Current Repo and Pick Up the Right Session
- `F01.2` Global Session Finder and Cross-Repo Jump

The release/test matrix likewise makes the first blessed automated end-to-end
path the core review loop without the browser.

That means the extension is not the first proving ground for Roger's truth
claims. The local path is.

## What This Does Not Prove

This finding is narrower than "Roger is ready for a full user-visible review
loop."

It does not prove that:

- the first usable local review loop is ready today
- CLI review/resume/findings/status/refresh work is fully specified end to end
- TUI usability or outbound approval flow is ready
- the bridge packaging lane is finished

Those concerns belong to later beads such as `rr-018`, `rr-019`, `rr-020`,
`rr-021`, and `rr-022`, plus the separate readiness synthesis in `rr-3ve`.

## Residual Caveats

The first slice can begin without the extension, but implementation should stay
explicit about these boundaries:

- do not describe early binaries as browser-launch capable unless the matching
  bridge registration and extension artifacts actually ship
- do not let CLI/TUI contract work silently depend on extension-specific
  message shapes or host-install assumptions
- do not let "extension deferred" turn into "local loop unspecified"; the local
  proving path still depends on `rr-018`, `rr-016`, and `rr-017` after the
  foundation slice

## Conclusion

`rr-q18` should be considered satisfied.

Roger's first implementation slice can start without the browser extension
because the authoritative docs consistently separate:

- foundational local-domain work
- local CLI/TUI product viability
- later browser bridge and extension delivery

No new blocker bead is required from this review. The remaining work is to keep
the implementation gate synthesis honest about the difference between "the first
slice can begin" and "the whole local review loop is ready."
