//! End-to-end CLI smoke tests against the checked-in fixtures.
//!
//! All runs are local-only. Diagnostics are asserted by stable rule id and by
//! status, never by "some error happened"; rendered output is compared with
//! the same goldens used by `xbrl-render`'s fixture smoke.

use xbrl_cli::{ExitStatus, Outcome, run};

fn fixture(relative: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(relative)
        .to_string_lossy()
        .into_owned()
}

fn golden(relative: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/render")
            .join(relative),
    )
    .unwrap()
}

fn json(outcome: &Outcome) -> serde_json::Value {
    serde_json::from_str(outcome.stdout())
        .unwrap_or_else(|error| panic!("stdout was not JSON: {error}\n{}", outcome.stdout()))
}

#[test]
fn parse_human_summary_reports_the_typed_model_counts() {
    let outcome = run(["parse", &fixture("instance/basic.xml")]);
    assert_eq!(outcome.status(), ExitStatus::Success);
    assert!(
        outcome.stdout().contains("schemaRefs: 1"),
        "{}",
        outcome.stdout()
    );
    assert!(
        outcome.stdout().contains("contexts: 2"),
        "{}",
        outcome.stdout()
    );
    assert!(
        outcome.stdout().contains("units: 2"),
        "{}",
        outcome.stdout()
    );
    assert!(
        outcome.stdout().contains("facts: 4"),
        "{}",
        outcome.stdout()
    );
    assert_eq!(outcome.stderr(), "");
}

#[test]
fn parse_json_is_versioned_and_deterministic() {
    let path = fixture("instance/basic.xml");
    let first = run(["--json", "parse", &path]);
    let second = run(["parse", &path, "--json"]);
    assert_eq!(first.status(), ExitStatus::Success);
    assert_eq!(
        first.stdout(),
        second.stdout(),
        "flag position must not change JSON"
    );
    assert_eq!(first.stderr(), "");

    let value = json(&first);
    assert_eq!(value["schema"], "xbrl-sdk/cli-result/v1");
    assert_eq!(value["command"], "parse");
    assert_eq!(value["status"], "ok");
    assert_eq!(value["exit_code"], 0);
    assert_eq!(value["input"]["path"], path);
    assert_eq!(value["summary"]["schema_refs"], 1);
    assert_eq!(value["summary"]["contexts"], 2);
    assert_eq!(value["summary"]["units"], 2);
    assert_eq!(value["summary"]["facts"], 4);
    assert_eq!(value["diagnostics"], serde_json::json!([]));
    assert_eq!(value["diagnostics_truncated"], false);
    assert_eq!(value["error"], serde_json::Value::Null);
    assert_eq!(value["output"], serde_json::Value::Null);
}

#[test]
fn parse_rejects_a_wrong_root_with_its_stable_rule_id() {
    let path = fixture("instance/invalid-root.xml");
    let human = run(["parse", &path]);
    assert_eq!(human.status(), ExitStatus::Findings);
    assert_eq!(
        human.stdout(),
        "",
        "a failed parse must not print a computed result"
    );
    assert!(
        human.stderr().contains("xbrl-instance:invalid-root"),
        "{}",
        human.stderr()
    );

    let machine = run(["parse", "--json", &path]);
    let value = json(&machine);
    assert_eq!(machine.status(), ExitStatus::Findings);
    assert_eq!(value["status"], "findings");
    assert_eq!(value["exit_code"], 1);
    assert_eq!(
        value["diagnostics"][0]["rule"],
        "xbrl-instance:invalid-root"
    );
    assert_eq!(value["diagnostics"][0]["severity"], "error");
    assert!(
        value["diagnostics"][0]["span"]["end"].as_u64().unwrap()
            > value["diagnostics"][0]["span"]["start"].as_u64().unwrap()
    );
    assert_eq!(machine.stderr(), "");
}

#[test]
fn unreadable_instance_is_an_input_error_not_a_finding() {
    let missing = fixture("instance/does-not-exist.xml");
    let human = run(["parse", &missing]);
    assert_eq!(human.status(), ExitStatus::Input);
    assert!(
        human.stderr().contains("cannot read instance"),
        "{}",
        human.stderr()
    );

    let machine = run(["parse", "--json", &missing]);
    let value = json(&machine);
    assert_eq!(value["status"], "error");
    assert_eq!(value["exit_code"], 4);
    assert_eq!(value["summary"]["kind"], "input");
    assert!(
        value["error"]
            .as_str()
            .unwrap()
            .contains("cannot read instance")
    );
}

