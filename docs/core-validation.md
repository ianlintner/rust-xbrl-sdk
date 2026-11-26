# Core validation and diagnostics (PLAN.md card 5)

Status: implemented for the declared subset. No full-conformance claim is made.

## Scope

`xbrl-validate` checks an extracted `xbrl_core::instance::Instance` against
rule families selected by `RuleSet`. Every finding is a
`xbrl_core::diagnostic::Diagnostic` with a stable `xbrl-validate:*` rule id,
`Severity` and (where the source item is known) a `SourceSpan`. A
`ValidationReport` records the rules actually executed; there is no boolean
"valid" verdict. Missing DTS, fractional calculation weights, mixed or
unsupported accuracy, out-of-range arithmetic, non-empty extension
`segment`/`scenario` content and budget overruns emit `xbrl-validate:unsupported`
or `xbrl-validate:limit-exceeded` rather than a clean report.

Out of scope: XBRL Dimensions interpretation, tuples, `xsi:type` substitution,
footnotes, Calculations 1.1, SEC EFM, ESMA ESEF, DQC rules and external
conformance certification. These constructs are absent from the extracted
`Fact` model and are therefore not diagnosed by this crate; no clean-report
claim is made for instances containing them. The parsed `Fact` model also
carries no `xml:lang` or parent-element field, so duplicate detection cannot
compare those aspects. Units are compared by `unitRef` ID (not u-equality),
and period/extension aspects by normalized lexical keys (not full s-equality).

## Normative sources

- XBRL 2.1 Recommendation, 2003-12-31, with errata through 2013-02-20,
  §§4.1–4.8 (duplicate items, contexts, units), §4.6.6 (precision to decimals)
  and §5.2.5.2 (calculation rounding).
  <https://www.xbrl.org/2003/XBRL_2_1_Recommendation_20031231.pdf>
- XML Schema 1.0 Part 2 (Datatypes), 2nd edition, §3.2.3 (`xsd:decimal`) and
  §3.2.4 (`xsd:integer`). <https://www.w3.org/TR/xmlschema-2/>

Arithmetic uses an in-crate `i128` fixed-point decimal with up to 18 fractional
digits; no floating-point comparison is used in the validator. Values outside
that exact representation produce an `unsupported` finding, never silent
clamping. Each fact and the total are rounded to the lowest reported decimals
across the summation relationship (`precision` is converted to `decimals` per
§4.6.6), weights are applied after rounding, and the exact sum is compared to
the rounded total, following §5.2.5.2. A missing contributing item is omitted
from the sum, as the spec requires.

## Rule-to-test record

| Rule | Clause | Positive fixture | Negative fixture |
|---|---|---|---|
| `xbrl-validate:structural` | XBRL 2.1 §4.1–4.4 | `validation.rs::valid_instance_produces_no_findings` | `validation.rs::structural_errors_are_reported` |
| `xbrl-validate:unresolved-reference` | XBRL 2.1 §4.2, §4.4 | `validation.rs::valid_instance_produces_no_findings` | `validation.rs::unresolved_context_and_unit_are_reported_with_spans` |
| `xbrl-validate:duplicate-inconsistent` | XBRL 2.1 §4.6 | `validation.rs::identical_duplicate_facts_are_allowed`, `duplicate_facts_with_equal_value_at_different_scales_are_consistent` | `validation.rs::duplicate_facts_with_different_values_are_reported`, `duplicate_facts_in_s_equal_contexts_are_detected` |
| `xbrl-validate:non-numeric-value` | XSD 1.0 §3.2.3 | `validation.rs::valid_instance_produces_no_findings` | `validation.rs::non_numeric_numeric_fact_is_reported` |
| `xbrl-validate:calculation-inconsistent` | XBRL 2.1 §5.2.5.2 | `validation.rs::calculation_matches_exact_sum`, `calculation_within_decimals_tolerance_is_accepted`, `rounding_model_uses_lowest_decimals_and_exact_comparison`, `precision_counts_significant_digits_not_decimal_places`, `missing_summand_is_omitted_without_an_error`, `zero_weight_and_negative_values_reconcile_exactly`, `calculation_rounds_facts_to_lowest_decimals_before_weighting`, `calculation_rounds_fact_before_applying_weight`, `precision_with_fractional_value_uses_exact_decimal_magnitude` | `validation.rs::calculation_mismatch_is_reported_with_rule_id`, `rounding_model_rejects_discrepancy_inside_old_tolerance` |
| `xbrl-validate:unsupported` | PLAN.md "fail closed" | `validation.rs::valid_instance_produces_no_findings` | `validation.rs::missing_dts_is_explicitly_unsupported_not_green`, `mixed_decimals_and_precision_accuracy_is_unsupported_not_tolerated`, `precision_zero_is_unsupported_not_a_tolerance`, `non_decimal_or_overflowing_calculation_operand_emits_finding`, `calculation_reports_rounding_overflow_instead_of_falling_back_to_unrounded` |
| `xbrl-validate:limit-exceeded` | Resource-limit policy | `validation.rs::valid_instance_produces_no_findings` | `validation.rs::item_limit_never_yields_a_false_green` |

Additional arithmetic and determinism coverage: `decimal.rs` unit tests for
lexical forms, out-of-range rejection, exact ordering across scales at the
`i128` boundary, exact addition, power-of-ten scaling, half-away-from-zero
rounding, weighted sums and §4.6.6 precision conversion; integration tests
`item_limit_never_yields_a_false_green` and
`findings_are_deterministic_across_runs`.

### Fixture provenance and license

All fact/context/unit values in `crates/xbrl-validate/tests/validation.rs` are
hand-authored for this repository; no third-party content is copied. The DTS
used by the calculation tests is the repository's authored
`fixtures/taxonomy/example.xsd` + `example-calculation.xml`, documented in
`fixtures/taxonomy/README.md`. The weight-2 regression writes its own schema
and linkbase into a temporary directory using the correct XBRL 2.1 linkbase
namespace (`http://www.xbrl.org/2003/linkbase`) and asserts that both
calculation arcs load before checking any diagnostic. The official conformance suite
<https://www.xbrl.org/2025/XBRL-CONF-2025-07-16.zip> is referenced but not
vendored because its redistribution license has not been established.

### Counts

Unit tests: 10. Integration tests: 26. Rule-id uniqueness tests are included
in the unit tests. All pass; unsupported rule families are counted as
unsupported, not as pass.

## Verification commands

- `cargo fmt --all -- --check`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
