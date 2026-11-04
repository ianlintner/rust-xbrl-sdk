//! XML qualified names: namespace URI, optional lexical prefix and local name.
//!
//! XBRL identifies concepts and relationships by *namespace URI plus local
//! name*; the lexical prefix only matters for round-tripping a source document.
//! [`QName`] therefore keeps the resolved namespace, the local name and an
//! optional prefix, and renders Clark notation (`{namespace}local`) for display.
//!
//! Namespace URIs are compared with simple string equality. That is correct for
//! the lexical forms XBRL uses in practice; URI normalization is deliberately
//! not attempted here (see PLAN.md milestone 2 for resolver behavior).
//!
//! # Validation cost
//!
//! Validating a namespace URI or checking a name scans the whole string, so a
//! parser must do it **once per declaration**, not once per name that refers to
//! the declaration. [`QName::from_validated_shared`] builds a name from a URI
//! that a caller has already validated, and [`is_ncname`] exposes the NCName
//! character-class check so the reader and the instance extractor share one
//! table instead of each keeping its own. Re-validating a long URI for every
//! element in its scope makes document cost `O(nodes × URI length)`; with the
//! two entry points below it stays `O(declarations × URI length + nodes)`.
//!
//! Reference: XML Namespaces 1.0 (3rd ed.), XBRL 2.1 §3; XML 1.0 (5th ed.) §2.3
//! and Appendix B `NameStartChar`/`NameChar`.

use std::cell::Cell;
use std::error::Error;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Reason a [`QName`] could not be constructed or parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QNameError {
    /// The namespace URI was empty.
    EmptyNamespace,
    /// The namespace URI contained whitespace or a brace character.
    InvalidNamespace {
        /// The rejected namespace URI.
        namespace: String,
    },
    /// The local name was empty.
    EmptyLocalName,
    /// The local name contained a character that cannot appear in an NCName.
    InvalidLocalName {
        /// The rejected local name.
        local_name: String,
        /// The first character that made the local name invalid.
        offending: char,
    },
    /// An explicit prefix was empty or contained `:`.
    InvalidPrefix {
        /// The rejected prefix.
        prefix: String,
    },
    /// The lexical form was not `{namespace}local`.
    MalformedClarkNotation {
        /// The rejected input.
        input: String,
    },
}

impl fmt::Display for QNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyNamespace => f.write_str("namespace URI must not be empty"),
            Self::InvalidNamespace { namespace } => write!(
                f,
                "namespace URI {namespace:?} must not contain whitespace or brace characters"
            ),
            Self::EmptyLocalName => f.write_str("local name must not be empty"),
            Self::InvalidLocalName {
                local_name,
                offending,
            } => write!(
                f,
                "local name {local_name:?} contains {offending:?}, which is not valid in an NCName"
            ),
            Self::InvalidPrefix { prefix } => {
                write!(f, "prefix {prefix:?} must be non-empty and free of ':'")
            }
            Self::MalformedClarkNotation { input } => {
                write!(
                    f,
                    "expected Clark notation '{{namespace}}local', got {input:?}"
                )
            }
        }
    }
}

impl Error for QNameError {}

/// An XML qualified name resolved to a namespace URI.
///
/// # Examples
///
/// ```
/// use xbrl_core::qname::QName;
///
/// let name = QName::new(Some("xbrli"), "http://www.xbrl.org/2003/instance", "context")?;
/// assert_eq!(name.to_clark(), "{http://www.xbrl.org/2003/instance}context");
/// assert_eq!(name.to_prefixed(), "xbrli:context");
/// # Ok::<(), xbrl_core::qname::QNameError>(())
/// ```
#[derive(Debug, Clone)]
pub struct QName {
    prefix: Option<String>,
    namespace: Arc<str>,
    local_name: String,
}

impl PartialEq for QName {
    fn eq(&self, other: &Self) -> bool {
        self.namespace == other.namespace && self.local_name == other.local_name
    }
}
impl Eq for QName {}
impl Hash for QName {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.namespace.hash(state);
        self.local_name.hash(state);
    }
}

impl QName {
    /// Builds a qualified name, rejecting lexical forms that cannot appear in XML.
    ///
    /// `prefix` is `None` for a default-namespace (unprefixed) name.
    ///
    /// # Errors
    ///
    /// Returns [`QNameError`] when the namespace is empty or malformed, when the
    /// local name is empty or is not a simplified NCName, or when the prefix is
    /// empty or contains `:`.
    pub fn new(
        prefix: Option<&str>,
        namespace: &str,
        local_name: &str,
    ) -> Result<Self, QNameError> {
        validate_namespace(namespace)?;
        Self::from_validated_shared(prefix, Arc::from(namespace), local_name)
    }

