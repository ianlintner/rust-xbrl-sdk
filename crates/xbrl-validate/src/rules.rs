//! Stable rule identifiers emitted by the validation layer.
//!
//! Rule ids are part of the CLI/JSON contract: changing one is breaking. All
//! findings this layer emits are member of [`ALL`], including the fail-closed
//! rule for constructs this subset does not implement.

/// A required structural invariant of the XBRL 2.1 instance is violated.
pub const STRUCTURAL: &str = "xbrl-validate:structural";
/// A `contextRef` or `unitRef` does not name a defined ID.
pub const UNRESOLVED_REFERENCE: &str = "xbrl-validate:unresolved-reference";
/// Two facts are duplicate-consistent only when all their aspects agree.
pub const DUPLICATE_INCONSISTENT: &str = "xbrl-validate:duplicate-inconsistent";
/// A fact's numeric content is not a valid XSD 1.0 decimal lexical value.
pub const NON_NUMERIC_VALUE: &str = "xbrl-validate:non-numeric-value";
/// A calculation relationship's summation does not reconcile within accuracy.
pub const CALCULATION_INCONSISTENT: &str = "xbrl-validate:calculation-inconsistent";
/// A construct is required by a rule family but is not implemented here.
pub const UNSUPPORTED: &str = "xbrl-validate:unsupported";
/// Input exceeded a validation budget.
pub const LIMIT_EXCEEDED: &str = "xbrl-validate:limit-exceeded";

/// Every rule id this crate can emit, for exhaustive checks and documentation.
pub const ALL: &[&str] = &[
    STRUCTURAL,
    UNRESOLVED_REFERENCE,
    DUPLICATE_INCONSISTENT,
    NON_NUMERIC_VALUE,
    CALCULATION_INCONSISTENT,
    UNSUPPORTED,
    LIMIT_EXCEEDED,
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use xbrl_core::diagnostic::RuleId;

    #[test]
    fn every_rule_is_valid_unique_and_prefixed() {
        let mut seen = HashSet::new();
        for id in ALL {
            assert!(RuleId::new(*id).is_ok(), "{id} is not a valid rule id");
            assert!(id.starts_with("xbrl-validate:"), "{id} lacks prefix");
            assert!(seen.insert(*id), "{id} is duplicated");
        }
    }
}
