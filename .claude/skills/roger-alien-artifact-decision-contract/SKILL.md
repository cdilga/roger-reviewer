---
name: Roger Alien Artifact Decision Contract
description: Use when making consequential Roger Reviewer decisions under uncertainty, especially retrieval scope expansion, finding elevation or suppression, fallback invocation, confidence calibration, evidence invalidation, or outbound-draft readiness. Applies decision theory, explicit evidence accounting, asymmetric error costs, confidence calibration, and compact explainability via an evidence ledger.
---

# Roger Alien Artifact Decision Contract

This is a project skill for Claude Code. It packages Roger's decision-theory and explainability workflow into a loadable skill.

For the canonical repo contract version used by Codex and other harnesses, see `docs/skills/ROGER_ALIEN_ARTIFACT_DECISION_CONTRACT.md`.

## Use this skill when

Apply this skill for consequential Roger decisions such as:
- whether to broaden retrieval from repo scope to project or org scope
- whether to elevate a tentative observation into a real finding
- whether to suppress, merge, downgrade, or invalidate a finding
- whether fallback or repair should trigger
- whether an output is strong enough to draft for explicit human approval
- whether refreshed evidence invalidates prior conclusions

Do not use this skill for trivial formatting or low-cost UI choices.

## Required workflow

1. State the decision exactly.
2. Enumerate the real action space, including conservative options.
3. List only actual evidence present in the current run or explicitly authorized memory.
4. Model asymmetric costs: false positives, false negatives, over-escalation, under-escalation, and explainability failure.
5. Update confidence from evidence only.
6. Prefer bounded, review-safe actions; if two actions are close, choose the more conservative one.
7. Trigger fallback when calibration is weak.
8. Emit an evidence ledger so the decision stays inspectable and replayable.

## Output shape

Structure the decision with these sections:
- Decision
- State space
- Evidence signals
- Loss model
- Confidence band
- Action
- Fallback trigger
- Evidence ledger

Use coarse confidence bands such as `low`, `guarded`, `strong`, and `very-strong`. Do not use fake decimal precision unless Roger has real calibration backing it.

Example ledger:

```json
{
  "decision": "expand_scope",
  "chosen_action": "project_scope",
  "confidence_band": "guarded",
  "evidence": [
    {"kind": "direct_repo_hits", "effect": "+"},
    {"kind": "cross_file_consistency", "effect": "+"},
    {"kind": "stale_project_memory", "effect": "-"}
  ],
  "fallback_triggered": false,
  "requires_human_confirmation": false
}
```

## Roger-specific constraints

Respect Roger's core constraints while using this skill:
- local-first
- repo-first by default
- no automatic GitHub posting
- no hidden mutation
- explicit human approval for outbound actions
- real OpenCode fallback

## Anti-patterns

Do not:
- silently broaden scope
- assign high confidence because the prose sounds persuasive
- let stale or cross-scope evidence dominate direct repo evidence without a visible penalty
- emit a strong finding without an evidence ledger
- hide pass disagreement or repair disagreement

## Minimal success condition

A later reviewer should be able to answer:
- what was being decided?
- what evidence moved the decision?
- what were the main error costs?
- why was this action chosen over the alternatives?
- what would have triggered fallback?

If those questions cannot be answered, this skill was not followed correctly.
