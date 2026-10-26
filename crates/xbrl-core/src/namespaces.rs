//! Namespace URIs defined by XBRL 2.1 and the namespaces XBRL documents reuse.
//!
//! These constants are copied verbatim from the specifications; they are compared
//! by string equality throughout the workspace.
//!
//! References: XBRL 2.1 §3 (namespaces), XLink 1.0, XBRL standard roles —
//! <https://specifications.xbrl.org/> and <https://www.xbrl.org/the-standard/what/specifications>.

/// XBRL 2.1 instance namespace (`xbrli` prefix by convention).
pub const XBRL_INSTANCE: &str = "http://www.xbrl.org/2003/instance";

/// XBRL 2.1 linkbase namespace (`link` prefix by convention).
pub const XBRL_LINKBASE: &str = "http://www.xbrl.org/2003/linkbase";

/// XLink namespace (`xlink` prefix by convention), used by XBRL linkbases.
pub const XLINK: &str = "http://www.w3.org/1999/xlink";

/// Base URI of the XBRL standard roles; a standard role appends its name, e.g.
/// `http://www.xbrl.org/2003/role/label` or `http://www.xbrl.org/2003/role/totalLabel`.
pub const STANDARD_ROLE_BASE: &str = "http://www.xbrl.org/2003/role/";

/// Whether `namespace` is one of the two namespaces XBRL 2.1 itself defines
/// (instance and linkbase). The XLink namespace is intentionally excluded: it is
/// reused by XBRL, not defined by it.
///
/// # Examples
///
/// ```
/// use xbrl_core::namespaces::{XLINK, is_xbrl_2_1_namespace};
///
/// assert!(is_xbrl_2_1_namespace("http://www.xbrl.org/2003/instance"));
/// assert!(!is_xbrl_2_1_namespace(XLINK));
/// ```
#[must_use]
pub fn is_xbrl_2_1_namespace(namespace: &str) -> bool {
    matches!(namespace, XBRL_INSTANCE | XBRL_LINKBASE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_constants_match_the_specification_text() {
        assert_eq!(XBRL_INSTANCE, "http://www.xbrl.org/2003/instance");
        assert_eq!(XBRL_LINKBASE, "http://www.xbrl.org/2003/linkbase");
        assert_eq!(XLINK, "http://www.w3.org/1999/xlink");
        assert_eq!(STANDARD_ROLE_BASE, "http://www.xbrl.org/2003/role/");
    }

    #[test]
    fn identifies_xbrl_2_1_namespaces_only() {
        assert!(is_xbrl_2_1_namespace(XBRL_INSTANCE));
        assert!(is_xbrl_2_1_namespace(XBRL_LINKBASE));
        assert!(!is_xbrl_2_1_namespace(XLINK));
        assert!(!is_xbrl_2_1_namespace("http://example.com/custom-taxonomy"));
        assert!(!is_xbrl_2_1_namespace("http://www.xbrl.org/2003/role/"));
    }

    #[test]
    fn standard_role_base_composes_with_role_names() {
        assert_eq!(
            format!("{STANDARD_ROLE_BASE}label"),
            "http://www.xbrl.org/2003/role/label"
        );
    }
}