    /// Like [`QName::new`], but shares an already-allocated namespace URI
    /// instead of copying it, so many names in one document reference a single
    /// allocation.
    ///
    /// # Errors
    ///
    /// Same as [`QName::new`].
    pub fn from_shared(
        prefix: Option<&str>,
        namespace: Arc<str>,
        local_name: &str,
    ) -> Result<Self, QNameError> {
        validate_namespace(&namespace)?;
        Self::from_validated_shared(prefix, namespace, local_name)
    }

    /// Like [`QName::from_shared`], but trusts that `namespace` already passed
    /// [`validate_namespace`] (or an equivalent check at the declaration that
    /// produced it) and therefore does not rescan the URI.
    ///
    /// The local name and prefix are still checked. Use this on a parse hot path
    /// after validating each namespace declaration once; using it with an
    /// unvalidated URI silently admits a URI that [`QName::new`] would reject.
    ///
    /// # Errors
    ///
    /// Returns [`QNameError::EmptyLocalName`]/[`QNameError::InvalidLocalName`]
    /// for a bad local name and [`QNameError::InvalidPrefix`] for a bad prefix.
    pub fn from_validated_shared(
        prefix: Option<&str>,
        namespace: Arc<str>,
        local_name: &str,
    ) -> Result<Self, QNameError> {
        validate_local_name(local_name)?;
        if let Some(prefix) = prefix
            && (prefix.is_empty() || prefix.contains(':'))
        {
            return Err(QNameError::InvalidPrefix {
                prefix: prefix.to_owned(),
            });
        }
        Ok(Self {
            prefix: prefix.map(str::to_owned),
            namespace,
            local_name: local_name.to_owned(),
        })
    }

    /// Parses Clark notation, e.g. `{http://www.xbrl.org/2003/instance}context`.
    ///
    /// The prefix is always [`None`]: Clark notation cannot carry one.
    ///
    /// # Errors
    ///
    /// Returns [`QNameError`] for anything that is not `{namespace}local` with a
    /// namespace and local name that pass [`QName::new`] validation.
    pub fn parse_clark(input: &str) -> Result<Self, QNameError> {
        let malformed = || QNameError::MalformedClarkNotation {
            input: input.to_owned(),
        };
        let Some(rest) = input.strip_prefix('{') else {
            return Err(malformed());
        };
        let Some((namespace, local_name)) = rest.split_once('}') else {
            return Err(malformed());
        };
        Self::new(None, namespace, local_name)
    }

    /// The resolved namespace URI.
    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// The local name, without prefix.
    #[must_use]
    pub fn local_name(&self) -> &str {
        &self.local_name
    }

    /// The lexical prefix, if the name was written with one.
    #[must_use]
    pub fn prefix(&self) -> Option<&str> {
        self.prefix.as_deref()
    }

    /// Returns a copy of this name carrying `prefix`.
    ///
    /// # Errors
    ///
    /// Returns [`QNameError::InvalidPrefix`] when `prefix` is empty or contains `:`.
    pub fn with_prefix(&self, prefix: &str) -> Result<Self, QNameError> {
        // `self.namespace` was validated when this name was built, so there is
        // no need to rescan it here.
        Self::from_validated_shared(Some(prefix), Arc::clone(&self.namespace), &self.local_name)
    }

    /// Renders Clark notation: `{namespace}local`.
    #[must_use]
    pub fn to_clark(&self) -> String {
        format!("{{{}}}{}", self.namespace, self.local_name)
    }

    /// Renders the lexical form: `prefix:local`, or `local` when unprefixed.
    #[must_use]
    pub fn to_prefixed(&self) -> String {
        match &self.prefix {
            Some(prefix) => format!("{prefix}:{}", self.local_name),
            None => self.local_name.clone(),
        }
    }
}

impl fmt::Display for QName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_clark())
    }
}

// Number of namespace-URI validations performed on this thread.
//
// Test instrumentation: it lets a regression test assert that a document with
// one long namespace URI and many elements validates the URI once per
// declaration rather than once per name. Not part of the stable API.
thread_local! {
    static NAMESPACE_VALIDATIONS: Cell<u64> = const { Cell::new(0) };
}

/// Resets the per-thread namespace-validation counter (see
/// [`namespace_validations`]). Test instrumentation; not part of the stable API.
#[doc(hidden)]
pub fn reset_namespace_validations() {
    NAMESPACE_VALIDATIONS.with(|count| count.set(0));
}

