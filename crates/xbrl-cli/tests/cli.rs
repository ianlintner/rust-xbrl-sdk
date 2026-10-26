//! End-to-end smoke test for the installed `xbrl` binary.
//!
//! The library unit tests cover [`xbrl_cli::run`] in isolation; this test proves
//! the binary actually writes the captured output to the right stream and exits
//! with the documented code, which is what a user or an agent harness sees.

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
fn help_exits_zero_and_lists_the_planned_commands() {
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
fn planned_commands_exit_three_with_an_honest_message() {
    let result = run(&["validate", "instance.xbrl"]);
    assert_eq!(result.code, 3);
    assert_eq!(result.stdout, "");
    assert!(result.stderr.contains("not implemented"));
}

#[test]
fn no_arguments_exits_two_with_usage() {
    let result = run(&[]);
    assert_eq!(result.code, 2);
    assert_eq!(result.stdout, "");
    assert!(result.stderr.contains("Usage: xbrl <COMMAND>"));
}
