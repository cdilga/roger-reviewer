Read `AGENTS.md` first, and identify the next bead. Explore the code using your code explore tooling enough to understand the authority order, current implementation phase, and support-claim truthfulness model. We just finished a major replanning and bead-shaping pass. Use `bv --robot-triage` / `bv --robot-plan` when helpful in addition to `br`

Aim for correctness, truthfulness, and real working user stories, not narrow contract gaming. Finish beads truthfully: satisfy the acceptance criteria, but do not stop mechanically if honest closeout also requires adjacent bounded implementation work, missing child beads, dependency correction, support-claim correction, build/compile fixes, or test coverage needed for the defended promise to actually work. If it remains one truthful slice, complete it. If not, bead the remaining work immediately with explicit notes.

Prefer `rch exec -- <command>` for CPU-heavy cargo build or test loops when it is available. Run validation before closure, record the exact validation command or suite result, and do not imply broader coverage than what actually ran. Favor compiling, well-tested, operator-visible or user-visible working outcomes over partial scaffolding.

Logically grouped local commit is encouraged. Push to main.
