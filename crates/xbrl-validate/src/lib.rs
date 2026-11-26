//! Core XBRL validation with rule-coded, source-provenant diagnostics.
//!
//! This crate validates an already-parsed [`Instance`] against structural,
//! reference, duplicate-consistency and calculation rule families. It never
//! returns a bare "valid" verdict: [`ValidationReport`] records which rules ran
//! and every finding with its severity and source span.
//!
//! # Specification versions
//!
//! * XBRL 2.1 Recommendation 2003-12-31 with errata through 2013-02-20,
//!   §§4.1–4.8 (instance facts, contexts, units, references, duplicates) and
//!   §5.2.5 (calculation relationships):
//!   <https://www.xbrl.org/specification/xbrl-2.1/>
//! * XML Schema 1.0 Part 2 (Datatypes), 2nd ed. 2004-10-28, §3.2.3 `decimal`
//!   and §3.2.4 `integer`: lexical form and exact value space.
//! * Calculation consistency uses the XBRL 2.1 §5.2.5.2 decimals-based rounding
//!   model for the supported subset. Calculations 1.1 is **not** implemented
//!   and never claimed.
//!
//! # Scope and fail-closed policy
//!
//! Supported: direct numeric and nonnumeric item facts, one `contextRef` and
//! optional `unitRef`, exact decimal comparison, `decimals`/`precision` accuracy
//! converted per XBRL 2.1 §4.6.6, identity-based duplicate detection over
//! s-equal context aspects, and one-level calculation summation from the
//! calculation linkbase.
//!
//! Explicit limitations: no `xml:lang` or parent-element field exists in the
//! parsed `Fact` model, so duplicate detection cannot compare those aspects
//! and does not claim to. Numeric `unitRef` presence is checked as a
//! reference, not as a taxonomy-derived concept-type requirement. Extension
//! `segment`/`scenario` content is compared structurally for s-equality but
//! is not interpreted as XBRL Dimensions.
//!
//! Unsupported: tuples, dimensions interpretation, `xsi:type` substitution,
//! calculations 1.1, formula, filing profiles and anything requiring
//! semantics this crate does not implement. Each produces an explicit
//! [`rules::UNSUPPORTED`] finding — an unsupported input can never produce a
//! finding-free report.
//!
//! Fixtures and their provenance are documented in `docs/core-validation.md`
//! and `crates/xbrl-validate/tests/validation.rs`.

pub mod decimal;
pub mod rules;

use std::collections::{HashMap, HashSet};

use xbrl_core::diagnostic::{Diagnostic, RuleId, Severity, SourceSpan};
use xbrl_core::instance::{Accuracy, Context, Fact, Instance, UnitMeasures};
use xbrl_core::qname::QName;
use xbrl_taxonomy::dts::Dts;
use xbrl_taxonomy::model::LinkbaseKind;

/// Which rule families a [`Validator`] runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleSet {
    /// Structural invariants: at least one schemaRef, required fact references,
    /// nil and accuracy consistency. Unit reference resolution is checked by
    /// the reference rule; taxonomy-derived type/unit compatibility is not.
    pub structural: bool,
    /// `contextRef`/`unitRef` must resolve to an ID in the instance.
    pub references: bool,
    /// Duplicate facts must agree on value, accuracy, context, unit and nil.
    pub duplicate_consistency: bool,
    /// Summation relationships must reconcile within reported accuracy.
    pub calculation: bool,
}

impl Default for RuleSet {
    fn default() -> Self {
        Self {
            structural: true,
            references: true,
            duplicate_consistency: true,
            calculation: true,
        }
    }
}

/// Resource budgets for one validation run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidationLimits {
    /// Maximum facts, contexts, units and schemaRefs combined. Default: 100_000.
    pub max_items: usize,
    /// Maximum findings retained in a report. Default: 10_000. The limit is
    /// itself reported as a finding, so the report never looks clean.
    pub max_findings: usize,
}

impl Default for ValidationLimits {
    fn default() -> Self {
        Self {
            max_items: 100_000,
            max_findings: 10_000,
        }
    }
}

/// Outcome of one validation run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReport {
    findings: Vec<Diagnostic>,
    executed: Vec<&'static str>,
    findings_truncated: bool,
}

impl ValidationReport {
    fn new(executed: Vec<&'static str>) -> Self {
        Self {
            findings: Vec::new(),
            executed,
            findings_truncated: false,
        }
    }