#[test]
fn validate_without_a_dts_fails_closed() {
    let outcome = run(["validate", "--json", &fixture("instance/basic.xml")]);
    let value = json(&outcome);
    assert_ne!(outcome.status(), ExitStatus::Success);
    assert_ne!(value["status"], "ok");
    assert!(
        value["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|finding| finding["rule"] == "xbrl-validate:unsupported"),
        "{}",
        value
    );
    assert!(value["summary"]["dts_entry"].is_null());
}

#[test]
fn validate_with_a_local_dts_reports_executed_rule_families() {
    let args = [
        "validate",
        "--json",
        &fixture("instance/basic.xml"),
        "--dts",
        &fixture("taxonomy"),
        "--entry",
        "example.xsd",
    ];
    let outcome = run(args);
    let value = json(&outcome);
    assert_eq!(outcome.status(), ExitStatus::Success, "{}", value);
    assert_eq!(value["status"], "ok");
    assert_eq!(value["exit_code"], 0);
    assert_eq!(value["summary"]["dts_entry"], "example.xsd");
    assert!(value["summary"]["rules_executed"].as_array().unwrap().len() >= 3);
    assert_eq!(value["summary"]["findings"], 0);
    assert_eq!(value["summary"]["errors"], 0);
    assert_eq!(value["diagnostics"], serde_json::json!([]));
}

#[test]
fn render_matches_the_render_crate_goldens_for_all_three_formats() {
    let instance = fixture("instance/basic.xml");
    let dts = fixture("taxonomy");
    for (format, golden_name) in [
        ("text", "example.txt"),
        ("html", "example.html"),
        ("csv", "example.csv"),
    ] {
        let outcome = run([
            "render",
            &instance,
            "--dts",
            &dts,
            "--entry",
            "example.xsd",
            "--format",
            format,
        ]);
        assert_eq!(
            outcome.status(),
            ExitStatus::Success,
            "{}",
            outcome.stderr()
        );
        assert_eq!(outcome.stdout(), golden(golden_name), "format {format}");
        assert_eq!(outcome.stderr(), "");
    }
}

#[test]
fn render_json_wraps_the_exact_rendered_text() {
    let outcome = run([
        "render",
        "--json",
        &fixture("instance/basic.xml"),
        "--dts",
        &fixture("taxonomy"),
        "--entry",
        "example.xsd",
    ]);
    let value = json(&outcome);
    assert_eq!(outcome.status(), ExitStatus::Success);
    assert_eq!(value["status"], "ok");
    assert_eq!(value["output"]["format"], "text");
    assert_eq!(value["output"]["text"], golden("example.txt"));
    assert_eq!(value["summary"]["reports"], 1);
    assert_eq!(value["summary"]["dts_entry"], "example.xsd");
}

#[test]
fn render_without_a_dts_is_a_usage_error_even_under_json() {
    let human = run(["render", &fixture("instance/basic.xml")]);
    assert_eq!(human.status(), ExitStatus::Usage);
    assert!(human.stderr().contains("--dts"), "{}", human.stderr());
    assert_eq!(human.stdout(), "");

    let machine = run(["render", "--json", &fixture("instance/basic.xml")]);
    let value = json(&machine);
    assert_eq!(machine.status(), ExitStatus::Usage);
    assert_eq!(value["status"], "error");
    assert_eq!(value["exit_code"], 2);
    assert_eq!(value["summary"]["kind"], "usage");
    assert!(value["error"].as_str().unwrap().contains("--dts"));
}

#[test]
fn unimplemented_is_gone_but_unknown_commands_still_fail_closed() {
    let outcome = run(["--json", "frobnicate"]);
    let value = json(&outcome);
    assert_eq!(outcome.status(), ExitStatus::Usage);
    assert_eq!(value["status"], "error");
    assert_eq!(value["exit_code"], 2);
    assert_eq!(value["error"], "unrecognized command `frobnicate`");
}
