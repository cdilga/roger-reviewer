# Dicklesworthstone Source Extraction Notes for Roger Reviewer

Status: extracted-source notes for reusable Roger skills.

This document records what was actually found in Dicklesworthstone's public
materials and what was distilled from repeated patterns rather than copied from
one literal file.

## What was found directly

### 1. A literal `SKILL.md`

The `coding_agent_session_search` repository contains a real `SKILL.md` file.
It is a machine-facing usage contract for `cass`, with:

- explicit robot-mode rules
- pre-flight checks
- command recipes
- JSON output contracts
- token-budget controls
- structured error handling
- a self-documenting API surface

Roger should treat this as evidence that Dicklesworthstone does sometimes encode
operational methodology as reusable skill files rather than burying everything
inside prose docs.

### 2. A literal optimization contract

The `coding_agent_session_search` repository contains
`PLAN_FOR_ADVANCED_OPTIMIZATIONS_ROUND_1__GPT.md`.
That file is the clearest direct source for the methodology we are calling
"extreme software optimization" in Roger.

Its hard requirements are concrete:

- baseline first
- profile before proposing
- define an equivalence oracle
- provide an isomorphism proof sketch
- rank opportunities by `(Impact × Confidence) / Effort`
- make one minimal, rollback-friendly change at a time
- reject output-changing performance work unless it is explicitly gated

This is not a vibe. It is an explicit written operating contract.

### 3. A family of explicit reasoning/codegen contracts in public gists

Dicklesworthstone's public gists include multiple files with names such as:

- `combined_hvm_prompt.txt`
- `optimal_safe_codegen_reasoning_contract_hvm_interaction_calculus_runtimes.txt`
- `cost_based_bushy_sql_join_order_optimizer.hvm`

These repeatedly use language like:

- "Optimal & Safe Codegen/Reasoning Contract"
- precise definitions up front
- explicit scope boundaries
- branch independence
- superposition-driven search
- no cross-branch leakage
- correctness constraints before cleverness

Those files are not about Roger's domain, but they are strong evidence for a
repeatable Dicklesworthstone pattern:
write the reasoning contract first, define invariants explicitly, and do not let
branch state or unsafe shortcuts contaminate the result.

## What was *not* found as one literal file

The exact phrases below were not found as one canonical public file title in the
materials inspected for this pass:

- `alien-artifact-coding`
- `extreme-software-optimization`

So Roger should be honest here:

- `extreme software optimization` is a Roger-level label applied to a very real,
  explicit optimization methodology found in `cass`
- `alien artifact` is a Roger-level label applied to a family of
  contract-oriented, first-principles, branch-isolating reasoning prompts and
  specs found across Dicklesworthstone materials

That means the Roger skills created from these sources are **adapted extractions**,
not verbatim imports.

## What Roger should import

### Import directly

- skill-file shape for machine-facing workflows
- optimization contract discipline
- proof-backed equivalence language
- rollback-friendly minimal diffs
- explicit invariants and failure boundaries

### Import carefully

- branch-isolation logic
- superposition / decision-tree style exploration
- high-formality reasoning contracts

These should be adapted to Roger's review-safe workflow rather than copied as if
Roger were an HVM or optimizer project.

## What Roger should not pretend

Roger should not claim:

- that Dicklesworthstone published one canonical file named
  `alien-artifact-coding`
- that the Roger alien-artifact skill is a verbatim transcription
- that all of the extracted methodology came from `frankentorch` alone

The honest statement is:
Roger's skill files below are distilled from a real Dicklesworthstone skill file,
from a real explicit optimization methodology document, and from a family of
public reasoning-contract gists that show a recurring design pattern.

## Roger-native outputs created from this extraction

- `ROGER_ALIEN_ARTIFACT_DECISION_CONTRACT.md`
- `ROGER_EXTREME_SOFTWARE_OPTIMIZATION.md`

These are intended as reusable prompt/skill artifacts for planning,
implementation, review, and critique inside Roger Reviewer.
