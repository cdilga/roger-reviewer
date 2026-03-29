# Exploration References

This directory is reserved for local reference clones used during planning and
architecture spikes.

Current reference targets:

- `frankentui` for the Rust TUI runtime and model constraints
- `cass` for Tantivy/FastEmbed search patterns and local-first indexing
- `asupersync` for daemonless bridge and runtime reference ideas

These clones are intentionally excluded from the initial GitHub snapshot because
they are large, local working copies rather than Roger Reviewer source.

Rules:

- treat these repos as references, not dependencies
- do not import code from them directly into Roger Reviewer
- document any decisions they drive in `docs/` or ADR-style notes
- use `/tmp/roger-reference-projects/` for larger or more temporary spike
  clones that are worth reviewing locally but do not belong in the repo
- keep the current approved external-source and exploration-target list in
  `docs/REFERENCE_SOURCES_AND_EXPLORATION_TARGETS.md`