/// The per-thread namespace-validation count since the last
/// [`reset_namespace_validations`]. Test instrumentation; not part of the stable
/// API.
#[doc(hidden)]
#[must_use]
pub fn namespace_validations() -> u64 {
    NAMESPACE_VALIDATIONS.with(Cell::get)
}

/// Rejects a namespace URI that is empty or contains whitespace or a brace.
///
/// Exposed so a parser can validate each namespace declaration once and then
/// build names with [`QName::from_validated_shared`] without rescanning the URI
/// for every element in the declaration's scope.
///
/// # Errors
///
/// [`QNameError::EmptyNamespace`] when `namespace` is empty and
/// [`QNameError::InvalidNamespace`] when it contains whitespace or a brace.
pub fn validate_namespace(namespace: &str) -> Result<(), QNameError> {
    NAMESPACE_VALIDATIONS.with(|count| count.set(count.get().wrapping_add(1)));
    if namespace.is_empty() {
        return Err(QNameError::EmptyNamespace);
    }
    if namespace
        .chars()
        .any(|c| c.is_whitespace() || c == '{' || c == '}')
    {
        return Err(QNameError::InvalidNamespace {
            namespace: namespace.to_owned(),
        });
    }
    Ok(())
}

/// True when `s` is a nonempty NCName: an XML 1.0 `Name` with no `:` (XML 1.0
/// 5th ed. §2.3 productions `[4]`/`[4a]`, minus the `:` alternative).
///
/// The full `NameStartChar`/`NameChar` ranges are used, so Unicode letters such
/// as `Ü` or `測` are accepted, `·` (U+00B7) is accepted after the first
/// character but not as one, and characters outside the production such as `²`
/// (U+00B2) are rejected. `:` is rejected everywhere.
#[must_use]
pub fn is_ncname(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        None => false,
        Some(first) => is_name_start_char(first) && chars.all(is_name_char),
    }
}

/// XML 1.0 5th ed. `NameStartChar` minus `:`.
fn is_name_start_char(c: char) -> bool {
    matches!(c,
        'A'..='Z' | '_' | 'a'..='z'
        | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}' | '\u{F8}'..='\u{2FF}'
        | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}'
        | '\u{200C}'..='\u{200D}' | '\u{2070}'..='\u{218F}'
        | '\u{2C00}'..='\u{2FEF}' | '\u{3001}'..='\u{D7FF}'
        | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}'
        | '\u{10000}'..='\u{EFFFF}')
}

/// XML 1.0 5th ed. `NameChar` minus `:`.
fn is_name_char(c: char) -> bool {
    is_name_start_char(c)
        || matches!(c,
            '-' | '.' | '0'..='9'
            | '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}')
}

