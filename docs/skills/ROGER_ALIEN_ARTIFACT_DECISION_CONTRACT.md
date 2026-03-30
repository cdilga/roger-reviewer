# Roger Alien Artifact Decision Contract

Status: reusable Roger skill.

Purpose:
Use this skill when Roger must make a consequential decision under uncertainty
and we want decision theory, confidence calibration, and explainability rather
than intuition theater.

This skill is **adapted** from Dicklesworthstone's public reasoning-contract
style, not copied from one literal file with this exact name.

## When to use it

Use this contract for decisions such as:

- whether to broaden retrieval scope from repo to project or org
- whether to elevate a tentative observation into a real finding
- whether to suppress, merge, or downgrade a noisy finding
- whether to invoke fallback or repair flows
- whether a review output is strong enough to draft for human approval
- whether a refresh invalidates prior evidence

Do not use it for trivial UI choices or low-cost formatting decisions.

## Core operating rule

No consequential Roger decision should rest on vibes, hidden chain-of-thought,
or uninspected heuristics.

For consequential decisions, Roger should:

1. define the decision clearly
2. enumerate the actual available evidence
3. state the asymmetric costs of being wrong
4. update confidence only from explicit evidence
5. choose the action with the best expected downside profile
6. trigger fallback when calibration is weak or evidence is out of distribution
7. emit a compact evidence ledger entry

## Required output shape

Every use of this contract should produce the following sections.

### 1. Decision

State exactly what Roger is deciding.

Example:

- expand retrieval from repo scope to project scope
- emit a `needs-follow-up` finding instead of an `accepted` finding
- invalidate a draft because the underlying evidence anchor moved

### 2. State space

List the real options.

Example:

- keep repo scope only
- expand to project scope
- expand to org scope
- stop and require human confirmation

### 3. Evidence signals

List only evidence actually available in the current run.

Examples:

- direct code evidence count
- retrieval score separation
- prior accepted finding similarity
- evidence freshness or staleness
- repair-pass disagreement
- scope provenance quality

Do not invent evidence that Roger does not possess.

### 4. Loss model

State the costs of error.

At minimum include:

- false positive cost
- false negative cost
- over-escalation cost
- under-escalation cost
- explainability cost if the decision cannot later be justified

Roger should strongly prefer bounded, review-safe errors over expansive,
opaque ones.

### 5. Confidence update

Confidence must be tied to evidence, not performance.

Rules:

- start from a conservative prior
- move confidence only when explicit evidence justifies it
- penalize cross-scope or stale evidence
- penalize disagreement between passes
- reduce confidence when similar past findings were rejected

Prefer coarse calibrated bands over fake decimal precision.

Suggested bands:

- `low`
- `guarded`
- `strong`
- `very-strong`

### 6. Action rule

Choose the action that minimizes expected review harm subject to Roger's
constraints:

- local-first
- repo-first by default
- no automatic GitHub posting
- no hidden mutation
- explicit human approval for outbound actions
- real OpenCode fallback

If two actions are close, choose the more conservative one.

### 7. Fallback trigger

Fallback is required when any of the following hold:

- evidence is stale or contradictory
- calibration is weak
- a decision depends on non-repo context that was not explicitly authorized
- a proposed finding matters materially but lacks sufficient direct evidence
- repair or normalization passes disagree materially

Fallback actions may include:

- downgrade confidence
- mark `needs-follow-up`
- keep draft local only
- require explicit human confirmation
- rerun with narrower or broader evidence under visible policy

### 8. Evidence ledger

Emit a compact, inspectable ledger.

Suggested shape:

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

The ledger should be readable in the TUI without exposing hidden reasoning.

## Prompt form

Use this prompt when you want an agent or model to operate under the contract.

```text
Operate under Roger's Alien Artifact Decision Contract.

For the current decision:
1. State the exact decision.
2. Enumerate the real action space.
3. List only the evidence actually available.
4. State the asymmetric costs of false positives, false negatives,
   over-escalation, and under-escalation.
5. Update confidence only from explicit evidence.
6. Choose the action with the best expected downside profile under Roger's
   constraints.
7. Trigger fallback if calibration is weak, evidence is stale, or the decision
   depends on unauthorized broader scope.
8. Output a compact evidence ledger.

Do not use vibes, hidden assumptions, or fake precision.
Prefer bounded, review-safe decisions.
```

## Roger-specific guidance

This contract is especially important because Roger is not just a chatbot.
Roger is a review system with durable findings, scope rules, approval gates,
and future replay/audit needs.

That means the contract should optimize for:

- inspectability
- replayability
- bounded harm
- explicit scope provenance
- graceful degradation under uncertainty

## Anti-patterns

Do not do the following:

- assign high confidence because the wording sounds persuasive
- silently broaden scope
- let historical memory override direct repo evidence without visible penalty
- emit a strong finding without a ledger
- hide disagreement between passes
- present unsupported certainty as explainability

## Minimal acceptance test for using this skill

A decision that used this skill should let a later reviewer answer:

- what was being decided?
- what evidence moved the decision?
- what were the main error costs?
- why was this action chosen over the next-best alternative?
- what would have triggered fallback?

If those questions cannot be answered, the contract was not followed.