    /// All findings in deterministic rule-family then source order.
    #[must_use]
    pub fn findings(&self) -> &[Diagnostic] {
        &self.findings
    }

    /// Rule families/rules this run actually executed.
    #[must_use]
    pub fn executed(&self) -> &[&'static str] {
        &self.executed
    }

    /// Whether the finding limit was reached (findings were dropped).
    #[must_use]
    pub fn findings_truncated(&self) -> bool {
        self.findings_truncated
    }

    /// Whether the run emitted no findings. Always false when a rule family
    /// was requested but could not run, because that emits `UNSUPPORTED`.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty() && !self.findings_truncated
    }

    /// Number of findings at or above `Severity::Error`.
    #[must_use]
    pub fn error_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|d| d.severity() == Severity::Error)
            .count()
    }

    /// Whether any finding has `rule` as its rule id.
    #[must_use]
    pub fn has_rule(&self, rule: &str) -> bool {
        self.findings.iter().any(|d| d.rule().as_str() == rule)
    }
}

/// Validates instances against the rule families in [`RuleSet`].
#[derive(Debug, Clone, Copy)]
pub struct Validator {
    rules: RuleSet,
    limits: ValidationLimits,
}

impl Default for Validator {
    fn default() -> Self {
        Self::new(RuleSet::default(), ValidationLimits::default())
    }
}

impl Validator {
    /// Creates a validator with explicit rule selection and budgets.
    #[must_use]
    pub fn new(rules: RuleSet, limits: ValidationLimits) -> Self {
        Self { rules, limits }
    }

    /// Runs all requested checks over `instance`.
    ///
    /// `dts` is required for concept-aware checks (numeric type and
    /// calculation relationships). Passing `None` is legal but emits
    /// [`rules::UNSUPPORTED`] findings for those checks, never a clean report.
    #[must_use]
    pub fn validate(&self, instance: &Instance, dts: Option<&Dts>) -> ValidationReport {
        let mut executed = Vec::new();
        if self.rules.structural {
            executed.push("structural");
        }
        if self.rules.references {
            executed.push("references");
        }
        if self.rules.duplicate_consistency {
            executed.push("duplicate-consistency");
        }
        if self.rules.calculation {
            executed.push("calculation");
        }
        let mut report = ValidationReport::new(executed);

        let item_count = instance.schema_refs.len()
            + instance.contexts.len()
            + instance.units.len()
            + instance.facts.len();
        if item_count > self.limits.max_items {
            self.push(
                &mut report,
                diagnostic(
                    rules::LIMIT_EXCEEDED,
                    Severity::Error,
                    format!(
                        "validation item count {item_count} exceeds limit {}",
                        self.limits.max_items
                    ),
                    None,
                ),
            );
            return report;
        }

        // Structural: concepts with a unitRef must also have accuracy when a
        // numeric value is present; every fact needs a contextRef (already
        // enforced by the parser, rechecked here for hand-built instances).
        if self.rules.structural {
            if instance.schema_refs.is_empty() {
                self.push(
                    &mut report,
                    diagnostic(
                        rules::STRUCTURAL,
                        Severity::Error,
                        "instance has no schemaRef",
                        None,
                    ),
                );
            }
            for fact in &instance.facts {
                if fact.context_ref.is_empty() {
                    self.push(
                        &mut report,
                        at_fact(rules::STRUCTURAL, "fact has no contextRef", fact),
                    );
                }
                if let Some(value) = &fact.value {
                    let trimmed = value.trim();
                    if !trimmed.is_empty()
                        && !is_numeric_lexical(trimmed)
                        && fact.accuracy.is_some()
                    {
                        self.push(
                            &mut report,
                            at_fact(
                                rules::NON_NUMERIC_VALUE,
                                format!("numeric fact value {trimmed:?} is not an XSD 1.0 decimal"),
                                fact,
                            ),
                        );
                    }
                }
            }
        }

        if self.rules.references {
            let context_ids: HashSet<&str> =
                instance.contexts.iter().map(|c| c.id.as_str()).collect();
            let unit_ids: HashSet<&str> = instance.units.iter().map(|u| u.id.as_str()).collect();
            for fact in &instance.facts {
                if !context_ids.contains(fact.context_ref.as_str()) {
                    self.push(
                        &mut report,
                        at_fact(
                            rules::UNRESOLVED_REFERENCE,
                            format!(
                                "contextRef {:?} has no context definition",
                                fact.context_ref
                            ),
                            fact,
                        ),
                    );
                }
                if let Some(unit) = &fact.unit_ref {
                    if !unit_ids.contains(unit.as_str()) {
                        self.push(
                            &mut report,
                            at_fact(
                                rules::UNRESOLVED_REFERENCE,
                                format!("unitRef {unit:?} has no unit definition"),
                                fact,
                            ),
                        );
                    }
                }
            }
        }

        if self.rules.duplicate_consistency {
            let contexts: HashMap<&str, &Context> = instance
                .contexts
                .iter()
                .map(|c| (c.id.as_str(), c))
                .collect();
            let mut seen: HashMap<DupKey, &Fact> = HashMap::new();
            for fact in &instance.facts {
                // An unresolved contextRef is reported by the reference rule;
                // without s-equal aspects there is nothing to compare here.
                let Some(context) = contexts.get(fact.context_ref.as_str()) else {
                    continue;
                };
                let key = DupKey {
                    concept: fact.concept.clone(),
                    context: context_aspect_key(context),
                    unit_ref: fact.unit_ref.clone(),
                };
                if let Some(first) = seen.insert(key, fact) {
                    let first_value = normalized_value(first);
                    let this_value = normalized_value(fact);
                    let equal = first_value.is_some_and(|a| {
                        this_value.is_some_and(|b| a.cmp(&b) == std::cmp::Ordering::Equal)
                    });
                    if first.value != fact.value && !equal {
                        self.push(
                            &mut report,
                            at_fact(
                                rules::DUPLICATE_INCONSISTENT,
                                format!(
                                    "duplicate fact for {} disagrees with earlier fact",
                                    fact.concept.to_prefixed()
                                ),
                                fact,
                            ),
                        );
                    }
                }
            }
        }

        if self.rules.calculation {
            match dts {
                None => self.push(
                    &mut report,
                    diagnostic(
                        rules::UNSUPPORTED,
                        Severity::Error,
                        "calculation checks require a DTS; none supplied",
                        None,
                    ),
                ),
                Some(dts) => self.check_calculations(instance, dts, &mut report),
            }
        }

        if !self.supported_subset(instance) {
            self.push(
                &mut report,
                diagnostic(
                    rules::UNSUPPORTED,
                    Severity::Error,
                    "instance contains a construct outside the supported validation subset",
                    None,
                ),
            );
        }
        report
    }