/// NCName check for a local name (XML 1.0 5th ed. `[4a]` minus `:`), reporting
/// the first offending character.
fn validate_local_name(local_name: &str) -> Result<(), QNameError> {
    let invalid = |offending: char| QNameError::InvalidLocalName {
        local_name: local_name.to_owned(),
        offending,
    };
    let mut chars = local_name.chars();
    let Some(first) = chars.next() else {
        return Err(QNameError::EmptyLocalName);
    };
    if !is_name_start_char(first) {
        return Err(invalid(first));
    }
    for c in chars {
        if !is_name_char(c) {
            return Err(invalid(c));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTANCE: &str = "http://www.xbrl.org/2003/instance";

    #[test]
    fn prefixes_are_not_part_of_expanded_name_identity() {
        let a = QName::new(Some("a"), INSTANCE, "context").unwrap();
        let b = QName::new(Some("b"), INSTANCE, "context").unwrap();
        assert_eq!(a, b);
        let mut names = std::collections::HashSet::new();
        names.insert(a);
        assert!(names.contains(&b));
    }

    #[test]
    fn builds_and_renders_a_prefixed_name() {
        let name = QName::new(Some("xbrli"), INSTANCE, "context").expect("valid qname");
        assert_eq!(name.prefix(), Some("xbrli"));
        assert_eq!(name.namespace(), INSTANCE);
        assert_eq!(name.local_name(), "context");
        assert_eq!(name.to_prefixed(), "xbrli:context");
        assert_eq!(
            name.to_clark(),
            "{http://www.xbrl.org/2003/instance}context"
        );
    }

    #[test]
    fn unprefixed_name_renders_local_name_only() {
        let name = QName::new(None, INSTANCE, "context").expect("valid qname");
        assert_eq!(name.prefix(), None);
        assert_eq!(name.to_prefixed(), "context");
    }

    #[test]
    fn parses_clark_notation_round_trip() {
        let parsed = QName::parse_clark("{http://www.xbrl.org/2003/instance}context")
            .expect("valid clark notation");
        assert_eq!(parsed.prefix(), None);
        assert_eq!(
            parsed.to_clark(),
            "{http://www.xbrl.org/2003/instance}context"
        );
    }

    #[test]
    fn prefix_is_carried_through_with_prefix() {
        let name = QName::new(None, INSTANCE, "context")
            .and_then(|q| q.with_prefix("xbrli"))
            .expect("valid");
        assert_eq!(name.to_prefixed(), "xbrli:context");
    }

    #[test]
    fn rejects_empty_namespace() {
        assert_eq!(
            QName::new(None, "", "context"),
            Err(QNameError::EmptyNamespace)
        );
    }

    #[test]
    fn rejects_namespace_with_spaces_or_braces() {
        assert!(matches!(
            QName::new(None, "http://example.com/ {odd}", "context"),
            Err(QNameError::InvalidNamespace { .. })
        ));
    }

    #[test]
    fn rejects_empty_local_name() {
        assert_eq!(
            QName::new(None, INSTANCE, ""),
            Err(QNameError::EmptyLocalName)
        );
    }

    #[test]
    fn rejects_local_name_with_colon_braces_or_bad_start() {
        assert!(matches!(
            QName::new(None, INSTANCE, "a:b"),
            Err(QNameError::InvalidLocalName { offending: ':', .. })
        ));
        assert!(matches!(
            QName::new(None, INSTANCE, "1st"),
            Err(QNameError::InvalidLocalName { offending: '1', .. })
        ));
        assert!(matches!(
            QName::new(None, INSTANCE, "a}b"),
            Err(QNameError::InvalidLocalName { offending: '}', .. })
        ));
    }

    #[test]
    fn rejects_empty_or_colon_bearing_prefix() {
        assert!(matches!(
            QName::new(Some(""), INSTANCE, "context"),
            Err(QNameError::InvalidPrefix { .. })
        ));
        assert!(matches!(
            QName::new(Some("a:b"), INSTANCE, "context"),
            Err(QNameError::InvalidPrefix { .. })
        ));
    }

    #[test]
    fn rejects_malformed_clark_notation() {
        for input in ["context", "{}context", "{}", "{http://example.com}"] {
            assert!(
                QName::parse_clark(input).is_err(),
                "expected {input:?} to be rejected"
            );
        }
    }

    #[test]
    fn errors_describe_the_problem() {
        let message = QName::new(None, "", "").unwrap_err().to_string();
        assert!(message.contains("namespace"), "got {message:?}");
        let message = QName::parse_clark("context").unwrap_err().to_string();
        assert!(message.contains("Clark notation"), "got {message:?}");
    }

    #[test]
    fn ncname_uses_the_xml_name_character_classes() {
        for valid in [
            "a",
            "_a",
            "a-b.c",
            "a\u{B7}b",
            "a\u{300}b",
            "Ü",
            "測",
            "x1",
            "⁰",
        ] {
            assert!(is_ncname(valid), "expected {valid:?} to be an NCName");
        }
        for invalid in [
            "",
            "1a",
            "\u{B7}a",
            "a\u{B2}",
            "a b",
            "a:b",
            "a{",
            "a}",
            "a\u{200B}",
        ] {
            assert!(
                !is_ncname(invalid),
                "expected {invalid:?} not to be an NCName"
            );
        }
    }

    #[test]
    fn local_names_follow_the_xml_ncname_production() {
        assert!(QName::new(None, INSTANCE, "a\u{B7}b").is_ok());
        assert!(matches!(
            QName::new(None, INSTANCE, "a\u{B2}"),
            Err(QNameError::InvalidLocalName {
                offending: '\u{B2}',
                ..
            })
        ));
    }

    #[test]
    fn namespace_uri_is_validated_once_per_construction() {
        reset_namespace_validations();
        QName::new(None, INSTANCE, "context").expect("valid qname");
        assert_eq!(namespace_validations(), 1);
        QName::from_shared(None, Arc::from(INSTANCE), "context").expect("valid qname");
        assert_eq!(namespace_validations(), 2);
        // The already-validated constructor must not rescan the URI.
        QName::from_validated_shared(None, Arc::from(INSTANCE), "context").expect("valid qname");
        assert_eq!(namespace_validations(), 2);
        // `with_prefix` reuses the namespace this name already validated.
        let name = QName::new(None, INSTANCE, "context").expect("valid qname");
        name.with_prefix("xbrli").expect("valid prefix");
        assert_eq!(namespace_validations(), 3);
    }
}
