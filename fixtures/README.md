# Test fixtures

Fixtures are **text-first and provenance-labelled**: every fixture directory has
a README (or a header comment inside the file) stating where the data came from,
which specification revision it targets, and what it is expected to prove. No
fixture may be added whose expected result we cannot justify from a cited source
or from a hand-computed derivation.

## Provenance rules

* `authored` — written for this repository from the cited specification text.
* `vendored` — copied from an official test suite or taxonomy package. A
  vendored fixture must record the upstream URL, the upstream revision/date, the
  license and the exact files copied.
* `derived` — computed from another artifact in this repository; record the
  command that produced it.

Third-party corpora are only vendored after a license review; until then a
fixture stays `authored`, and the corresponding conformance claim stays open
(PLAN.md: report unsupported, never fabricated coverage).

## Layout

| Path | Contents |
|---|---|
| `qname/lexical.tsv` | `xbrl-core` qualified-name lexical cases (authored). |

## File format

Tab-separated, `#` comments and blank lines ignored, one case per line:

```
id <TAB> input <TAB> expected_namespace <TAB> expected_local_name <TAB> expected_error_kind
```

`expected_error_kind` is `-` for a case that must parse, otherwise the error
variant the parser must report (snake_case, e.g. `empty_namespace`).
