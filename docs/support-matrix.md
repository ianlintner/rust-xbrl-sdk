# Supported / unsupported matrix (MVP)

Status snapshot for the merged MVP slices through PR #8 (`c1a53ee`). This is an
**evidence table, not a conformance claim**: every official suite linked below
has **0 executed cases** in this repository. `Implemented` means a bounded slice
with authored fixtures and tests; `Partial` means that slice plus an explicit
unsupported report for the gaps; `Not implemented` means there is no code path
that can claim a successful result.

Totals at this snapshot: **301 tests pass** (`cargo test --locked --workspace`,
0 failed). Authored data: 14 QName lexical cases, 66 instance negative cases,
12 broken taxonomy schemas, 8 taxonomy fix schemas, 6 rendering goldens. All
fixtures are `authored`; no third-party corpus has been vendored
([fixtures/README.md](../fixtures/README.md)).

| Area | Normative reference (pinned in [standards-catalog](../skills/xbrl-agent/references/standards-catalog.md)) | Status | Evidence and explicit limits |
| --- | --- | --- | --- |
| Qualified names / namespaces | XML Namespaces 1.0 3rd ed.; XML Schema Part 2 (NCName); XBRL 2.1 §3 | Implemented (lexical subset) | `xbrl-core::QName`; 14 authored cases in `fixtures/qname/lexical.tsv`, `xbrl-core::benches::qname`. |
| XBRL 2.1 instance reading | [XBRL 2.1 REC 2003-12-31 + corrected errata 2013-02-20](https://www.xbrl.org/Specification/XBRL-2.1/REC-2003-12-31/XBRL-2.1-REC-2003-12-31+corrected-errata-2013-02-20.html) (suite: [XBRL-CONF 2025-07-16](https://www.xbrl.org/2025/XBRL-CONF-2025-07-16.zip)) | Partial | `xbrl-xml` bounded reader + typed `Instance` (schema refs, contexts, units, direct item facts, extension nodes). BOM/entity/depth/size limits and typed parsing errors tested. Tuples, footnotes, `xsi:type` substitution and linkbase/role references are not interpreted; extension `segment`/`scenario` trees are preserved but not dimensionally interpreted. 0 suite cases run. |
| DTS discovery and linkbases | XBRL 2.1 §§3, 5.2.2, 5.2.4; XML Schema 1.0 §4.2.1; OASIS Catalogs 1.1 | Partial | `xbrl-taxonomy` local-only discovery, document-scoped IDs, schema/Label/Presentation/Calculation linkbases, ELRs, catalog mapping, cycle termination. 12 broken + 8 fix schemas; unsupported arcs and unresolved locators are typed errors, not silent passes. Generic links, taxonomy packages and remote references are not supported. |
| Core validation | XBRL 2.1 §§4.1–4.8, §5.2.5.2; XML Schema 1.0 Part 2 §3.2.3 (`decimal`) | Partial | `xbrl-validate` emits `xbrl-validate:*` findings for structural, reference, duplicate-consistency and calculation rule families, with exact decimal arithmetic. Missing DTS, non-empty extension context, mixed accuracy and budget overruns fail closed via `unsupported`/`limit-exceeded`. No `xml:lang`/parent aspect, no unit u-equality, no full s-equal periods. **Calculations 1.1 is not implemented**; 0 suite cases run. |
| Headless rendering | XBRL 2.1 §5.2.2/§5.2.4; XML 1.0 character data; RFC 4180 | Implemented (SDK output formats) | `xbrl-render` walks presentation networks once per concept and emits deterministic text, HTML and CSV. 6 authored goldens cover normal and dimensional-context scenarios. Dimensional contexts yield `[unsupported: dimensional context]`; missing/nil/duplicate/cycle/already-shown cells are explicit display states. This is **not** xBRL-JSON/xBRL-CSV/OIM output and is not a filing renderer. |
| CLI and machine contract | SDK-local [`cli-result.schema.json`](cli-result.schema.json) (`xbrl-sdk/cli-result/v1`) | Implemented | `xbrl parse|validate|render --json`: one-line envelope, stable rule ids, lowercase severities, half-open byte spans, exit codes 0/1/2/4. `crates/xbrl-cli/tests/mvp_integration.rs` checks the full chain over the same fixtures and byte-for-byte determinism. This is an SDK contract, not an XBRL standard. |
| Inline XBRL 1.0 / 1.1 | [iXBRL 1.0](https://www.xbrl.org/specification/inlinexbrl-part1/rec-2010-04-20/inlinexbrl-part1-rec-2010-04-20+corrected-errata-2011-08-17.html), [1.1](https://www.xbrl.org/Specification/inlineXBRL-part1/REC-2013-11-18+errata-2026-07-14/inlineXBRL-part1-REC-2013-11-18+corrected-errata-2026-07-14.html) | Not implemented | `xbrl-inline` is a scaffold with no items; suites [2010](https://www.xbrl.org/2010/inlinexbrl-conformancesuite-2010-04-20.zip) / [2020](https://www.xbrl.org/2020/inlineXBRL-1.1-conformanceSuite-2020-04-08.zip) not run. |
| XBRL Dimensions 1.0 | [Dimensions REC 2012-01-25 + errata](https://www.xbrl.org/specification/dimensions/rec-2012-01-25/dimensions-rec-2006-09-18+corrected-errata-2012-01-25-clean.html) (suite: [XDT-CONF 2025-09-09](https://www.xbrl.org/2025/XBRL-XDT-CONF-2025-09-09.zip)) | Not implemented (detected in contexts) | Segment/scenario content is reported by validation and withheld by rendering; no hypercube, explicit/typed member or dimension-domain processing. 0 suite cases run. |
| Formula 1.0, Table Linkbase 1.0, Versioning, Registries, Taxonomy/Report Packages, Extensible Enumerations, OIM / xBRL-JSON / xBRL-XML / xBRL-CSV, Calculations 1.1 | See [standards-catalog](../skills/xbrl-agent/references/standards-catalog.md) for exact documents and suites | Not implemented | No code path or fixture claims support. Catalog entries are references only, not evidence of implementation. |
| SEC EFM, ESMA ESEF, DQC, certification | Regulator-issued manuals (not XBRL International) | Out of scope | Explicitly excluded from this MVP; no filing or certification verdict is produced. |

## Re-checking this table

Run the commands in [mvp-integration.md](mvp-integration.md) and
[benchmark-baseline.md](benchmark-baseline.md). Any change to a rule id, exit
code or envelope field is a breaking change to the CLI contract; any official
suite run must record the suite URI, case id and expected result before this
matrix can be upgraded.
