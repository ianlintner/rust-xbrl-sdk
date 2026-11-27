//! Display helpers for names, periods, units and accuracy.
//!
//! Periods: XBRL 2.1 §4.7.2 (instant, startDate/endDate, forever). Units: §4.8.
//! Accuracy: §4.6.4–4.6.6. Values are shown lexically; nothing is rounded.

use xbrl_core::instance::{Accuracy, Period, Unit, UnitMeasures};
use xbrl_core::qname::QName;

/// `prefix:local` when the name carries a prefix, otherwise Clark notation, so a
/// bare local name is never shown ambiguously.
#[must_use]
pub fn qname_display(name: &QName) -> String {
    if name.prefix().is_some() {
        name.to_prefixed()
    } else {
        name.to_clark()
    }
}

/// Instant as its lexical date, duration as `start to end`, `forever`.
#[must_use]
pub fn format_period(period: &Period) -> String {
    match period {
        Period::Instant(d) => d.clone(),
        Period::Duration { start, end } => format!("{start} to {end}"),
        Period::Forever => "forever".to_owned(),
    }
}

fn product(measures: &[QName]) -> String {
    measures
        .iter()
        .map(qname_display)
        .collect::<Vec<_>>()
        .join("*")
}

/// Measures joined by `*`; a divide is `numerator / denominator`.
#[must_use]
pub fn format_unit(unit: &Unit) -> String {
    match &unit.measures {
        UnitMeasures::Simple(m) => product(m),
        UnitMeasures::Divide {
            numerator,
            denominator,
        } => format!("{} / {}", product(numerator), product(denominator)),
    }
}

/// `decimals=N`, `precision=N`, or `accuracy=INF` (the model does not record
/// which attribute carried `INF`).
#[must_use]
pub fn format_accuracy(accuracy: Accuracy) -> String {
    match accuracy {
        Accuracy::Decimals(n) => format!("decimals={n}"),
        Accuracy::Precision(n) => format!("precision={n}"),
        Accuracy::Infinite => "accuracy=INF".to_owned(),
    }
}
