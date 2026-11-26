//! Behavioral fixtures for the core validation engine.
//!
//! Provenance: all instance values below are hand-authored in this file;
//! taxonomy/calculation relationships come from the repository's authored
//! `fixtures/taxonomy/example.xsd` and `example-calculation.xml` (see
//! `fixtures/taxonomy/README.md`). No third-party conformance corpus is vendored.
//! Normative references:
//! - XBRL 2.1 REC 2003-12-31 with errata 2013-02-20, §§4.1–4.8, §5.2.5
//!   https://www.xbrl.org/2003/XBRL_2_1_Recommendation_20031231.pdf
//! - XSD 1.0 Part 2 2nd ed. §3.2.3 (`xsd:decimal`), §3.2.4 (`xsd:integer`)
//!   https://www.w3.org/TR/xmlschema-2/
//!
//! Official suite for future conformance runs (not vendored here):
//! https://www.xbrl.org/2025/XBRL-CONF-2025-07-16.zip

use std::path::Path;

use xbrl_core::diagnostic::{Severity, SourceSpan};
use xbrl_core::instance::{
    Accuracy, Context, Entity, Fact, Instance, Period, SchemaRef, Unit, UnitMeasures,
};
use xbrl_core::qname::QName;
use xbrl_taxonomy::dts::{Dts, DtsBuilder};
use xbrl_validate::{RuleSet, ValidationReport, Validator};

const NS: &str = "http://example.com/taxonomy";
const ISO4217: &str = "http://www.xbrl.org/2003/iso4217";

fn q(local: &str) -> QName {
    QName::new(None, NS, local).unwrap()
}

fn measure(local: &str) -> QName {
    QName::new(None, ISO4217, local).unwrap()
}

fn span() -> SourceSpan {
    SourceSpan::new(10, 20).unwrap()
}

fn context(id: &str) -> Context {
    Context {
        id: id.into(),
        entity: Entity {
            scheme: "https://example.com/cik".into(),
            identifier: "0000000001".into(),
            segment: None,
        },
        period: Period::Instant("2024-12-31".into()),
        scenario: None,
        span: span(),
    }
}

fn unit(id: &str) -> Unit {
    Unit {
        id: id.into(),
        measures: UnitMeasures::Simple(vec![measure("USD")]),
        span: span(),
    }
}

fn fact(concept: &str, ctx: &str, unit: &str, value: &str, accuracy: Accuracy) -> Fact {
    Fact {
        concept: q(concept),
        context_ref: ctx.into(),
        unit_ref: Some(unit.into()),
        accuracy: Some(accuracy),
        value: Some(value.into()),
        id: None,
        span: span(),
    }
}

fn instance(facts: Vec<Fact>) -> Instance {
    Instance {
        schema_refs: vec![SchemaRef {
            href: "example.xsd".into(),
            base_chain: Vec::new(),
            span: span(),
        }],
        contexts: vec![context("c1")],
        units: vec![unit("u1")],
        facts,
    }
}

fn fixture_dts() -> Option<Dts> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/taxonomy")
        .canonicalize()
        .ok()?;
    DtsBuilder::new(&root).ok()?.discover("example.xsd").ok()
}

fn count(report: &ValidationReport, rule: &str) -> usize {
    report
        .findings()
        .iter()
        .filter(|d| d.rule().as_str() == rule)
        .count()
}

fn validate(inst: &Instance, use_dts: bool) -> ValidationReport {
    let dts = if use_dts { fixture_dts() } else { None };
    Validator::default().validate(inst, dts.as_ref())
}

#[test]
fn valid_instance_produces_no_findings() {
    let inst = instance(vec![fact(
        "Revenue",
        "c1",
        "u1",
        "100.25",
        Accuracy::Infinite,
    )]);
    let report = validate(&inst, true);
    assert!(report.findings().is_empty(), "{:?}", report.findings());
    assert!(report.is_clean());
    assert!(!report.executed().is_empty());
}

#[test]
fn missing_dts_is_explicitly_unsupported_not_green() {
    let inst = instance(vec![fact("Revenue", "c1", "u1", "100", Accuracy::Infinite)]);
    let report = validate(&inst, false);
    assert!(
        count(&report, "xbrl-validate:unsupported") >= 1,
        "{:?}",
        report.findings()
    );
    assert!(!report.is_clean());
    assert!(report.error_count() > 0);
}

