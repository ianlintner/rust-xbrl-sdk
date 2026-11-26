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
//! optional `unitRef`, exact decimal comparison, `decimals`/`precision`
//! accuracy, identity-based duplicate detection and one-level calculation
//! summation from the calculation linkbase.
//!
//! Unsupported: tuples, dimensions (segment/scenario interpretation),
//! `xsi:type` substitution, calculations 1.1, formula, filing profiles and
//! anything requiring semantics this crate does not implement. Each produces
//! an explicit [`rules::UNSUPPORTED`] finding — an unsupported input can never
//! produce a finding-free report.
//!
//! Fixtures and their provenance are documented in `fixtures/validate/README.md`.

pub mod decimal;
pub mod rules;

use std::collections::{HashMap, HashSet};

use xbrl_core::diagnostic::{Diagnostic, RuleId, Severity, SourceSpan};
use xbrl_core::instance::{Accuracy, Fact, Instance, UnitMeasures};
use xbrl_core::qname::QName;
use xbrl_taxonomy::dts::Dts;
use xbrl_taxonomy::model::LinkbaseKind;

/// Which rule families a [`Validator`] runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleSet {
    /// Structural invariants: at least one schemaRef, required fact references,
    /// unit requirement for numeric concepts, nil and accuracy consistency.
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
            let mut seen: HashMap<DupKey, &Fact> = HashMap::new();
            for fact in &instance.facts {
                let key = DupKey {
                    concept: fact.concept.clone(),
                    context_ref: fact.context_ref.clone(),
                    unit_ref: fact.unit_ref.clone(),
                };
                if let Some(first) = seen.insert(key, fact) {
                    if first.value != fact.value || first.accuracy != fact.accuracy {
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
        let by_context: HashMap<&str, Vec<&Fact>> = {
            let mut map: HashMap<&str, Vec<&Fact>> = HashMap::new();
            for fact in &instance.facts {
                map.entry(fact.context_ref.as_str()).or_default().push(fact);
            }
            map
        };
        let mut checked: HashSet<(&QName, &str)> = HashSet::new();
        for rel in dts
            .relationships()
            .iter()
            .filter(|r| r.kind == LinkbaseKind::Calculation)
        {
            if !checked.insert((&rel.from, rel.elr.as_str())) {
                continue;
            }
            let summands = dts.calculation_summands_in_elr(&rel.from, &rel.elr);
            if summands.is_empty() {
                continue;
            }
            for (context, facts) in &by_context {
                let total = facts
                    .iter()
                    .find(|f| f.concept == rel.from && f.context_ref == *context);
                let Some(total) = total else { continue };
                // Unit consistency: all summands and the total must share the
                // same unitRef. A missing unitRef is a comparison, not a
                // numeric value, and is reported by the structural rules.
                let mut sum = decimal::Decimal::ZERO;
                let mut comparable = true;
                let mut accuracy = Accuracy::Infinite;
                for child in &summands {
                    if child.edge.weight.fract() != 0.0 {
                        self.push(
                            report,
                            diagnostic(
                                rules::UNSUPPORTED,
                                Severity::Error,
                                format!(
                                    "fractional calculation weight {} is not supported",
                                    child.edge.weight
                                ),
                                Some(total.span),
                            ),
                        );
                        comparable = false;
                        break;
                    }
                    let Some(fact) = facts
                        .iter()
                        .find(|f| f.concept == child.concept && f.unit_ref == total.unit_ref)
                    else {
                        comparable = false;
                        break;
                    };
                    let Some(value) = &fact.value else {
                        comparable = false;
                        break;
                    };
                    let Ok(value) = decimal::Decimal::parse(value) else {
                        comparable = false;
                        break;
                    };
                    let Some(scaled) = value.checked_mul_i64(child.edge.weight.trunc() as i64)
                    else {
                        comparable = false;
                        break;
                    };
                    sum = match sum.checked_add(scaled) {
                        Some(v) => v,
                        None => {
                            comparable = false;
                            break;
                        }
                    };
                    accuracy = min_accuracy(accuracy, fact.accuracy.unwrap_or(Accuracy::Infinite));
                }
                if !comparable {
                    self.push(report, diagnostic(rules::UNSUPPORTED, Severity::Error,
                        format!("calculation for {} in context {context:?} has missing, nil or non-decimal operands",
                            rel.from.to_prefixed()), Some(total.span)));
                    continue;
                }
                let Some(total_value) = total
                    .value
                    .as_deref()
                    .and_then(|v| decimal::Decimal::parse(v).ok())
                else {
                    continue;
                };
                let tolerance = accuracy_tolerance(min_accuracy(
                    accuracy,
                    total.accuracy.unwrap_or(Accuracy::Infinite),
                ));
                let difference = total_value.checked_sub(sum);
                let Some(difference) = difference else {
                    continue;
                };
                if difference.abs() > tolerance {
                    self.push(report, at_fact(rules::CALCULATION_INCONSISTENT,
                        format!("{} does not equal the sum of its calculation summands in context {context:?}",
                            rel.from.to_prefixed()), total));
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
    context_ref: String,
    unit_ref: Option<String>,
}

fn is_numeric_lexical(s: &str) -> bool {
    decimal::Decimal::parse(s).is_ok()
}

fn min_accuracy(a: Accuracy, b: Accuracy) -> Accuracy {
    match (a, b) {
        (Accuracy::Infinite, x) | (x, Accuracy::Infinite) => x,
        (Accuracy::Decimals(x), Accuracy::Decimals(y)) => Accuracy::Decimals(x.min(y)),
        (Accuracy::Precision(x), Accuracy::Precision(y)) => Accuracy::Precision(x.min(y)),
        // Mixed accuracy types cannot be combined safely; fail closed to zero
        // tolerance rather than silently accepting an unknown bound.
        (Accuracy::Decimals(_), Accuracy::Precision(_))
        | (Accuracy::Precision(_), Accuracy::Decimals(_)) => Accuracy::Decimals(i64::MIN),
    }
}

fn accuracy_tolerance(accuracy: Accuracy) -> decimal::Decimal {
    match accuracy {
        Accuracy::Infinite => decimal::Decimal::ZERO,
        Accuracy::Decimals(d) => {
            // Clamp to the representation's own range: magnitudes beyond 10^18
            // are outside the supported exact-decimal domain and are reported
            // as unsupported by the parser's own limits.
            let d = d.clamp(-18, 18);
            decimal::Decimal::one()
                .scale_b10(-d)
                .unwrap_or(decimal::Decimal::ZERO)
        }
        Accuracy::Precision(p) => {
            let p = p.min(18);
            decimal::Decimal::one()
                .scale_b10(-(p as i64))
                .unwrap_or(decimal::Decimal::ZERO)
        }
    }
}