    fn check_calculations(&self, instance: &Instance, dts: &Dts, report: &mut ValidationReport) {
        // XBRL 2.1 §5.2.5.2: round every contributing fact and the total to
        // the lowest reported decimals, then compare the sums exactly.
        // Mixed/unsupported accuracy and unrepresentable arithmetic produce
        // an explicit unsupported finding — never a green result.
        let by_context: HashMap<&str, Vec<&Fact>> = {
            let mut map: HashMap<&str, Vec<&Fact>> = HashMap::new();
            for fact in &instance.facts {
                map.entry(fact.context_ref.as_str()).or_default().push(fact);
            }
            map
        };
        let mut checked: HashSet<(&QName, &str)> = HashSet::new();
        let mut relationships: Vec<_> = dts
            .relationships()
            .iter()
            .filter(|r| r.kind == LinkbaseKind::Calculation)
            .collect();
        relationships.sort_by(|a, b| {
            a.from
                .to_prefixed()
                .cmp(&b.from.to_prefixed())
                .then_with(|| a.elr.cmp(&b.elr))
        });
        for rel in relationships {
            if !checked.insert((&rel.from, rel.elr.as_str())) {
                continue;
            }
            let summands = dts.calculation_summands_in_elr(&rel.from, &rel.elr);
            if summands.is_empty() {
                continue;
            }
            let mut context_ids: Vec<&str> = by_context.keys().copied().collect();
            context_ids.sort_unstable();
            for context in context_ids {
                let facts = &by_context[context];
                let Some(total) = facts
                    .iter()
                    .find(|f| f.concept == rel.from && f.context_ref == context)
                else {
                    continue;
                };
                let unsupported = |message: String| {
                    diagnostic(
                        rules::UNSUPPORTED,
                        Severity::Error,
                        message,
                        Some(total.span),
                    )
                };

                // Resolve every operand's decimals before summing: the lowest
                // decimals controls the rounding of all operands.
                let Some(total_value) = total
                    .value
                    .as_deref()
                    .and_then(|v| decimal::Decimal::parse(v).ok())
                else {
                    self.push(report, unsupported(format!("calculation total for {} in context {context:?} is not an exact decimal", rel.from.to_prefixed())));
                    continue;
                };
                let mut operands: Vec<(decimal::Decimal, Option<i64>)> = Vec::new();
                // Mixed accuracy kinds are reported as unsupported even
                // though precision can be converted to decimals: the model
                // cannot prove the reported combination is comparable.
                let total_kind = match total.accuracy.unwrap_or(Accuracy::Infinite) {
                    Accuracy::Decimals(_) => 1,
                    Accuracy::Precision(_) => 2,
                    Accuracy::Infinite => 0,
                };
                let mut mixed = false;
                if effective_decimals(total.accuracy.unwrap_or(Accuracy::Infinite), &total_value)
                    .is_none()
                {
                    self.push(report, unsupported(format!("calculation total for {} in context {context:?} has unsupported accuracy", rel.from.to_prefixed())));
                    continue;
                }
                let mut failed = false;
                for child in &summands {
                    let w = child.edge.weight;
                    if w.fract() != 0.0 || w.abs() > i64::MAX as f64 {
                        self.push(
                            report,
                            unsupported(format!(
                                "fractional or out-of-range calculation weight {w} is not supported"
                            )),
                        );
                        failed = true;
                        break;
                    }
                    // XBRL 2.1 §5.2.5.2: a missing contribution is simply
                    // omitted from the sum; it is not an error.
                    let Some(fact) = facts
                        .iter()
                        .find(|f| f.concept == child.concept && f.unit_ref == total.unit_ref)
                    else {
                        continue;
                    };
                    let Some(value) = fact
                        .value
                        .as_deref()
                        .and_then(|v| decimal::Decimal::parse(v).ok())
                    else {
                        self.push(report, unsupported(format!("calculation operand {} in context {context:?} is not an exact decimal", child.concept.to_prefixed())));
                        failed = true;
                        break;
                    };
                    let Some(d) =
                        effective_decimals(fact.accuracy.unwrap_or(Accuracy::Infinite), &value)
                    else {
                        self.push(report, unsupported(format!("calculation operand {} in context {context:?} has unsupported accuracy", child.concept.to_prefixed())));
                        failed = true;
                        break;
                    };
                    let kind = match fact.accuracy.unwrap_or(Accuracy::Infinite) {
                        Accuracy::Decimals(_) => 1,
                        Accuracy::Precision(_) => 2,
                        Accuracy::Infinite => 0,
                    };
                    if kind != total_kind && kind != 0 && total_kind != 0 {
                        mixed = true;
                    }
                    let weight = w as i64;
                    let Some(product) = value.checked_mul_i64(weight) else {
                        self.push(report, unsupported(format!("calculation operand {} in context {context:?} overflows exact arithmetic", child.concept.to_prefixed())));
                        failed = true;
                        break;
                    };
                    operands.push((product, d));
                }
                if failed {
                    continue;
                }
                if mixed {
                    self.push(report, unsupported(format!("calculation for {} in context {context:?} mixes decimals and precision accuracy; unsupported", rel.from.to_prefixed())));
                    continue;
                }
                let total_decimals =
                    effective_decimals(total.accuracy.unwrap_or(Accuracy::Infinite), &total_value)
                        .flatten();
                let rounded_total = round_to(total_value, total_decimals).unwrap_or(total_value);
                let mut sum = decimal::Decimal::ZERO;
                for (value, d) in &operands {
                    // Per XBRL 2.1 §4.6.6 each fact is rounded to its own
                    // reported accuracy before the exact sum comparison.
                    let Some(rounded) = round_to(*value, *d) else {
                        self.push(report, unsupported(format!("calculation operand for {} in context {context:?} cannot be rounded", rel.from.to_prefixed())));
                        failed = true;
                        break;
                    };
                    match sum.checked_add(rounded) {
                        Some(next) => sum = next,
                        None => {
                            self.push(report, unsupported(format!("calculation sum for {} in context {context:?} overflows exact arithmetic", rel.from.to_prefixed())));
                            failed = true;
                            break;
                        }
                    }
                }
                if failed {
                    continue;
                }
                if rounded_total.cmp(&round_to(sum, total_decimals).unwrap_or(sum))
                    != std::cmp::Ordering::Equal
                {
                    self.push(
                        report,
                        at_fact(
                            rules::CALCULATION_INCONSISTENT,
                            format!("{} does not equal the rounded sum of its calculation summands in context {context:?}", rel.from.to_prefixed()),
                            total,
                        ),
                    );
                }
            }
        }
    }

