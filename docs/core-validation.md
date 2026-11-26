# Core validation and diagnostics (PLAN.md card 5)

Status: implemented for the declared subset. No full-conformance claim is made.

## Scope

`xbrl-validate` checks an extracted `xbrl_core::instance::Instance` against
rule families selected by `RuleSet`. Every finding is a
`xbrl_core::diagnostic::Diagnostic` with a stable `xbrl-validate:*` rule id,
`Severity` and (where the source item is known) a `SourceSpan`. A
`ValidationReport` records the rules actually executed; there is no boolean
"valid" verdict. Missing DTS, fractional calculation weights, out-of-subset
constructs and budget overruns emit `xbrl-validate:unsupported` or
`xbrl-validate:limit-exceeded` rather than a clean report.

Out of scope: XBRL Dimensions, tuples, footnotes, Calculations 1.1, SEC EFM,
ESMA ESEF, DQC rules and external conformance certification.

## Normative sources

- XBRL 2.1 Recommendation, 2003-12-31, with errata through 2013-02-20,
  §§4.1–4.8 (duplicate items, contexts, units) and §5.2.5 (calculation).
  <https://www.xbrl.org/2003/XBRL_2_1_Recommendation_20031231.pdf>
- XML Schema 1.0 Part 2 (Datatypes), 2nd edition, §3.2.3 (`xsd:decimal`) and
  §3.2.4 (`xsd:integer`). <https://www.w3.org/TR/xmlschema-2/>

Arithmetic uses an in-crate `i128` fixed-point decimal with up to 18 fractional
digits; no floating-point comparison is used. Values outside that exact
representation produce an `unsupported` finding, never rounding.

## Rule-to-test record

| Rule | Clause | Positive fixture | Negative fixture | Observed |
|---|---|---|---|---|
| `xbrl-validate:structural` | XBRL 2.1 §4.1–4.4 | `validation.rs::valid_instance_produces_no_findings` | `validation.rs::structural_errors_are_reported` | pass |
| `xbrl-validate:unresolved-reference` | XBRL 2.1 §4.2, §4.4 | `validation.rs::valid_instance_produces_no_findings` | `validation.rs::unresolved_context_and_unit_are_reported_with_spans` | pass |
| `xbrl-validate:duplicate-inconsistent` | XBRL 2.1 §4.6 | `validation.rs::identical_duplicate_facts_are_allowed` | `validation.rs::duplicate_facts_with_different_values_are_reported` | pass |
| `xbrl-validate:non-numeric-value` | XSD 1.0 §3.2.3 | `validation.rs::valid_instance_produces_no_findings` | `validation.rs::non_numeric_numeric_fact_is_reported` | pass |
| `xbrl-validate:calculation-inconsistent` | XBRL 2.1 §5.2.5 | `validation.rs::calculation_matches_exact_sum`, `calculation_within_decimals_tolerance_is_accepted` | `validation.rs::calculation_mismatch_is_reported_with_rule_id` | pass |
| `xbrl-validate:unsupported` | PLAN.md "fail closed" | `validation.rs::valid_instance_produces_no_findings` | `validation.rs::missing_dts_is_explicitly_unsupported_not_green` | pass |
| `xbrl-validate:limit-exceeded` | Resource-limit policy | `validation.rs::valid_instance_produces_no_findings` | `validation.rs::item_limit_never_yields_a_false_green` | pass |

Arithmetic corner cases live in `decimal.rs` tests (lexical forms, out-of-range
rejection, exact addition across scales, power-of-ten scaling, weighted sums).

### Fixture provenance and license

All fact/context/unit values in `crates/xbrl-validate/tests/validation.rs` are
hand-authored for this repository; no third-party content is copied. The DTS
used by the calculation tests is the repository's authored
`fixtures/taxonomy/example.xsd` + `example-calculation.xml`, documented in
`fixtures/taxonomy/README.md`. The official conformance suite
<https://www.xbrl.org/2025/XBRL-CONF-2025-07-16.zip> is referenced but not
vendored because its redistribution license has not been established.

### Counts

Positive cases: 5. Negative cases: 6. Unsupported cases: 2. Arithmetic unit
cases: 6. Rule-id uniqueness cases: 1. All pass; unsupported rule families are
counted as unsupported, not as pass.

## Verification commands

- `cargo fmt --all -- --check`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
