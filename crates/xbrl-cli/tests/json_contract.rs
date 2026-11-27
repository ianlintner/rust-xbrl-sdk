//! Contract tests for the stable CLI JSON output.
//!
//! These assertions pin field names, ordering and value vocabulary: agents and
//! downstream tools parse this shape, so a change here is a breaking change.
//! Normative basis: RFC 8259 (JSON, 2017-12); rule/severity/span vocabulary is
//! `xbrl_core::diagnostic` (XBRL 2.1 REC 2003-12-31 + errata 2013-02-20).

use xbrl_cli::json::{
    DiagnosticView, Envelope, InputInfo, OutputView, ParseSummary, SCHEMA_ID, SpanView, Status,
    Summary,
};
use xbrl_core::diagnostic::{Diagnostic, RuleId, Severity, SourceSpan};

fn diagnostic() -> Diagnostic {
    Diagnostic::new(
        RuleId::new("xbrl-validate:unresolved-reference").unwrap(),
        Severity::Error,
        "contextRef \"missing\" has no context definition",
        Some(SourceSpan::new(120, 141).unwrap()),
    )
}

#[test]
fn diagnostic_view_has_stable_fields_lowercase_severity_and_half_open_span() {
    let view = DiagnosticView::from(&diagnostic());
    let value = serde_json::to_value(&view).unwrap();
    assert_eq!(
        serde_json::to_string(&view).unwrap(),
        "{\"rule\":\"xbrl-validate:unresolved-reference\",\"severity\":\"error\",\"message\":\"contextRef \\\"missing\\\" has no context definition\",\"span\":{\"start\":120,\"end\":141}}"
    );
    assert_eq!(value["severity"], "error");
    assert_eq!(value["span"]["start"], 120);
    assert_eq!(value["span"]["end"], 141);
    assert_eq!(value.as_object().unwrap().len(), 4);
}

#[test]
fn document_wide_diagnostic_has_null_span() {
    let finding = Diagnostic::new(
        RuleId::new("xbrl-validate:unsupported").unwrap(),
        Severity::Warning,
        "no DTS supplied",
        None,
    );
    let json = serde_json::to_string(&DiagnosticView::from(&finding)).unwrap();
    assert!(json.contains("\"span\":null"), "{json}");
    assert!(json.contains("\"severity\":\"warning\""), "{json}");
}

#[test]
fn severity_vocabulary_is_the_documented_lowercase_set() {
    for (severity, expected) in [
        (Severity::Info, "info"),
        (Severity::Warning, "warning"),
        (Severity::Error, "error"),
    ] {
        let finding = Diagnostic::new(
            RuleId::new("xbrl-core:test").unwrap(),
            severity,
            "message",
            None,
        );
        let value = serde_json::to_value(DiagnosticView::from(&finding)).unwrap();
        assert_eq!(value["severity"], expected);
    }
}

#[test]
fn envelope_field_order_and_schema_id_are_stable() {
    let envelope = Envelope {
        schema: SCHEMA_ID.to_owned(),
        command: "parse".to_owned(),
        status: Status::Ok,
        exit_code: 0,
        input: InputInfo {
            path: "instance.xbrl".to_owned(),
        },
        summary: Summary::Parse(ParseSummary {
            schema_refs: 1,
            contexts: 2,
            units: 2,
            facts: 4,
        }),
        diagnostics: Vec::new(),
        diagnostics_truncated: false,
        error: None,
        output: None,
    };
    let json = serde_json::to_string(&envelope).unwrap();
    let expected = concat!(
        "{\"schema\":\"xbrl-sdk/cli-result/v1\",\"command\":\"parse\",\"status\":\"ok\",",
        "\"exit_code\":0,\"input\":{\"path\":\"instance.xbrl\"},",
        "\"summary\":{\"schema_refs\":1,\"contexts\":2,\"units\":2,\"facts\":4},",
        "\"diagnostics\":[],\"diagnostics_truncated\":false,\"error\":null,\"output\":null}"
    );
    assert_eq!(json, expected);
}

#[test]
fn render_output_is_wrapped_with_exact_format_and_text() {
    let envelope = Envelope {
        schema: SCHEMA_ID.to_owned(),
        command: "render".to_owned(),
        status: Status::Ok,
        exit_code: 0,
        input: InputInfo {
            path: "instance.xbrl".to_owned(),
        },
        summary: Summary::Render {
            reports: 1,
            dts_entry: "example.xsd".to_owned(),
        },
        diagnostics: Vec::new(),
        diagnostics_truncated: false,
        error: None,
        output: Some(OutputView {
            format: "html".to_owned(),
            text: "<p>&amp;</p>\n".to_owned(),
        }),
    };
    let value = serde_json::to_value(&envelope).unwrap();
    assert_eq!(value["status"], "ok");
    assert_eq!(value["output"]["format"], "html");
    assert_eq!(value["output"]["text"], "<p>&amp;</p>\n");
}

#[test]
fn published_schema_matches_the_emitted_schema_id() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/cli-result.schema.json");
    let text = std::fs::read_to_string(&path).expect("published JSON schema exists");
    let schema: serde_json::Value = serde_json::from_str(&text).expect("schema is valid JSON");
    assert_eq!(schema["$id"], "xbrl-sdk/cli-result/v1");
    assert_eq!(schema["properties"]["schema"]["const"], SCHEMA_ID);
    for field in [
        "schema",
        "command",
        "status",
        "exit_code",
        "input",
        "summary",
        "diagnostics",
        "diagnostics_truncated",
        "error",
        "output",
    ] {
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&serde_json::Value::from(field)),
            "schema must require {field}"
        );
    }
}

#[test]
fn published_schema_defines_the_diagnostic_shape() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/cli-result.schema.json");
    let text = std::fs::read_to_string(&path).unwrap();
    let schema: serde_json::Value = serde_json::from_str(&text).unwrap();
    let diagnostic = &schema["$defs"]["diagnostic"];
    assert_eq!(diagnostic["type"], "object");
    for field in ["rule", "severity", "message", "span"] {
        assert!(
            diagnostic["required"]
                .as_array()
                .unwrap()
                .contains(&serde_json::Value::from(field)),
            "diagnostic must require {field}"
        );
    }
    assert_eq!(
        diagnostic["properties"]["severity"]["enum"],
        serde_json::json!(["info", "warning", "error"])
    );
    assert_eq!(
        diagnostic["properties"]["span"]["type"],
        serde_json::json!(["object", "null"])
    );
}

#[test]
fn span_view_round_trips_byte_offsets() {
    let span = SpanView { start: 7, end: 7 };
    assert_eq!(
        serde_json::to_string(&span).unwrap(),
        "{\"start\":7,\"end\":7}"
    );
}
