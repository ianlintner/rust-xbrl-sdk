//! Stable, typed diagnostics with rule identifiers, severity and source spans.
//!
//! Every check in this workspace reports through [`Diagnostic`] so that CLI
//! JSON output, tests and future profile rules (SEC EFM / ESMA) share one
//! shape. Rule identifiers are meant to be stable across releases: changing one
//! is a breaking change for consumers filtering by rule id.
//!
//! Status: scaffold baseline. Only the envelope type exists — no checks emit
//! diagnostics yet.

use std::error::Error;
use std::fmt;

/// Severity of a diagnostic, ordered from least to most urgent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Context for a consumer that passed the check anyway.
    Info,
    /// Something suspicious that does not by itself invalidate the document.
    Warning,
    /// A spec or profile violation.
    Error,
}

impl Severity {
    /// Lowercase name used in CLI/JSON output: `info`, `warning` or `error`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A half-open byte range (`start..end`) into the source document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceSpan {
    start: usize,
    end: usize,
}

impl SourceSpan {
    /// Creates a span. Offsets are byte offsets, `end` is exclusive.
    ///
    /// # Errors
    ///
    /// Returns [`SpanError::Inverted`] when `end < start`.
    pub fn new(start: usize, end: usize) -> Result<Self, SpanError> {
        if end < start {
            return Err(SpanError::Inverted { start, end });
        }
        Ok(Self { start, end })
    }

    /// Inclusive start offset in bytes.
    #[must_use]
    pub fn start(self) -> usize {
        self.start
    }

    /// Exclusive end offset in bytes.
    #[must_use]
    pub fn end(self) -> usize {
        self.end
    }

    /// Length of the span in bytes.
    #[must_use]
    pub fn len(self) -> usize {
        self.end - self.start
    }

    /// Whether the span covers no bytes.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.start == self.end
    }
}

impl fmt::Display for SourceSpan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bytes {}..{}", self.start, self.end)
    }
}

/// Reason a [`SourceSpan`] could not be constructed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanError {
    /// `end` was smaller than `start`.
    Inverted {
        /// Requested start offset.
        start: usize,
        /// Requested end offset.
        end: usize,
    },
}

impl fmt::Display for SpanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inverted { start, end } => {
                write!(f, "span end {end} precedes start {start}")
            }
        }
    }
}

impl Error for SpanError {}

/// A stable diagnostic rule identifier such as `xbrl-instance:duplicate-context`.
///
/// Identifiers are lowercase, start with a letter and may contain ASCII letters,
/// digits, `-`, `_` and a single `:` separator between a namespace-ish prefix and
/// the rule name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuleId(String);

impl RuleId {
    /// Validates and stores `id`.
    ///
    /// # Errors
    ///
    /// Returns [`RuleIdError`] when the identifier is empty, starts with a
    /// non-letter, or contains a character outside `[a-z0-9\-_:]`.
    pub fn new(id: impl Into<String>) -> Result<Self, RuleIdError> {
        let id = id.into();
        let mut chars = id.chars();
        let Some(first) = chars.next() else {
            return Err(RuleIdError::Empty);
        };
        if !first.is_ascii_lowercase() {
            return Err(RuleIdError::InvalidCharacter { offending: first });
        }
        for c in chars {
            if !(c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | ':')) {
                return Err(RuleIdError::InvalidCharacter { offending: c });
            }
        }
        Ok(Self(id))
    }

    /// The identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Reason a [`RuleId`] could not be constructed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleIdError {
    /// The identifier was empty.
    Empty,
    /// The identifier contained a character outside `[a-z0-9\-_:]`, or did not
    /// start with a lowercase ASCII letter.
    InvalidCharacter {
        /// The offending character.
        offending: char,
    },
}

impl fmt::Display for RuleIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("rule id must not be empty"),
            Self::InvalidCharacter { offending } => write!(
                f,
                "rule id must match [a-z][a-z0-9\\-_:]*, found {offending:?}"
            ),
        }
    }
}

impl Error for RuleIdError {}

/// A single typed finding: rule, severity, message and optional source location.
///
/// # Examples
///
/// ```
/// use xbrl_core::diagnostic::{Diagnostic, RuleId, Severity, SourceSpan};
///
/// let diagnostic = Diagnostic::new(
///     RuleId::new("xbrl-instance:duplicate-context")?,
///     Severity::Error,
///     "context 'c1' is defined twice",
///     Some(SourceSpan::new(120, 141)?),
/// );
/// assert_eq!(diagnostic.severity(), Severity::Error);
/// assert_eq!(
///     diagnostic.to_string(),
///     "error [xbrl-instance:duplicate-context] context 'c1' is defined twice (bytes 120..141)"
/// );
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    rule: RuleId,
    severity: Severity,
    message: String,
    span: Option<SourceSpan>,
}

