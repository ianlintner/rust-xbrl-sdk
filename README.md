# Rust XBRL SDK

A pure-Rust, headless XBRL SDK and toolchain for parsing, validating, and
rendering XBRL reports. **Status: bounded XBRL 2.1 instance reading, DTS
discovery, core validation, headless rendering and a local CLI are implemented;
the SDK is not conforming or certified.** See [PLAN.md](PLAN.md) for phased
scope and verifiable milestones, [docs/support-matrix.md](docs/support-matrix.md)
for the exact supported/unsupported matrix, and
[docs/mvp-integration.md](docs/mvp-integration.md) for end-to-end smoke evidence.

The first release targets XBRL 2.1 instances, common taxonomies/linkbases, core
diagnostics, headless rendering, and a CLI. Dimensions, Inline XBRL, OIM,
Formula, and regulatory profiles are later milestones. Unsupported standards
must be reported as unsupported, not silently accepted.

Standards index: https://specifications.xbrl.org/ . No UI or Python/Arelle runtime in the SDK. License: MIT.

## Repository layout

The workspace (`rust-toolchain.toml` pins the exact toolchain and components) has seven crates with one-directional dependency edges; the contract is written down in [docs/architecture.md](docs/architecture.md).

| Crate | Responsibility | Baseline state |
|---|---|---|
| `xbrl-core` | Typed model: qualified names, instance facts/contexts/units, diagnostics | Implemented (syntax subset; no DTS validation) |
| `xbrl-xml` | Bounded, namespace-aware XBRL 2.1 XML reading/extraction | Implemented (supported subset; no conformance claim) |
| `xbrl-taxonomy` | DTS resolution, schemas, concepts, linkbases | Implemented (local-only; common linkbases) |
| `xbrl-inline` | Inline XBRL (iXBRL) extraction | Scaffold (no items) |
| `xbrl-validate` | Core rule engine and profiles | Implemented (4 rule families; fail-closed gaps) |
| `xbrl-render` | Headless text/HTML/CSV rendering | Implemented (SDK formats; no filing renderer) |
| `xbrl-cli` | `xbrl` binary: `parse`, `validate`, `render`, `--json` | Implemented (versioned JSON envelope) |

`xbrl-core` and `xbrl-xml` provide the supported instance syntax subset;
`xbrl-taxonomy`, `xbrl-validate`, `xbrl-render` and `xbrl-cli` provide the MVP
slices documented in [docs/support-matrix.md](docs/support-matrix.md). Only
`xbrl-inline` remains a scaffold exposing no items. The `parse`, `validate` and
`render` subcommands run real local operations; `validate` without a DTS emits
`xbrl-validate:unsupported` and unknown subcommands are usage errors, so no
unimplemented path can print an empty success.

## Commands

Run from the repository root; they are the same commands CI (`.github/workflows/ci.yml`) runs.

```sh
cargo fmt --all -- --check                                    # formatting
cargo clippy --workspace --all-targets -- -D warnings         # lints (warnings are errors)
cargo test --locked --workspace                                # unit, integration and doc tests
cargo test --locked --workspace --doc                          # doc tests only
cargo bench --locked --workspace --no-run                      # build the criterion harness
cargo run -q -p xbrl-cli -- --json parse fixtures/instance/basic.xml  # CLI smoke
```

`--locked` matches CI. End-to-end CLI smoke and recorded results are in
[docs/mvp-integration.md](docs/mvp-integration.md); the first criterion baseline
is in [docs/benchmark-baseline.md](docs/benchmark-baseline.md). CI only builds
benchmarks; the recorded numbers come from an explicit local `cargo bench` run
with the exact command and toolchain documented there.

## Tests and fixtures

Behavior lives in unit tests inside each crate, in integration tests under `crates/*/tests/`, and in data-driven fixtures under [fixtures/](fixtures/) (provenance rules and file format: [fixtures/README.md](fixtures/README.md)). Fixture-driven tests assert a specific failure *kind*, not merely "an error occurred".