#[test]
fn unresolved_context_and_unit_are_reported_with_spans() {
    let mut inst = instance(vec![fact(
        "Assets",
        "missing",
        "alsoMissing",
        "1",
        Accuracy::Infinite,
    )]);
    inst.contexts.clear();
    inst.units.clear();
    let report = validate(&inst, true);
    assert_eq!(
        count(&report, "xbrl-validate:unresolved-reference"),
        2,
        "{:?}",
        report.findings()
    );
    for finding in report.findings() {
        if finding.rule().as_str() == "xbrl-validate:unresolved-reference" {
            assert_eq!(finding.severity(), Severity::Error);
            assert!(finding.span().is_some(), "finding must carry a source span");
        }
    }
}

#[test]
fn duplicate_facts_with_different_values_are_reported() {
    let inst = instance(vec![
        fact("Assets", "c1", "u1", "100", Accuracy::Infinite),
        Fact {
            id: Some("dup".into()),
            value: Some("101".into()),
            ..fact("Assets", "c1", "u1", "100", Accuracy::Infinite)
        },
    ]);
    let report = validate(&inst, true);
    assert_eq!(
        count(&report, "xbrl-validate:duplicate-inconsistent"),
        1,
        "{:?}",
        report.findings()
    );
}

#[test]
fn identical_duplicate_facts_are_allowed() {
    let inst = instance(vec![
        fact("Assets", "c1", "u1", "100", Accuracy::Infinite),
        fact("Assets", "c1", "u1", "100", Accuracy::Infinite),
    ]);
    let report = validate(&inst, true);
    assert_eq!(
        count(&report, "xbrl-validate:duplicate-inconsistent"),
        0,
        "{:?}",
        report.findings()
    );
}

#[test]
fn structural_errors_are_reported() {
    let mut inst = instance(vec![fact("Assets", "c1", "u1", "1", Accuracy::Infinite)]);
    inst.facts[0].context_ref = String::new();
    let report = validate(&inst, true);
    assert!(
        report.has_rule("xbrl-validate:structural"),
        "{:?}",
        report.findings()
    );
    assert!(report.error_count() > 0);
}

#[test]
fn non_numeric_numeric_fact_is_reported() {
    let inst = instance(vec![fact(
        "Assets",
        "c1",
        "u1",
        "not-a-number",
        Accuracy::Infinite,
    )]);
    let report = validate(&inst, true);
    assert_eq!(
        count(&report, "xbrl-validate:non-numeric-value"),
        1,
        "{:?}",
        report.findings()
    );
}

// Authored fixture calculation: Assets = Revenue - Expenses.
fn calc_instance(assets: &str, revenue: &str, expenses: &str) -> Instance {
    instance(vec![
        fact("Assets", "c1", "u1", assets, Accuracy::Infinite),
        fact("Revenue", "c1", "u1", revenue, Accuracy::Infinite),
        fact("Expenses", "c1", "u1", expenses, Accuracy::Infinite),
    ])
}

#[test]
fn calculation_matches_exact_sum() {
    let report = validate(&calc_instance("5.5", "7.25", "1.75"), true);
    assert_eq!(
        count(&report, "xbrl-validate:calculation-inconsistent"),
        0,
        "{:?}",
        report.findings()
    );
}

#[test]
fn calculation_mismatch_is_reported_with_rule_id() {
    let report = validate(&calc_instance("5", "7", "1"), true);
    assert_eq!(
        count(&report, "xbrl-validate:calculation-inconsistent"),
        1,
        "{:?}",
        report.findings()
    );
    let finding = report
        .findings()
        .iter()
        .find(|d| d.rule().as_str() == "xbrl-validate:calculation-inconsistent")
        .unwrap();
    assert_eq!(finding.severity(), Severity::Error);
    assert!(finding.span().is_some());
}

