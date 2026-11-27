//! Argument parsing tests for the three implemented subcommands.
//!
//! Usage errors are values, not process exits: `run` captures them so the
//! binary, integration tests and future library callers see identical text.

use xbrl_cli::args::{Command, Format, Topic, parse_args};

fn ok(args: &[&str]) -> Command {
    parse_args(args.iter().copied()).unwrap_or_else(|error| panic!("{args:?}: {error}"))
}

fn err(args: &[&str]) -> String {
    parse_args(args.iter().copied())
        .err()
        .unwrap_or_else(|| panic!("{args:?} unexpectedly parsed"))
}

#[test]
fn global_help_and_version_are_recognized() {
    assert_eq!(ok(&["--help"]), Command::Help(None));
    assert_eq!(ok(&["-h"]), Command::Help(None));
    assert_eq!(ok(&["--version"]), Command::Version);
    assert_eq!(ok(&["-V"]), Command::Version);
}

#[test]
fn command_help_selects_a_topic() {
    assert_eq!(ok(&["parse", "--help"]), Command::Help(Some(Topic::Parse)));
    assert_eq!(
        ok(&["validate", "--help"]),
        Command::Help(Some(Topic::Validate))
    );
    assert_eq!(ok(&["render", "-h"]), Command::Help(Some(Topic::Render)));
}

#[test]
fn parse_takes_one_instance_and_optional_global_json() {
    assert_eq!(
        ok(&["parse", "instance.xbrl"]),
        Command::Parse(xbrl_cli::args::InstanceArgs {
            path: "instance.xbrl".to_owned(),
            json: false,
        })
    );
    assert_eq!(
        ok(&["--json", "parse", "instance.xbrl"]),
        Command::Parse(xbrl_cli::args::InstanceArgs {
            path: "instance.xbrl".to_owned(),
            json: true,
        })
    );
    assert_eq!(
        ok(&["parse", "instance.xbrl", "--json"]),
        Command::Parse(xbrl_cli::args::InstanceArgs {
            path: "instance.xbrl".to_owned(),
            json: true,
        })
    );
}

#[test]
fn validate_accepts_an_optional_dts_pair() {
    assert_eq!(
        ok(&["validate", "instance.xbrl"]),
        Command::Validate(xbrl_cli::args::ValidateArgs {
            path: "instance.xbrl".to_owned(),
            dts: None,
            json: false,
        })
    );
    let parsed = ok(&[
        "--json",
        "validate",
        "instance.xbrl",
        "--dts",
        "fixtures/taxonomy",
        "--entry",
        "example.xsd",
    ]);
    match parsed {
        Command::Validate(args) => {
            assert_eq!(args.path, "instance.xbrl");
            assert_eq!(args.dts.as_ref().unwrap().root, "fixtures/taxonomy");
            assert_eq!(args.dts.as_ref().unwrap().entry, "example.xsd");
            assert!(args.json);
        }
        other => panic!("expected validate, got {other:?}"),
    }
}

#[test]
fn dts_root_and_entry_are_a_pair() {
    assert_eq!(
        err(&["validate", "instance.xbrl", "--dts", "dir"]),
        "`--entry` is required when `--dts` is supplied"
    );
    assert_eq!(
        err(&["validate", "instance.xbrl", "--entry", "example.xsd"]),
        "`--dts` is required when `--entry` is supplied"
    );
}

#[test]
fn render_requires_a_dts_and_accepts_three_formats() {
    assert_eq!(
        err(&["render", "instance.xbrl"]),
        "`render` requires --dts <DIR> and --entry <REF>"
    );
    for (name, expected) in [
        ("text", Format::Text),
        ("html", Format::Html),
        ("csv", Format::Csv),
    ] {
        let command = ok(&[
            "render",
            "instance.xbrl",
            "--dts",
            "dir",
            "--entry",
            "example.xsd",
            "--format",
            name,
        ]);
        match command {
            Command::Render(args) => {
                assert_eq!(args.format, expected);
                assert_eq!(args.dts.entry, "example.xsd");
            }
            other => panic!("expected render, got {other:?}"),
        }
    }
    let default_format = ok(&[
        "render",
        "instance.xbrl",
        "--dts",
        "dir",
        "--entry",
        "e.xsd",
    ]);
    match default_format {
        Command::Render(args) => assert_eq!(args.format, Format::Text),
        other => panic!("expected render, got {other:?}"),
    }
}

#[test]
fn unsupported_format_is_a_usage_error_not_a_silent_fallback() {
    assert_eq!(
        err(&[
            "render",
            "instance.xbrl",
            "--dts",
            "dir",
            "--entry",
            "e.xsd",
            "--format",
            "pdf",
        ]),
        "unsupported format `pdf`; expected text, html or csv"
    );
}

#[test]
fn missing_unknown_and_extra_arguments_are_usage_errors() {
    assert_eq!(err(&[]), "missing command; see `xbrl --help`");
    assert_eq!(err(&["frobnicate"]), "unrecognized command `frobnicate`");
    assert_eq!(err(&["--frobnicate"]), "unrecognized option `--frobnicate`");
    assert_eq!(err(&["parse"]), "`parse` requires an instance path");
    assert_eq!(
        err(&["parse", "a.xbrl", "extra.xbrl"]),
        "unexpected argument `extra.xbrl`"
    );
    assert_eq!(
        err(&["parse", "a.xbrl", "--format"]),
        "`--format` requires a value"
    );
}

#[test]
fn flags_are_rejected_for_commands_that_do_not_support_them() {
    assert_eq!(
        err(&["parse", "a.xbrl", "--dts", "dir"]),
        "`--dts` is not valid for `parse`"
    );
    assert_eq!(
        err(&["parse", "a.xbrl", "--entry", "e.xsd"]),
        "`--entry` is not valid for `parse`"
    );
    assert_eq!(
        err(&["validate", "a.xbrl", "--format", "csv"]),
        "`--format` is not valid for `validate`"
    );
}

#[test]
fn help_texts_document_the_real_flags() {
    let global = xbrl_cli::args::help(None);
    assert!(global.contains("Usage: xbrl <COMMAND>"), "{global}");
    assert!(global.contains("parse"), "{global}");
    assert!(global.contains("validate"), "{global}");
    assert!(global.contains("render"), "{global}");
    let render = xbrl_cli::args::help(Some(Topic::Render));
    assert!(render.contains("--dts"), "{render}");
    assert!(render.contains("--entry"), "{render}");
    assert!(render.contains("--format"), "{render}");
    let validate = xbrl_cli::args::help(Some(Topic::Validate));
    assert!(validate.contains("--dts"), "{validate}");
    assert!(!validate.contains("--format"), "{validate}");
}
