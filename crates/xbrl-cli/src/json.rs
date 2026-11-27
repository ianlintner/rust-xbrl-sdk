//! Stable, versioned JSON output for the `xbrl` CLI and agent consumers.
//!
//! This module is the serialization boundary of the SDK: the typed
//! [`xbrl_core::diagnostic::Diagnostic`] is converted into an owned, flat JSON
//! view so that `xbrl-core` does not depend on a serialization crate and rule
//! ids/severities stay identical everywhere. Field order, names and value
//! vocabulary are part of the contract published in
//! `docs/cli-result.schema.json`; changing them requires a new schema id.
//!
//! Normative basis: RFC 8259 (The JavaScript Object Notation, 2017-12) for the
//! wire format, and XBRL 2.1 REC 2003-12-31 with errata through 2013-02-20 for
//! the facts/contexts/units counted in the summaries. Spans are half-open byte
//! offsets into the original UTF-8 source, matching `xbrl_core::diagnostic`.

use serde::Serialize;

use xbrl_core::diagnostic::Diagnostic;

/// Versioned identifier of the JSON result envelope.
///
/// It is emitted verbatim in every JSON result and is the `$id` of the
/// published JSON Schema. A breaking change to any field requires a new id.
pub const SCHEMA_ID: &str = "xbrl-sdk/cli-result/v1";

/// Overall result of a command, independent of its exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// The command completed with no error-severity diagnostic.
    Ok,
    /// The command completed but emitted at least one error-severity finding.
    Findings,
    /// The command could not complete (usage error or input failure).
    Error,
}

/// Half-open byte range (`start..end`) in the source document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SpanView {
    /// Inclusive byte offset of the first byte.
    pub start: usize,
    /// Exclusive byte offset one past the last byte.
    pub end: usize,
}

/// One diagnostic in the stable JSON vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiagnosticView {
    /// Stable rule identifier, e.g. `xbrl-validate:unresolved-reference`.
    pub rule: String,
    /// One of `info`, `warning`, `error`.
    pub severity: String,
    /// Human-readable message without rule id or location.
    pub message: String,
    /// Byte span, or `null` for a document-wide finding.
    pub span: Option<SpanView>,
}

impl From<&Diagnostic> for DiagnosticView {
    fn from(value: &Diagnostic) -> Self {
        Self {
            rule: value.rule().as_str().to_owned(),
            severity: value.severity().as_str().to_owned(),
            message: value.message().to_owned(),
            span: value.span().map(|span| SpanView {
                start: span.start(),
                end: span.end(),
            }),
        }
    }
}

/// The instance path exactly as supplied on the command line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InputInfo {
    /// Instance path as supplied by the caller; never canonicalized.
    pub path: String,
}

/// Counts produced by the `parse` subcommand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParseSummary {
    /// Number of `xbrli:schemaRef` elements.
    pub schema_refs: usize,
    /// Number of contexts.
    pub contexts: usize,
    /// Number of units.
    pub units: usize,
    /// Number of direct item facts.
    pub facts: usize,
}

/// Counts and executed rule families produced by `validate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidateSummary {
    /// Rule families actually executed, in execution order.
    pub rules_executed: Vec<String>,
    /// Total number of retained findings.
    pub findings: usize,
    /// Findings at `Error` severity.
    pub errors: usize,
    /// Findings at `Warning` severity.
    pub warnings: usize,
    /// Findings at `Info` severity.
    pub info: usize,
    /// Whether the finding limit dropped additional findings.
    pub findings_truncated: bool,
    /// DTS entry used for concept-aware checks, when supplied.
    pub dts_entry: Option<String>,
}

/// Command-specific body of a result envelope.
///
/// The untagged representation keeps each variant a flat object, which makes
/// the published JSON Schema's `oneOf` straightforward for consumers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum Summary {
    /// `parse` counts.
    Parse(ParseSummary),
    /// `validate` counts and rule families.
    Validate(ValidateSummary),
    /// `render` report count and DTS entry.
    Render {
        /// Number of presentation networks rendered.
        reports: usize,
        /// DTS entry used to build the presentation networks.
        dts_entry: String,
    },
    /// No command-specific body was computed (the command failed early).
    Error {
        /// Failure class: `usage` or `input`.
        kind: String,
    },
}

/// Rendered payload for the `render` subcommand under `--json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OutputView {
    /// Requested format: `text`, `html` or `csv`.
    pub format: String,
    /// Exact rendered bytes as UTF-8 text; escaping is done by `xbrl-render`.
    pub text: String,
}

/// The complete machine-readable result of one CLI invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Envelope {
    /// Stable schema identifier; always [`SCHEMA_ID`].
    pub schema: String,
    /// Subcommand that produced this result.
    pub command: String,
    /// Coarse outcome; see [`Status`].
    pub status: Status,
    /// Process exit code, matching `xbrl_cli::ExitStatus::code`.
    pub exit_code: u8,
    /// Input identity.
    pub input: InputInfo,
    /// Command-specific body.
    pub summary: Summary,
    /// Typed findings in deterministic order.
    pub diagnostics: Vec<DiagnosticView>,
    /// Whether the producing component truncated its findings.
    pub diagnostics_truncated: bool,
    /// Human-readable reason when `status` is [`Status::Error`].
    pub error: Option<String>,
    /// Rendered payload, present only for `render --json`.
    pub output: Option<OutputView>,
}

impl Envelope {
    /// Serializes to compact, field-order-stable JSON.
    ///
    /// All fields are plain strings, integers, booleans, arrays and objects,
    /// so serialization cannot fail for a well-formed value.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("envelope serialization is infallible")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xbrl_core::diagnostic::{RuleId, Severity, SourceSpan};

    #[test]
    fn converting_a_typed_diagnostic_preserves_every_field() {
        let finding = Diagnostic::new(
            RuleId::new("xbrl-instance:invalid-root").unwrap(),
            Severity::Error,
            "expected xbrli:xbrl",
            Some(SourceSpan::new(3, 9).unwrap()),
        );
        assert_eq!(
            DiagnosticView::from(&finding),
            DiagnosticView {
                rule: "xbrl-instance:invalid-root".to_owned(),
                severity: "error".to_owned(),
                message: "expected xbrli:xbrl".to_owned(),
                span: Some(SpanView { start: 3, end: 9 }),
            }
        );
    }
}
