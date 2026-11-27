//! End-to-end smoke test for the installed `xbrl` binary.
//!
//! The library tests cover `xbrl_cli::run` in isolation; this test proves the
//! binary writes captured output to the right stream and exits with the
//! documented code, which is what a user or an agent harness actually sees.

use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_xbrl");

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run(args: &[&str]) -> Run {
    let output = Command::new(BIN).args(args).output().expect("spawn xbrl");
    Run {
        code: output.status.code().expect("process exited normally"),
        stdout: String::from_utf8(output.stdout).expect("stdout is UTF-8"),
        stderr: String::from_utf8(output.stderr).expect("stderr is UTF-8"),
    }
}

fn fixture(relative: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(relative)
        .to_string_lossy()
        .into_owned()
}

#[test]
fn version_exits_zero_and_prints_the_program_version() {
    let result = run(&["--version"]);
    assert_eq!(result.code, 0);
    assert_eq!(
        result.stdout,
        format!("xbrl {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(result.stderr, "");
}

#[test]
fn help_exits_zero_and_lists_the_implemented_commands() {
    let result = run(&["--help"]);
    assert_eq!(result.code, 0);
    assert!(result.stdout.contains("Usage: xbrl <COMMAND>"));
    for command in ["parse", "validate", "render"] {
        assert!(
            result.stdout.contains(command),
            "help must mention {command}"
        );
    }
    assert_eq!(result.stderr, "");
}

#[test]
fn parse_of_a_real_fixture_exits_zero() {
    let result = run(&["parse", &fixture("instance/basic.xml")]);
    assert_eq!(result.code, 0);
    assert!(result.stdout.contains("schemaRefs: 1"), "{}", result.stdout);
    assert_eq!(result.stderr, "");
}

#[test]
fn parse_of_a_bad_root_exits_one_and_reports_its_rule() {
    let result = run(&["parse", &fixture("instance/invalid-root.xml")]);
    assert_eq!(result.code, 1);
    assert_eq!(result.stdout, "");
    assert!(
        result.stderr.contains("xbrl-instance:invalid-root"),
        "{}",
        result.stderr
    );
}

#[test]
fn validate_without_a_dts_exits_one_not_zero() {
    let result = run(&["validate", &fixture("instance/basic.xml")]);
    assert_eq!(result.code, 1);
    assert!(result.stdout.contains("1 finding(s)"), "{}", result.stdout);
    assert!(
        result.stderr.contains("xbrl-validate:unsupported"),
        "{}",
        result.stderr
    );
}

#[test]
fn render_with_a_local_dts_exits_zero() {
    let result = run(&[
        "render",
        &fixture("instance/basic.xml"),
        "--dts",
        &fixture("taxonomy"),
        "--entry",
        "example.xsd",
    ]);
    assert_eq!(result.code, 0);
    assert!(result.stdout.contains("Assets"), "{}", result.stdout);
    assert_eq!(result.stderr, "");
}

#[test]
fn no_arguments_exits_two_with_usage() {
    let result = run(&[]);
    assert_eq!(result.code, 2);
    assert_eq!(result.stdout, "");
    assert!(result.stderr.contains("Usage: xbrl <COMMAND>"));
}

#[test]
fn json_output_is_a_single_line_envelope() {
    let result = run(&["--json", "parse", &fixture("instance/basic.xml")]);
    assert_eq!(result.code, 0);
    assert_eq!(result.stderr, "");
    assert!(
        result
            .stdout
            .starts_with("{\"schema\":\"xbrl-sdk/cli-result/v1\"")
    );
    assert!(result.stdout.ends_with("}\n"));
    assert_eq!(result.stdout.lines().count(), 1);
}
