//! Hard resource limits applied by the streaming reader.
//!
//! Defaults are defensive: generous enough for large real filings, small enough
//! that well-known hostile shapes (deep nesting, attribute floods, huge tokens)
//! fail fast. They are starting points to tune per deployment; no performance
//! claim is attached to them.

/// Resource limits for one document. Every limit is enforced eagerly and
/// reported with its own stable rule id (see [`crate::rules`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    /// Maximum bytes read from the source. A document of exactly this size is
    /// accepted; one byte more is rejected. Default: 64 MiB.
    pub max_document_bytes: u64,
    /// Maximum element nesting depth (the root is depth 1). Default: 256.
    pub max_depth: u32,
    /// Maximum attributes (including `xmlns` declarations) on one element.
    /// Default: 512.
    pub max_attributes_per_element: u32,
    /// Maximum bytes of a single token: a name, an attribute value, a comment, or
    /// one run of coalesced character data. Default: 8 MiB.
    pub max_token_bytes: u64,
    /// Maximum namespace bindings in scope at once, inherited ones included.
    /// Default: 512.
    pub max_namespace_bindings: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_document_bytes: 64 * 1024 * 1024,
            max_depth: 256,
            max_attributes_per_element: 512,
            max_token_bytes: 8 * 1024 * 1024,
            max_namespace_bindings: 512,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_nonzero_and_bounded() {
        let l = Limits::default();
        assert!(l.max_document_bytes > 0 && l.max_depth > 0);
        assert!(l.max_attributes_per_element > 0 && l.max_token_bytes > 0);
        assert!(l.max_namespace_bindings > 0);
        assert!(l.max_token_bytes <= l.max_document_bytes);
    }
}
