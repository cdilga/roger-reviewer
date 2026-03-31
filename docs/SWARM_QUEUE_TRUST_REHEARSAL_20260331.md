# Swarm Queue-Trust Rehearsal (2026-03-31)

## Scope

Bounded operator rehearsal for `rr-qoq.5` to validate the remediation bundle from:

- `rr-qoq.1` preflight guard
- `rr-qoq.2` short-form worker prompt + doctrine split
- `rr-qoq.3` bead-batch audit preflight
- `rr-qoq.4` maintenance-lane separation guidance

This rehearsal intentionally did **not** launch a large swarm.

## Commands Run

```bash
scripts/swarm/preflight_swarm.sh --session roger-reviewer-rehearsal --codex 1 --claude 0 --gemini 0 --opencode 0
scripts/swarm/audit_bead_batch.sh --limit 10 --strict
scripts/swarm/build_prompt.sh docs/swarm/overnight-marching-orders.md DemoImpl <tmp> implementation
scripts/swarm/build_prompt.sh docs/swarm/overnight-marching-orders.md DemoMaint <tmp> maintenance
```

## Results

### 1) Preflight behavior (fail-loud classification)

Observed output:

- `PREFLIGHT_STATUS=fail`
- `PREFLIGHT_CLASS=operator_actionable`
- `PREFLIGHT_REASON=br doctor failed`
- exit code `1`

Interpretation:

- Startup failure is now explicit and classified as operator-actionable instead of silently degrading into queue confusion.
- Transient retry class (`PREFLIGHT_CLASS=transient_retry`, exit `75`) is implemented but was not hit in this run.

### 2) Queue audit behavior

Observed output (`--strict`, `--limit 10`):

- ready issues: `10`
- open issues: `20`
- warnings: `9`
- errors: `0`
- exit code: `2` (strict mode warning gate)

Interpretation:

- Queue-truth ambiguity is surfaced before launch.
- Remaining warnings are mostly acceptance-criteria gaps and missing-leaf shape on several feature/epic beads.

### 3) Prompt-tax reduction

Measured generated prompt lengths:

- implementation worker prompt: `43` lines
- maintenance worker prompt: `43` lines
- long-form doctrine reference: `81` lines

Interpretation:

- Worker startup prompt is now short-form and behavior-focused.
- Long doctrine remains available via explicit authority links.

## What ambiguity/startup cost was removed

- A canonical launch guard now classifies startup failures (`operator_actionable` vs `transient_retry`) before swarm launch.
- Worker prompt vs doctrine split is explicit in both docs and script-generated prompts.
- Lane intent (implementation vs maintenance) is surfaced in generated worker prompts with reduced repeated control-plane text.

## What still remains

- `br doctor` currently fails in this workspace; remediation still required before trusting larger launch throughput.
- Audit still reports warning-heavy frontier quality (`9` warnings in first `10` ready items).
- Some ready beads still lack explicit acceptance criteria and/or clean leaf decomposition.

## Queue-trust verdict

Queue trust is improved enough for **small bounded runs with explicit operator checks**, but not yet ready for another large uncontrolled swarm launch. The remediation lane is directionally successful; next step is to reduce warning density and stabilize `br doctor` health before scaling worker count.