#[test]
fn calculation_within_decimals_tolerance_is_accepted() {
    let inst = instance(vec![
        Fact {
            accuracy: Some(Accuracy::Decimals(0)),
            ..fact("Assets", "c1", "u1", "6.0", Accuracy::Infinite)
        },
        fact("Revenue", "c1", "u1", "7.4", Accuracy::Infinite),
        fact("Expenses", "c1", "u1", "1.4", Accuracy::Infinite),
    ]);
    let report = validate(&inst, true);
    assert_eq!(
        count(&report, "xbrl-validate:calculation-inconsistent"),
        0,
        "{:?}",
        report.findings()
    );
}

#[test]
fn zero_weight_and_negative_values_reconcile_exactly() {
    let report = validate(&calc_instance("-2.50", "0.1", "2.6"), true);
    assert_eq!(
        count(&report, "xbrl-validate:calculation-inconsistent"),
        0,
        "{:?}",
        report.findings()
    );
}

#[test]
fn item_limit_never_yields_a_false_green() {
    let mut inst = instance(Vec::new());
    inst.facts = vec![fact("Assets", "c1", "u1", "1", Accuracy::Infinite); 5];
    let report = Validator::new(
        RuleSet::default(),
        xbrl_validate::ValidationLimits {
            max_items: 3,
            ..Default::default()
        },
    )
    .validate(&inst, fixture_dts().as_ref());
    assert!(report.has_rule("xbrl-validate:limit-exceeded"));
    assert!(!report.is_clean());
}

// --- Regression fixtures for independent-review findings -------------------

fn calc_with_accuracy(
    assets: (&str, Accuracy),
    revenue: (&str, Accuracy),
    expenses: (&str, Accuracy),
) -> Instance {
    instance(vec![
        fact("Assets", "c1", "u1", assets.0, assets.1),
        fact("Revenue", "c1", "u1", revenue.0, revenue.1),
        fact("Expenses", "c1", "u1", expenses.0, expenses.1),
    ])
}

#[test]
fn mixed_decimals_and_precision_accuracy_is_unsupported_not_tolerated() {
    let inst = calc_with_accuracy(
        ("1000000", Accuracy::Decimals(0)),
        ("1", Accuracy::Precision(3)),
        ("0", Accuracy::Precision(3)),
    );
    let report = validate(&inst, true);
    assert!(
        report.has_rule("xbrl-validate:unsupported"),
        "mixed accuracy must never produce a tolerant calculation result: {:?}",
        report.findings()
    );
    assert!(!report.is_clean());
}

#[test]
fn precision_counts_significant_digits_not_decimal_places() {
    // 12000 = 17000 - 5100 with precision 2 on every fact. Per XBRL 2.1
    // §4.6.6 each value rounds to 2 significant digits: 12000, 17000, 5100.
    let inst = calc_with_accuracy(
        ("12000", Accuracy::Precision(2)),
        ("17000", Accuracy::Precision(2)),
        ("5100", Accuracy::Precision(2)),
    );
    let report = validate(&inst, true);
    assert_eq!(
        count(&report, "xbrl-validate:calculation-inconsistent"),
        0,
        "{:?}",
        report.findings()
    );
    assert!(
        !report.has_rule("xbrl-validate:unsupported"),
        "unexpected unsupported: {:?}",
        report.findings()
    );
}

#[test]
fn precision_zero_is_unsupported_not_a_tolerance() {
    let inst = calc_with_accuracy(
        ("5", Accuracy::Precision(0)),
        ("7", Accuracy::Infinite),
        ("1", Accuracy::Infinite),
    );
    let report = validate(&inst, true);
    assert!(
        report.has_rule("xbrl-validate:unsupported"),
        "precision=0 means unknown accuracy: {:?}",
        report.findings()
    );
}

#[test]
fn rounding_model_uses_lowest_decimals_and_exact_comparison() {
    // Lowest decimals across operands is 0: 6.0 -> 6, 7.4 -> 7, 1.4 -> 1.
    // Rounded sum 7 - 1 = 6 equals rounded total 6; the old tolerance model
    // would instead accept arbitrary differences up to 10^0.
    let inst = calc_with_accuracy(
        ("6", Accuracy::Decimals(0)),
        ("7.4", Accuracy::Decimals(0)),
        ("1.4", Accuracy::Decimals(0)),
    );
    let report = validate(&inst, true);
    assert_eq!(
        count(&report, "xbrl-validate:calculation-inconsistent"),
        0,
        "rounded operands reconcile: {:?}",
        report.findings()
    );
}

