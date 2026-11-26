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
