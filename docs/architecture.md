# Architecture and dependency boundaries

Status: **scaffold baseline** (PLAN.md milestone 1). Only `xbrl-core` contains
code; the other six crates are members of the Cargo workspace with declared
dependency edges and no implementation. This document is the contract those
crates are expected to keep as milestones land, so that parsing, validation and
rendering do not grow into one another.

## Crates and their allowed dependencies

| Crate | May depend on | Milestone |
|---|---|---|
| `xbrl-core` | — (workspace root of the graph) | 1 |
| `xbrl-xml` | `xbrl-core` | 2 |
| `xbrl-taxonomy` | `xbrl-core`, `xbrl-xml` | 4 |
| `xbrl-inline` | `xbrl-core`, `xbrl-xml` | post-MVP |
| `xbrl-validate` | `xbrl-core`, `xbrl-xml`, `xbrl-taxonomy` | 5 |
| `xbrl-render` | `xbrl-core`, `xbrl-taxonomy` | 6 |
| `xbrl-cli` | `xbrl-core`, `xbrl-validate`, `xbrl-render` | 7 |

Rules that make the boundaries load-bearing:

1. **No cycles, no upward edges.** `xbrl-core` is depended on, never depending.
   Validation and rendering are terminal: nothing in the workspace depends on
   `xbrl-validate`, `xbrl-render` or `xbrl-cli`.
2. **Parsing is not validation.** `xbrl-xml` produces syntax-level events and
   `xbrl-core` holds the typed model; neither decides whether a document is
   valid. Validity is only ever decided in `xbrl-validate`.
3. **Rendering never re-parses.** `xbrl-render` formats the typed model. If a
   renderer needs source text it is missing, the model is incomplete — fix the
   model, not the renderer.
4. **One diagnostic shape.** Every crate reports findings as
   `xbrl_core::diagnostic::Diagnostic` with a stable `RuleId`, a `Severity` and an
   optional `SourceSpan` (byte offsets). No crate invents a second error type for
   user-visible problems, and no rule id is renamed without treating it as a
   breaking change (consumers filter on it).
5. **Fail closed.** A construct the SDK does not implement must be reported as
   unsupported — never silently accepted and never counted as passing. This is
   why `xbrl` exits `3` for its reserved subcommands today instead of printing an
   empty success.
6. **No remote I/O by default.** Resolvers read local files only; any future
   network path is opt-in, explicitly bounded (size, depth, cycles) and
   documented.

## Test and fixture strategy

* Behavior lives behind unit tests inside each crate (`#[cfg(test)]`) plus
  integration tests under `crates/*/tests/`.
* Cross-crate or data-driven expectations live in `fixtures/`, text-first, with
  provenance recorded per PLAN.md and `fixtures/README.md`. Fixtures are vendored
  only after a license review; until then they stay `authored`.
* Fixture-driven tests assert a specific failure *kind*, not merely "an error
  occurred" — see `crates/xbrl-core/tests/qname_fixtures.rs`.
* Benchmarks use criterion (`cargo bench -p xbrl-core`) over the same fixtures.
  CI only builds them (`--no-run`); there is no performance claim until a
  baseline is recorded with reproducible provenance (PLAN.md milestone 8).

## Adding a crate

Add it to `[workspace.members]` and `[workspace.dependencies]`, inherit
`version`/`edition`/`rust-version`/`license` from `[workspace.package]`, opt into
`[lints] workspace = true`, add the crate's row to the table above, and state its
boundary in the module-level docs. Keep `unsafe_code = "forbid"` and
`missing_docs = "warn"` intact.
