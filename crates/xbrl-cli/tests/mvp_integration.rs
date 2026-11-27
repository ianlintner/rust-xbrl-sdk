//! End-to-end MVP smoke over the same authored fixtures for all three commands.
//!
//! The unit tests in this crate cover individual commands. This suite checks
//! the integration contract: parse, validate and render agree on one instance;
//! all outputs carry the same schema id; the chain is deterministic; and the
//! negative fixture is rejected identically by every command (fail closed).

use xbrl_cli::{ExitStatus, run};

fn fixture(relative: &str) -> String {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    root.join(relative)
        .canonicalize()
        .unwrap_or_else(|error| panic!("fixture {relative}: {error}"))
        .to_string_lossy()
        .into_owned()
}

fn envelope(args: &[&str]) -> (ExitStatus, String, serde_json::Value) {
    let outcome = run(args);
    let stdout = outcome.stdout().to_owned();
    assert_eq!(outcome.stderr(), "", "args: {args:?}");
    assert_eq!(
        stdout.lines().count(),
        1,
        "the JSON envelope must be exactly one line: {args:?}"
    );
    let value: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|error| panic!("{args:?}: {error}: {stdout}"));
    assert_eq!(value["schema"], "xbrl-sdk/cli-result/v1");
    assert_eq!(value["exit_code"], outcome.status().code());
    (outcome.status(), stdout, value)
}

fn chain() -> Vec<Vec<&'static str>> {
    let instance = fixture("instance/basic.xml");
    let dts = fixture("taxonomy");
    // Keep the temporary strings alive by leaking them for the test process;
    // this keeps the command table readable and lets us reuse it for the
    // determinism assertion below.
    let instance: &'static str = Box::leak(instance.into_boxed_str());
    let dts: &'static str = Box::leak(dts.into_boxed_str());
    vec![
        vec!["--json", "parse", instance],
        vec![
            "--json",
            "validate",
            instance,
            "--dts",
            dts,
            "--entry",
            "example.xsd",
        ],
        vec![
            "--json",
            "render",
            instance,
            "--dts",
            dts,
            "--entry",
            "example.xsd",
            "--format",
            "text",
        ],
        vec![
            "--json",
            "render",
            instance,
            "--dts",
            dts,
            "--entry",
            "example.xsd",
            "--format",
            "html",
        ],
        vec![
            "--json",
            "render",
            instance,
            "--dts",
            dts,
            "--entry",
            "example.xsd",
            "--format",
            "csv",
        ],
    ]
}

#[test]
fn parse_validate_and_render_agree_on_the_authored_instance() {
    let commands = chain();
    let mut results = Vec::new();
    for args in &commands {
        let (status, _, value) = envelope(args);
        assert_eq!(status, ExitStatus::Success, "args: {args:?}");
        assert_eq!(value["status"], "ok");
        assert_eq!(value["diagnostics"], serde_json::json!([]));
        results.push(value);
    }

    let parsed = &results[0]["summary"];
    assert_eq!(parsed["schema_refs"], 1);
    assert_eq!(parsed["contexts"], 2);
    assert_eq!(parsed["units"], 2);
    assert_eq!(parsed["facts"], 4);

    let validated = &results[1]["summary"];
    assert_eq!(validated["dts_entry"], "example.xsd");
    assert_eq!(validated["findings"], 0);
    let executed = validated["rules_executed"]
        .as_array()
        .expect("rule list")
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect::<Vec<_>>();
    assert_eq!(
        executed,
        [
            "structural",
            "references",
            "duplicate-consistency",
            "calculation"
        ]
    );

    for (index, format) in ["text", "html", "csv"].iter().enumerate() {
        let rendered = &results[2 + index];
        assert_eq!(rendered["summary"]["reports"], 1);
        assert_eq!(rendered["output"]["format"], *format);
        let text = rendered["output"]["text"].as_str().expect("rendered text");
        assert!(text.contains("Total revenue"), "format {format}");
        assert!(text.contains("[missing]"), "format {format}");
        assert!(
            !text.contains("[unsupported"),
            "authored fixture has no dimensional contexts: {format}"
        );
    }
    let csv = results[4]["output"]["text"].as_str().expect("csv text");
    assert_eq!(csv.lines().count(), 4, "header, root, two child rows");
    assert!(
        results[2]["output"]["text"]
            .as_str()
            .expect("text report")
            .starts_with("ELR: http://www.xbrl.org/2003/role/presentationLink")
    );
    assert!(
        results[3]["output"]["text"]
            .as_str()
            .expect("html report")
            .starts_with("<!DOCTYPE html>")
    );
}

#[test]
fn the_chain_is_byte_for_byte_deterministic() {
    for args in chain() {
        let (_, first, _) = envelope(&args);
        let (_, second, _) = envelope(&args);
        assert_eq!(first, second, "args: {args:?}");
    }
}

#[test]
fn every_command_rejects_the_invalid_root_fixture() {
    let instance = fixture("instance/invalid-root.xml");
    let dts: &'static str = Box::leak(fixture("taxonomy").into_boxed_str());
    for command in ["parse", "validate", "render"] {
        let mut args = vec!["--json", command, instance.as_str()];
        if command == "validate" || command == "render" {
            args.extend(["--dts", dts, "--entry", "example.xsd"]);
        }
        let (status, _, value) = envelope(&args);
        assert_ne!(status, ExitStatus::Success, "command {command}");
        assert_eq!(value["status"], "findings", "command {command}");
        assert_eq!(
            value["diagnostics"][0]["rule"],
            "xbrl-instance:invalid-root"
        );
        assert_eq!(value["diagnostics"][0]["severity"], "error");
        assert!(value["output"].is_null(), "command {command}");
    }
}