#[test]
fn rounding_model_rejects_discrepancy_inside_old_tolerance() {
    // With decimals=0 all values round to integers: total 6, sum 7-1=6 is
    // consistent; but 7.6 - 1.0 = 6.6 rounds to 7 - 1 = 6, total 8 rounds to
    // 8 -> mismatch although a 1.0 tolerance would have accepted it.
    let inst = calc_with_accuracy(
        ("8", Accuracy::Decimals(0)),
        ("7.6", Accuracy::Decimals(0)),
        ("1.0", Accuracy::Decimals(0)),
    );
    let report = validate(&inst, true);
    assert_eq!(
        count(&report, "xbrl-validate:calculation-inconsistent"),
        1,
        "{:?}",
        report.findings()
    );
}

#[test]
fn missing_summand_is_omitted_without_an_error() {
    let inst = instance(vec![
        fact("Assets", "c1", "u1", "5", Accuracy::Infinite),
        fact("Revenue", "c1", "u1", "5", Accuracy::Infinite),
    ]);
    let report = validate(&inst, true);
    assert!(
        !report.has_rule("xbrl-validate:unsupported"),
        "missing contributions are omitted by XBRL 2.1 §5.2.5.2: {:?}",
        report.findings()
    );
    assert_eq!(
        count(&report, "xbrl-validate:calculation-inconsistent"),
        0,
        "{:?}",
        report.findings()
    );
}

#[test]
fn non_decimal_or_overflowing_calculation_operand_emits_finding() {
    let inst = calc_with_accuracy(
        ("5", Accuracy::Infinite),
        ("not-a-number", Accuracy::Infinite),
        ("1", Accuracy::Infinite),
    );
    let report = validate(&inst, true);
    assert!(
        report.has_rule("xbrl-validate:non-numeric-value")
            || report.has_rule("xbrl-validate:unsupported"),
        "a non-decimal operand must never be silently skipped: {:?}",
        report.findings()
    );
}

#[test]
fn duplicate_facts_with_equal_value_at_different_scales_are_consistent() {
    let inst = instance(vec![
        Fact {
            accuracy: Some(Accuracy::Decimals(1)),
            ..fact("Assets", "c1", "u1", "1.0", Accuracy::Decimals(1))
        },
        Fact {
            accuracy: Some(Accuracy::Decimals(0)),
            ..fact("Assets", "c1", "u1", "1", Accuracy::Decimals(0))
        },
    ]);
    let report = validate(&inst, true);
    assert_eq!(
        count(&report, "xbrl-validate:duplicate-inconsistent"),
        0,
        "1.0 and 1 are the same value: {:?}",
        report.findings()
    );
}

#[test]
fn duplicate_facts_in_s_equal_contexts_are_detected() {
    let mut inst = instance(vec![
        fact("Assets", "c1", "u1", "100", Accuracy::Infinite),
        fact("Assets", "c2", "u1", "101", Accuracy::Infinite),
    ]);
    inst.contexts.push(context("c2"));
    let report = validate(&inst, true);
    assert_eq!(
        count(&report, "xbrl-validate:duplicate-inconsistent"),
        1,
        "context ID is not an aspect; s-equal contexts are: {:?}",
        report.findings()
    );
}

#[test]
fn findings_are_deterministic_across_runs() {
    let mut inst = instance(vec![
        fact("Assets", "c1", "u1", "1", Accuracy::Infinite),
        fact("Revenue", "c1", "u1", "7", Accuracy::Infinite),
        fact("Expenses", "c1", "u1", "1", Accuracy::Infinite),
    ]);
    inst.contexts.push(context("c2"));
    let dts = fixture_dts();
    let reports: Vec<_> = (0..8)
        .map(|_| Validator::default().validate(&inst, dts.as_ref()))
        .collect();
    let first = &reports[0];
    for report in &reports[1..] {
        assert_eq!(first.findings(), report.findings());
    }
}