    fn supported_subset(&self, instance: &Instance) -> bool {
        instance.contexts.iter().all(|c| {
            c.scenario.as_ref().is_none_or(|nodes| nodes.is_empty())
                && c.entity
                    .segment
                    .as_ref()
                    .is_none_or(|nodes| nodes.is_empty())
        }) && instance.units.iter().all(|u| match &u.measures {
            UnitMeasures::Simple(_) => true,
            UnitMeasures::Divide { .. } => true,
        })
    }

    fn push(&self, report: &mut ValidationReport, finding: Diagnostic) {
        if report.findings.len() >= self.limits.max_findings {
            if !report.findings_truncated {
                report.findings_truncated = true;
                report.findings.push(diagnostic(
                    rules::LIMIT_EXCEEDED,
                    Severity::Error,
                    format!(
                        "finding limit {} reached; additional findings suppressed",
                        self.limits.max_findings
                    ),
                    None,
                ));
            }
            return;
        }
        report.findings.push(finding);
    }
}

fn diagnostic(
    rule: &str,
    severity: Severity,
    message: impl Into<String>,
    span: Option<SourceSpan>,
) -> Diagnostic {
    Diagnostic::new(
        RuleId::new(rule).expect("static rule id"),
        severity,
        message,
        span,
    )
}

