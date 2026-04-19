# rr-skdz README Truth Closeout

Date: 2026-04-19
Bead: `rr-skdz`

## Scope

Performed the final README public-surface truth pass against live CLI/help contracts and release/install asset naming. This closeout tightens deterministic proof for:

- install script asset naming and URLs (`rr-install.sh`, `rr-install.ps1`)
- browser companion support wording and command surfaces (`edge|chrome|brave`)
- provider-lane wording consistency with the `0.1.0` bounded support posture
- hygiene checks that README stays free of obvious maintainer-only/dev-only leakage

## Changes

- added `packages/cli/tests/readme_public_surface_truth_smoke.rs` to enforce:
  - published installer URLs in `README.md`
  - supported browser wording and extension setup/doctor command strings
  - absence of stale/dev-only wording fragments (`ngrok`, dated status phrasing)
  - release workflow retention of `rr-install.sh`/`rr-install.ps1` installer assets

## Validation

- `cargo test -q -p roger-cli --test readme_public_surface_truth_smoke -- --nocapture`
- `cargo test -q -p roger-cli --test provider_surface_truth_guard -- --nocapture`

Both commands passed on this lane.

## Notes

- Agent Mail transport on `127.0.0.1:8765` was unavailable during this lane, so file reservation calls failed at transport-level. Work stayed bounded to README truth-guard coverage and deterministic docs-surface assertions.
