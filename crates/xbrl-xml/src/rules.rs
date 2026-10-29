//! Stable rule identifiers for findings the XML layer produces.
//!
//! Consumers filter diagnostics on these strings, so changing one is a breaking
//! change (see `docs/architecture.md`). Every constant is a valid
//! [`xbrl_core::diagnostic::RuleId`]; a unit test enforces that.

// --- resource limits ---

/// The document exceeded its byte budget.
pub const LIMIT_DOCUMENT_SIZE: &str = "xbrl-xml:limit-document-size";
/// Element nesting ran past the configured depth.
pub const LIMIT_DEPTH: &str = "xbrl-xml:limit-depth";
/// An element carried more attributes than allowed.
pub const LIMIT_ATTRIBUTES: &str = "xbrl-xml:limit-attributes";
/// A single token (name, attribute value, coalesced text, comment) grew past the limit.
pub const LIMIT_TOKEN_SIZE: &str = "xbrl-xml:limit-token-size";
/// Too many namespace bindings were in scope at once.
pub const LIMIT_NAMESPACE_BINDINGS: &str = "xbrl-xml:limit-namespace-bindings";

// --- namespaces ---

/// A prefixed name used a prefix with no in-scope declaration.
pub const UNBOUND_PREFIX: &str = "xbrl-xml:unbound-prefix";
/// A namespace declaration violates Namespaces in XML 1.0 §3 / §6.
pub const INVALID_NAMESPACE_DECLARATION: &str = "xbrl-xml:invalid-namespace-declaration";
/// Two attributes on one element have the same expanded name (Namespaces §6.3).
pub const DUPLICATE_ATTRIBUTE: &str = "xbrl-xml:duplicate-attribute";
/// A name is not a valid (optionally prefixed) NCName.
pub const INVALID_NAME: &str = "xbrl-xml:invalid-name";

// --- well-formedness and encoding ---

/// The document is not well-formed XML.
pub const MALFORMED: &str = "xbrl-xml:malformed";
/// The document is not valid UTF-8.
pub const INVALID_ENCODING: &str = "xbrl-xml:invalid-encoding";
/// The XML declaration names a non-UTF-8 encoding.
pub const UNSUPPORTED_ENCODING: &str = "xbrl-xml:unsupported-encoding";
/// The XML declaration names a version other than 1.0.
pub const UNSUPPORTED_VERSION: &str = "xbrl-xml:unsupported-version";
/// A character reference names a character that is not legal in XML 1.0.
pub const INVALID_CHARACTER_REFERENCE: &str = "xbrl-xml:invalid-character-reference";
/// A DOCTYPE declaration was present; the reader refuses all of them.
pub const DOCTYPE_FORBIDDEN: &str = "xbrl-xml:doctype-forbidden";
/// A named entity other than the five predefined ones was referenced.
pub const UNDEFINED_ENTITY: &str = "xbrl-xml:undefined-entity";
/// Reading the underlying source failed.
pub const IO_ERROR: &str = "xbrl-xml:io-error";

// --- resolver / catalog ---

/// The catalog root element is missing or uses the wrong namespace.
pub const CATALOG_INVALID_ROOT: &str = "xbrl-xml:catalog-invalid-root";
/// A remote URI scheme was blocked (only `file:` is permitted).
pub const RESOLVER_REMOTE_BLOCKED: &str = "xbrl-xml:resolver-remote-blocked";
/// The resolved path is outside the document root.
pub const RESOLVER_OUTSIDE_ROOT: &str = "xbrl-xml:resolver-outside-root";
/// The document root is not accessible.
pub const RESOLVER_ROOT_NOT_ACCESSIBLE: &str = "xbrl-xml:resolver-root-not-accessible";
/// A resolved file exceeds the size limit.
pub const RESOLVER_FILE_TOO_LARGE: &str = "xbrl-xml:resolver-file-too-large";
/// Reading a local file failed (not a regular file, permission, symlink loop, ...).
pub const RESOLVER_IO_ERROR: &str = "xbrl-xml:resolver-io-error";
/// The file does not exist (inside the document root).
pub const RESOLVER_NOT_FOUND: &str = "xbrl-xml:resolver-not-found";
/// A catalog entry is missing required attributes or has empty required values.
pub const CATALOG_INVALID_ENTRY: &str = "xbrl-xml:catalog-invalid-entry";
/// The catalog uses a feature not yet implemented (fail-closed).
pub const CATALOG_UNSUPPORTED: &str = "xbrl-xml:catalog-unsupported";
/// A nextCatalog chain contains a cycle.
pub const CATALOG_CYCLE: &str = "xbrl-xml:catalog-cycle";
/// The catalog entry or file budget has been exceeded.
pub const RESOLVER_LIMIT_CATALOG: &str = "xbrl-xml:resolver-limit-catalog";
/// The URI is not a valid reference.
pub const RESOLVER_INVALID_URI: &str = "xbrl-xml:resolver-invalid-uri";

/// Every rule id this crate can emit, for exhaustive checks and documentation.
pub const ALL: &[&str] = &[
    LIMIT_DOCUMENT_SIZE,
    LIMIT_DEPTH,
    LIMIT_ATTRIBUTES,
    LIMIT_TOKEN_SIZE,
    LIMIT_NAMESPACE_BINDINGS,
    UNBOUND_PREFIX,
    INVALID_NAMESPACE_DECLARATION,
    DUPLICATE_ATTRIBUTE,
    INVALID_NAME,
    MALFORMED,
    INVALID_ENCODING,
    UNSUPPORTED_ENCODING,
    UNSUPPORTED_VERSION,
    INVALID_CHARACTER_REFERENCE,
    DOCTYPE_FORBIDDEN,
    UNDEFINED_ENTITY,
    IO_ERROR,
    CATALOG_INVALID_ROOT,
    RESOLVER_REMOTE_BLOCKED,
    RESOLVER_OUTSIDE_ROOT,
    RESOLVER_ROOT_NOT_ACCESSIBLE,
    RESOLVER_FILE_TOO_LARGE,
    RESOLVER_IO_ERROR,
    RESOLVER_NOT_FOUND,
    CATALOG_INVALID_ENTRY,
    CATALOG_UNSUPPORTED,
    CATALOG_CYCLE,
    RESOLVER_LIMIT_CATALOG,
    RESOLVER_INVALID_URI,
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use xbrl_core::diagnostic::RuleId;

    #[test]
    fn every_rule_constant_is_a_valid_unique_rule_id() {
        let mut seen = HashSet::new();
        for id in ALL {
            assert!(RuleId::new(*id).is_ok(), "{id} is not a valid RuleId");
            assert!(id.starts_with("xbrl-xml:"), "{id} lacks the crate prefix");
            assert!(seen.insert(*id), "{id} is duplicated");
        }
    }
}
