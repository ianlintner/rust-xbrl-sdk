//! Stable rule identifiers emitted by the DTS layer.
//!
//! Rule ids are part of the CLI/JSON contract: changing one is breaking.

/// A referenced local document does not exist or cannot be read.
pub const REFERENCE_NOT_FOUND: &str = "xbrl-taxonomy:reference-not-found";
/// A discovery path revisits a document already on its current stack.
pub const DISCOVERY_CYCLE: &str = "xbrl-taxonomy:discovery-cycle";
/// The DTS document budget was exceeded.
pub const LIMIT_DOCUMENTS: &str = "xbrl-taxonomy:limit-documents";
/// The number of discovered concepts, types or relationships was too large.
pub const LIMIT_ITEMS: &str = "xbrl-taxonomy:limit-items";
/// A class of document or XML construct this layer does not support.
pub const UNSUPPORTED: &str = "xbrl-taxonomy:unsupported";
/// The schema root is not `xs:schema` (or the linkbase root is not `link:linkbase`).
pub const INVALID_ROOT: &str = "xbrl-taxonomy:invalid-root";
/// Required or malformed schema/linkbase syntax.
pub const INVALID_STRUCTURE: &str = "xbrl-taxonomy:invalid-structure";
/// More than one XML ID definition in one document carries the same value.
pub const DUPLICATE_ID: &str = "xbrl-taxonomy:duplicate-id";
/// Two different elements expand to the same QName in one DTS.
pub const DUPLICATE_CONCEPT: &str = "xbrl-taxonomy:duplicate-concept";
/// Two different schema types expand to the same QName in one DTS.
pub const DUPLICATE_TYPE: &str = "xbrl-taxonomy:duplicate-type";
/// A locator's `xlink:href` names an id that no DTS document defines, names a
/// document outside the DTS, or names a non-concept component where a concept
/// is required.
pub const LOCATOR_UNRESOLVED: &str = "xbrl-taxonomy:locator-unresolved";
/// An arc endpoint label has no locator/resource in its extended link.
pub const ARC_UNRESOLVED: &str = "xbrl-taxonomy:arc-unresolved";
/// A presentation or calculation graph contains a cycle.
pub const GRAPH_CYCLE: &str = "xbrl-taxonomy:graph-cycle";
/// A QName-valued attribute uses an unbound prefix or cannot be represented.
pub const INVALID_QNAME: &str = "xbrl-taxonomy:invalid-qname";
/// An `order` or `weight` attribute is not a valid `xs:decimal`.
pub const INVALID_NUMERIC: &str = "xbrl-taxonomy:invalid-numeric";

/// Every rule id this crate can emit, for exhaustive checks and documentation.
pub const ALL: &[&str] = &[
    REFERENCE_NOT_FOUND,
    DISCOVERY_CYCLE,
    LIMIT_DOCUMENTS,
    LIMIT_ITEMS,
    UNSUPPORTED,
    INVALID_ROOT,
    INVALID_STRUCTURE,
    DUPLICATE_ID,
    DUPLICATE_CONCEPT,
    DUPLICATE_TYPE,
    LOCATOR_UNRESOLVED,
    ARC_UNRESOLVED,
    GRAPH_CYCLE,
    INVALID_QNAME,
    INVALID_NUMERIC,
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
            assert!(id.starts_with("xbrl-taxonomy:"), "{id} lacks prefix");
            assert!(seen.insert(*id), "{id} is duplicated");
        }
    }
}
