# TOON Output Format Evaluation

Date: 2026-03-31  
Bead: `rr-3hh`

## Question

Assess whether TOON should be expanded as a robot-facing output format for
`roger-cli`, versus keeping JSON/compact JSON as the default machine contract.

## Current Roger State

- `roger-cli` already depends on `toon-format = 0.4.5`.
- `--robot-format toon` is currently allowed only for `rr status` and
  `rr findings`.
- Other robot commands fail closed when TOON is requested.

This means TOON is already present as a bounded optional surface, not a
greenfield decision.

## Primary Source Findings

### Spec maturity

- TOON spec is published as v3.0 (2025-11-24) and marked Working Draft:
  stable for implementation but not final.
- Media type is provisional (`text/toon`, not yet IANA registered).

Implication: usable now, but still carries spec-churn risk for broad contract
claims.

### Rust ecosystem support

- `toon-format` has an active Rust implementation and recent releases, with
  `0.4.5` published on 2026-03-30.
- The crate provides encoder/decoder APIs and strict validation controls.

Implication: Rust library support is strong enough for optional use in Roger.

### Bench evidence on benefits vs trade-offs

- Benchmarks report meaningful token savings in some structured scenarios.
- The same benchmark line also reports trade-offs: prompt overhead can reduce
  gains in short contexts, and JSON can retain better one-shot/final accuracy
  on some models/tasks.

Implication: TOON is not an unconditional win for robot output; benefit depends
on payload shape and model behavior.

## Recommendation for Roger `0.1.0`

1. Keep TOON optional and bounded for robot output.
2. Keep JSON as canonical default machine format.
3. Do not expand TOON to all robot commands yet.
4. Gate any TOON expansion (`rr-2hg`) behind command-level smoke evidence for:
   - parse round-trip correctness
   - stable model/backend behavior
   - net token/latency benefit against compact JSON

This matches the canonical plan direction: TOON is a useful optional packer,
not a required foundation.

## Suggested Promotion Criteria for `rr-2hg`

- Command payload is mostly uniform/tabular.
- TOON decode success and schema correctness are equal to JSON baseline in
  Roger-owned smoke tests.
- End-to-end token savings are material after prompt overhead.
- Degraded fallback to JSON remains explicit and automatic on TOON failure.

## Sources

- TOON spec reference: <https://toonformat.dev/reference/spec>
- TOON main repository: <https://github.com/toon-format/toon>
- Rust crate docs (`toon-format 0.4.5`): <https://docs.rs/crate/toon-format/0.4.5>
- Benchmark paper: <https://arxiv.org/abs/2603.03306>
