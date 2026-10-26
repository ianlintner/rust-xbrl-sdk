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
//! Reference: XML Namespaces 1.0 (3rd ed.), XBRL 2.1 §3.

use std::error::Error;
use std::fmt;

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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QName {
    prefix: Option<String>,
    namespace: String,
    local_name: String,
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
            namespace: namespace.to_owned(),
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
        Self::new(Some(prefix), &self.namespace, &self.local_name)
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

fn validate_namespace(namespace: &str) -> Result<(), QNameError> {
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

/// Simplified NCName check: enough to reject clearly impossible XBRL names
/// (empty, `:`, braces, whitespace, or a leading digit/dot/hyphen) without
/// claiming to implement the full XML Schema NCName production.
fn validate_local_name(local_name: &str) -> Result<(), QNameError> {
    let invalid = |offending: char| QNameError::InvalidLocalName {
        local_name: local_name.to_owned(),
        offending,
    };
    let mut chars = local_name.chars();
    let Some(first) = chars.next() else {
        return Err(QNameError::EmptyLocalName);
    };
    if !(first.is_alphabetic() || first == '_') {
        return Err(invalid(first));
    }
    for c in chars {
        if c == ':' || c == '{' || c == '}' || c.is_whitespace() {
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
}