fn at_fact(rule: &str, message: impl Into<String>, fact: &Fact) -> Diagnostic {
    diagnostic(rule, Severity::Error, message, Some(fact.span))
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DupKey {
    concept: QName,
    /// s-equal context aspects; the XML ID itself is not an aspect.
    /// `xml:lang` is not represented in the parsed model and is therefore
    /// outside the supported duplicate check (documented as a limitation).
    context: ContextAspects,
    unit_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ContextAspects {
    scheme: String,
    identifier: String,
    period: String,
    scenario: Option<String>,
    segment: Option<String>,
}

fn context_aspect_key(context: &Context) -> ContextAspects {
    fn nodes(nodes: &Option<Vec<xbrl_core::instance::ExtensionNode>>) -> Option<String> {
        nodes.as_ref().map(|nodes| format!("{nodes:?}"))
    }
    ContextAspects {
        scheme: context.entity.scheme.clone(),
        identifier: context.entity.identifier.clone(),
        period: format!("{:?}", context.period),
        scenario: nodes(&context.scenario),
        segment: nodes(&context.entity.segment),
    }
}

/// Normalized value of a fact at its own accuracy. Returns `None` when the
/// lexical form is not an exact decimal within the supported `i128` domain.
fn normalized_value(fact: &Fact) -> Option<decimal::Decimal> {
    let value = decimal::Decimal::parse(fact.value.as_deref()?).ok()?;
    let accuracy = effective_decimals(fact.accuracy.unwrap_or(Accuracy::Infinite), &value)?;
    round_to(value, accuracy)
}

/// Resolves a fact's accuracy to a number of decimal places, following
/// XBRL 2.1 §4.6.6 for `precision`. `None` means the construct is outside the
/// supported exact-decimal domain and must be reported, never tolerated.
fn effective_decimals(accuracy: Accuracy, value: &decimal::Decimal) -> Option<Option<i64>> {
    match accuracy {
        Accuracy::Infinite => Some(None),
        Accuracy::Decimals(d) if (-18..=18).contains(&d) => Some(Some(d)),
        Accuracy::Decimals(_) => None,
        Accuracy::Precision(0) => None,
        Accuracy::Precision(p) => decimal::decimals_for_precision(p, *value)
            .filter(|d| (-18..=18).contains(d))
            .map(Some),
    }
}

/// Rounds `value` to `decimals` places, half away from zero. `None` means
/// `decimals` is outside the exact fixed-point domain.
fn round_to(value: decimal::Decimal, decimals: Option<i64>) -> Option<decimal::Decimal> {
    match decimals {
        None => Some(value),
        Some(d) if d < 0 => {
            let places = u8::try_from(-d).ok()?;
            value.round_to_negative_decimals(places)
        }
        Some(d) => {
            let scale = u8::try_from(d).ok()?;
            value.round_to_decimals(scale)
        }
    }
}

fn is_numeric_lexical(s: &str) -> bool {
    decimal::Decimal::parse(s).is_ok()
}
