# Contributing

Roger Reviewer uses an issue-first contribution path.

If you want to help, start with an issue:

- bug report
- feature request
- workflow pain point
- architectural concern
- docs or onboarding problem

The best issues include:

- what you were trying to do
- what Roger should have done
- what happened instead
- the command, surface, or browser involved
- screenshots or terminal output when they clarify the problem

If a change should exist, discuss and shape it in an issue first so scope,
support claims, and validation expectations are clear before implementation
work starts. Code contributions and PRs should follow that coordinated path
rather than bypassing it.

## Getting set up

Start with [`docs/DEV_MACHINE_ONBOARDING.md`](docs/DEV_MACHINE_ONBOARDING.md)
for the practical machine setup (provider access, planning workflow, and the
Copilot operator contract).

### Toolchain

Roger's source tree is pinned to the Rust **nightly** channel through
[`rust-toolchain.toml`](rust-toolchain.toml). The workspace language edition
stays `2024`; the Rust edition and the compiler channel are separate settings,
so "Rust 2024 edition" does not mean a stable toolchain. Run Cargo commands
from the repo root so the repo-local nightly override is picked up
automatically:

```bash
rustup update nightly
cargo test --workspace --all-targets
```

## Versioning

`0.2` is the current product milestone. Published releases are CalVer-tagged
(`vYYYY.MM.DD`) and the Cargo workspace semver stays `0.1.0`. See
[`docs/RELEASE_CALVER_VERSIONING_CONTRACT.md`](docs/RELEASE_CALVER_VERSIONING_CONTRACT.md)
for the canonical version authority.
