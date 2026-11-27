# Render goldens (`xbrl-render`)

Golden outputs compared byte-for-byte by `crates/xbrl-render/tests/`. None of
these is vendored or official conformance data; no conformance claim is made.

Specification basis: XBRL 2.1 REC 2003-12-31 with errata to 2013-02-20
(presentation links §5.2.4, labels §5.2.2, contexts §4.7, units §4.8, accuracy
§4.6); XML 1.0 (Fifth Edition) character escaping; RFC 4180 CSV.

**Unsupported, not valid-by-inference:** XBRL Dimensions (any context with
segment/scenario content renders as an `[unsupported: dimensional context]`
column with values withheld), SEC/ESMA profiles and certification. Rendering
never decides validity.

| File | Provenance | Proves |
|---|---|---|
| `scenario.txt`, `scenario.html`, `scenario.csv` | `authored`: model built in `tests/render.rs::authored_scenario_matches_goldens` from the cited spec text; bytes `derived` by `XBRL_RENDER_BLESS=1 cargo test -p xbrl-render --test render` and then reviewed by hand | value / nil / missing / duplicate / unsupported cells, preferred-label and language selection, nested rows, HTML escaping, CSV quoting and formula defusing, flattened newlines |
| `example.txt`, `example.html`, `example.csv` | `derived` from the authored `fixtures/instance/basic.xml` and `fixtures/taxonomy/example.xsd` (+ linkbases) by `XBRL_RENDER_BLESS=1 cargo test -p xbrl-render --test smoke`, reviewed by hand | local-file smoke: instance + DTS → typed model → all three formats |

The CSV goldens use CRLF record terminators (RFC 4180); keep them byte-exact
(`.gitattributes`-style line-ending conversion would break the comparison).

Regenerating: run the command above, inspect `git diff fixtures/render`, and
commit only changes you can justify from the specs.
