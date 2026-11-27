# MVP integration smoke

The three MVP slices (parse, validate, render) now run against the same authored
fixtures and are checked end-to-end by
`crates/xbrl-cli/tests/mvp_integration.rs`. This is a **bounded smoke test**, not
conformance evidence; see [support-matrix.md](support-matrix.md).

## One-shot check

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo test --locked -p xbrl-cli --test mvp_integration
cargo bench --locked --workspace --no-run
```

At this snapshot `cargo test --locked --workspace` reports **301 passed; 0
failed; 0 ignored; 0 measured; 0 filtered out** (unit, integration and doc
tests). CI runs the same gates with `--locked`; see
[`.github/workflows/ci.yml`](../.github/workflows/ci.yml).

## Fixture CLI smoke (from the repository root)

```sh
cargo run -q -p xbrl-cli -- --json parse fixtures/instance/basic.xml
cargo run -q -p xbrl-cli -- --json validate fixtures/instance/basic.xml \
  --dts fixtures/taxonomy --entry example.xsd
for f in text html csv; do
  cargo run -q -p xbrl-cli -- --json render fixtures/instance/basic.xml \
    --dts fixtures/taxonomy --entry example.xsd --format "$f"
done
cargo run -q -p xbrl-cli -- --json parse fixtures/instance/invalid-root.xml
```

Expected outcomes:

| Command | Exit | Envelope summary |
| --- | ---: | --- |
| `parse basic.xml` | 0 | `schema_refs=1`, `contexts=2`, `units=2`, `facts=4`, no diagnostics |
| `validate basic.xml --dts … --entry example.xsd` | 0 | rules `structural`, `references`, `duplicate-consistency`, `calculation`; 0 findings |
| `render basic.xml … --format text/html/csv` | 0 | 1 report; identical label/value states across all three formats, matching the [render goldens](../fixtures/render/README.md) |
| `parse invalid-root.xml` | 1 | `xbrl-instance:invalid-root` error with half-open byte span `{start:39,end:104}` |

`validate` and `render` on `invalid-root.xml` return the same fail-closed finding;
`validate` without `--dts` returns `xbrl-validate:unsupported` rather than a
clean result. Every `--json` run emits exactly one line on stdout and nothing on
stderr, with schema id `xbrl-sdk/cli-result/v1`
([agent tool contract](agent-tool-contract.md),
[JSON schema](cli-result.schema.json)).

The integration test additionally asserts that the chain is byte-for-byte
deterministic when repeated, that the three render formats agree on the same
visible states (`Total revenue`, `[missing]`, no `[unsupported` marker for this
fixture), and that the CSV envelope has one header and three data rows.

## Architectural note

`xbrl-taxonomy::Dts::graph_*` is the single canonical graph traversal. The
`presentation_*` and `calculation_*` helpers only fix the `LinkbaseKind`, so a
presentation-named call can no longer request a calculation graph.
`GraphChild` carries the resolved concept and one `Relationship`; arc order,
weight and `preferredLabel` are read through `GraphChild::edge()`, removing the
duplicated copies that could drift. `xbrl-render` still builds its own
presentation row state from `RenderInput` (ELR partition, input-order ties and
`[cycle]` / `[already shown]` markers) because rendering formats the typed model
and must never re-parse source text (architecture rule 3).
