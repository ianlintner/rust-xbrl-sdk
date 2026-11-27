# Architecture and dependency boundaries

Status: **milestones 1–7 implemented (bounded slices); milestone 8 records
integration, benchmark provenance and limitations** (PLAN.md). `xbrl-core` holds
the typed syntax model, `xbrl-xml` provides bounded XML/instance extraction, and
`xbrl-taxonomy`, `xbrl-validate`, `xbrl-render` and `xbrl-cli` implement the MVP
slices. `xbrl-inline` remains a scaffold. This is **not** XBRL conformance;
[support-matrix.md](support-matrix.md) lists the exact scope.

## Crates and their allowed dependencies

| Crate | May depend on | Milestone |
|---|---|---|
| `xbrl-core` | — (workspace root of the graph) | 1, 3 |
| `xbrl-xml` | `xbrl-core` (external quick-xml, chrono) | 2, 3 |
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
   unsupported — never silently accepted and never counted as passing. `xbrl`
   emits `xbrl-validate:unsupported` when validation is requested without a DTS
   and treats unknown subcommands as usage errors; it never prints an empty
   success for work it did not do.
6. **No remote I/O by default.** Resolvers read local files only; any future
   network path is opt-in, explicitly bounded (size, depth, cycles) and
   documented.
7. **One canonical graph traversal.** `xbrl-taxonomy::Dts::graph_*` is the single
   relationship walker. `presentation_*` and `calculation_*` helpers only fix the
   `LinkbaseKind`; they are not parallel traversal implementations. A
   `GraphChild` carries the resolved concept plus one `Relationship`, and arc
   order, weight and preferred label are read through `GraphChild::edge()` so no
   copied edge fields can drift. `xbrl-render` builds its own presentation row
   state from the typed `RenderInput` (ELR partition, input-order ties and
   `[cycle]` / `[already shown]` markers) under rule 3; it does not duplicate a
   second DTS graph walk.

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