impl Diagnostic {
    /// Builds a diagnostic. `span` is `None` when the finding is document-wide.
    pub fn new(
        rule: RuleId,
        severity: Severity,
        message: impl Into<String>,
        span: Option<SourceSpan>,
    ) -> Self {
        Self {
            rule,
            severity,
            message: message.into(),
            span,
        }
    }

    /// The rule that produced this finding.
    #[must_use]
    pub fn rule(&self) -> &RuleId {
        &self.rule
    }

    /// How urgent the finding is.
    #[must_use]
    pub fn severity(&self) -> Severity {
        self.severity
    }

    /// Human-readable message (no rule id, no location).
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Source location, when the finding has one.
    #[must_use]
    pub fn span(&self) -> Option<SourceSpan> {
        self.span
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} [{}] {}", self.severity, self.rule, self.message)?;
        if let Some(span) = self.span {
            write!(f, " ({span})")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: &str) -> RuleId {
        RuleId::new(id).expect("valid rule id")
    }

    #[test]
    fn severity_orders_from_info_to_error_and_renders_lowercase() {
        assert!(Severity::Info < Severity::Warning);
        assert!(Severity::Warning < Severity::Error);
        assert_eq!(Severity::Warning.to_string(), "warning");
        assert_eq!(Severity::Error.as_str(), "error");
    }

    #[test]
    fn span_is_half_open_and_reports_length() {
        let span = SourceSpan::new(10, 14).expect("valid span");
        assert_eq!((span.start(), span.end(), span.len()), (10, 14, 4));
        assert!(!span.is_empty());

        let empty = SourceSpan::new(7, 7).expect("valid span");
        assert!(empty.is_empty());
        assert_eq!(empty.to_string(), "bytes 7..7");
    }

    #[test]
    fn inverted_span_is_rejected() {
        assert_eq!(
            SourceSpan::new(5, 4),
            Err(SpanError::Inverted { start: 5, end: 4 })
        );
        assert!(
            SpanError::Inverted { start: 5, end: 4 }
                .to_string()
                .contains("precedes")
        );
    }

    #[test]
    fn rule_id_accepts_namespaced_lowercase_ids() {
        let id = rule("xbrl-calc:inconsistent-sum");
        assert_eq!(id.as_str(), "xbrl-calc:inconsistent-sum");
        assert_eq!(id.to_string(), "xbrl-calc:inconsistent-sum");
    }

    #[test]
    fn rule_id_rejects_empty_uppercase_and_punctuation() {
        assert_eq!(RuleId::new(""), Err(RuleIdError::Empty));
        assert_eq!(
            RuleId::new("XBRL-Bad"),
            Err(RuleIdError::InvalidCharacter { offending: 'X' })
        );
        assert_eq!(
            RuleId::new("xbrl.bad"),
            Err(RuleIdError::InvalidCharacter { offending: '.' })
        );
    }

    #[test]
    fn renders_severity_rule_message_and_span() {
        let diagnostic = Diagnostic::new(
            rule("xbrl-instance:duplicate-context"),
            Severity::Error,
            "context 'c1' is defined twice",
            Some(SourceSpan::new(120, 141).expect("valid span")),
        );
        assert_eq!(
            diagnostic.rule().as_str(),
            "xbrl-instance:duplicate-context"
        );
        assert_eq!(diagnostic.severity(), Severity::Error);
        assert_eq!(diagnostic.message(), "context 'c1' is defined twice");
        assert_eq!(diagnostic.span().map(SourceSpan::len), Some(21));
        assert_eq!(
            diagnostic.to_string(),
            "error [xbrl-instance:duplicate-context] context 'c1' is defined twice (bytes 120..141)"
        );
    }

    #[test]
    fn renders_without_span_when_document_wide() {
        let diagnostic = Diagnostic::new(
            rule("xbrl-core:unsupported-construct"),
            Severity::Warning,
            "construct not implemented; treating as unsupported",
            None,
        );
        assert_eq!(diagnostic.span(), None);
        assert_eq!(
            diagnostic.to_string(),
            "warning [xbrl-core:unsupported-construct] construct not implemented; treating as unsupported"
        );
    }
}
