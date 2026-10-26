# Rust XBRL SDK

A pure-Rust, headless XBRL SDK and toolchain for parsing, validating, querying, authoring, and rendering XBRL reports. **Status: planning/scaffold; not yet a conforming parser or validator.** See [PLAN.md](PLAN.md) for phased scope and verifiable milestones.

The first release targets XBRL 2.1 instances, common taxonomies/linkbases, core diagnostics, headless rendering, and a CLI. Dimensions, Inline XBRL, OIM, Formula, and regulatory profiles are later milestones. Unsupported standards must be reported as unsupported, not silently accepted.

Standards index: https://specifications.xbrl.org/ . No UI or Python/Arelle runtime in the SDK. License: MIT.

## Repository layout

The workspace (`rust-toolchain.toml` pins the exact toolchain and components) has seven crates with one-directional dependency edges; the contract is written down in [docs/architecture.md](docs/architecture.md).

| Crate | Responsibility | Baseline state |
|---|---|---|
| `xbrl-core` | Typed model: qualified names, XBRL namespaces, diagnostics | QName + diagnostic envelope implemented |
| `xbrl-xml` | Bounded, namespace-aware XBRL 2.1 XML reading | Scaffold, no parser |
| `xbrl-taxonomy` | DTS resolution, schemas, concepts, linkbases | Scaffold |
| `xbrl-inline` | Inline XBRL (iXBRL) extraction | Scaffold |
| `xbrl-validate` | Core rule engine and profiles | Scaffold |
| `xbrl-render` | Headless text/HTML/CSV rendering | Scaffold |
| `xbrl-cli` | `xbrl` binary (`--help`, `--version` only) | Scaffold |

"Scaffold" means the crate is a workspace member with pinned dependency edges and no implementation; it exposes no items and cannot report a passing result it did not compute. The `parse`, `validate` and `render` subcommands of `xbrl` are recognised and exit `3` (not implemented) rather than printing an empty success.

## Commands

Run from the repository root; they are the same commands CI (`.github/workflows/ci.yml`) runs.

```sh
cargo fmt --all -- --check                                    # formatting
cargo clippy --workspace --all-targets -- -D warnings         # lints (warnings are errors)
cargo test --workspace                                        # unit, integration and doc tests
cargo test --workspace --doc                                  # doc tests only
cargo bench -p xbrl-core --no-run                             # build the criterion harness
cargo run -p xbrl-cli -- --help                               # CLI smoke
```

`cargo bench -p xbrl-core` runs the baseline QName micro-benchmarks over `fixtures/qname/lexical.tsv`. They are a harness baseline only — there is no performance claim until a baseline is recorded with reproducible provenance (PLAN.md milestone 8), so CI builds the benchmarks but does not run them.

## Tests and fixtures

Behavior lives in unit tests inside each crate, in integration tests under `crates/*/tests/`, and in data-driven fixtures under [fixtures/](fixtures/) (provenance rules and file format: [fixtures/README.md](fixtures/README.md)). Fixture-driven tests assert a specific failure *kind*, not merely "an error occurred".
