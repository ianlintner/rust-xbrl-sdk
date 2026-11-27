//! Command-line front end (`xbrl`) for the Rust XBRL SDK.
//!
//! This is the machine-facing boundary of the SDK (PLAN.md milestone 7). Three
//! subcommands are implemented, all local-only:
//!
//! - `xbrl parse <INSTANCE>` extracts the typed XBRL 2.1 instance model.
//! - `xbrl validate <INSTANCE> [--dts DIR --entry REF]` runs the rule engine in
//!   `xbrl-validate`; without a DTS it emits an `xbrl-validate:unsupported`
//!   finding instead of claiming a clean report.
//! - `xbrl render <INSTANCE> --dts DIR --entry REF [--format FMT]` builds
//!   presentation reports from the typed model and renders text/HTML/CSV.
//!
//! Every subcommand accepts global `--json`, producing the versioned envelope
//! published in `docs/cli-result.schema.json`. Diagnostics always use the one
//! [`xbrl_core::diagnostic::Diagnostic`] shape; a failed command never prints
//! a success-looking result. Exit codes are `0` success, `1` findings,
//! `2` usage error and `4` input error.
//!
//! [`run`] captures stdout/stderr and an [`ExitStatus`] instead of exiting, so
//! the binary in `main.rs` stays a thin wrapper and every branch is testable.

pub mod args;
mod commands;
pub mod json;

use std::fmt;

use args::Command;
use json::{DiagnosticView, Envelope, InputInfo, Status};

pub use args::GLOBAL_HELP as USAGE;

/// Binary name as installed.
pub const PROGRAM: &str = "xbrl";

/// Implemented subcommands, in help order.
pub const COMMANDS: [&str; 3] = ["parse", "validate", "render"];

/// Process exit status of a CLI invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitStatus {
    /// The command completed with no error-severity diagnostic.
    Success,
    /// The command completed but emitted an error-severity finding.
    Findings,
    /// The command line was not usable.
    Usage,
    /// The instance could not be read.
    Input,
}

impl ExitStatus {
    /// Stable process exit code for this status.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Success => 0,
            Self::Findings => 1,
            Self::Usage => 2,
            Self::Input => 4,
        }
    }
}

impl fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Success => f.write_str("success"),
            Self::Findings => f.write_str("findings"),
            Self::Usage => f.write_str("usage error"),
            Self::Input => f.write_str("input error"),
        }
    }
}

impl From<ExitStatus> for Status {
    fn from(value: ExitStatus) -> Self {
        match value {
            ExitStatus::Success => Self::Ok,
            ExitStatus::Findings => Self::Findings,
            ExitStatus::Usage | ExitStatus::Input => Self::Error,
        }
    }
}

/// Captured result of a CLI invocation: what to print and how to exit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

impl Outcome {
    /// How the process should exit.
    #[must_use]
    pub fn status(&self) -> ExitStatus {
        self.status
    }

    /// Text for standard output (empty when there is nothing to report).
    #[must_use]
    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    /// Text for standard error (empty when there is nothing to report).
    #[must_use]
    pub fn stderr(&self) -> &str {
        &self.stderr
    }

    fn success(stdout: String) -> Self {
        Self {
            status: ExitStatus::Success,
            stdout,
            stderr: String::new(),
        }
    }
}

fn envelope(result: commands::CommandResult) -> Envelope {
    Envelope {
        schema: json::SCHEMA_ID.to_owned(),
        command: result.command.to_owned(),
        status: result.status.into(),
        exit_code: result.status.code(),
        input: InputInfo { path: result.path },
        summary: result.summary,
        diagnostics: result
            .diagnostics
            .iter()
            .map(DiagnosticView::from)
            .collect(),
        diagnostics_truncated: result.diagnostics_truncated,
        error: result.error,
        output: result.output,
    }
}

fn finish(result: commands::CommandResult, json_mode: bool) -> Outcome {
    let status = result.status;
    if json_mode {
        return Outcome {
            status,
            stdout: format!("{}\n", envelope(result).to_json()),
            stderr: String::new(),
        };
    }
    let stderr = if let Some(error) = result.error {
        format!("error: {error}\n")
    } else {
        result
            .diagnostics
            .iter()
            .map(|finding| format!("{finding}\n"))
            .collect()
    };
    Outcome {
        status,
        stdout: result.human_stdout,
        stderr,
    }
}

fn command_hint(raw: &[String]) -> &str {
    raw.iter()
        .find(|argument| COMMANDS.contains(&argument.as_str()))
        .map_or("unknown", String::as_str)
}

fn usage(message: &str, json_mode: bool, command: &str) -> Outcome {
    if json_mode {
        let envelope = Envelope {
            schema: json::SCHEMA_ID.to_owned(),
            command: command.to_owned(),
            status: Status::Error,
            exit_code: ExitStatus::Usage.code(),
            input: InputInfo {
                path: String::new(),
            },
            summary: json::Summary::Error {
                kind: "usage".to_owned(),
            },
            diagnostics: Vec::new(),
            diagnostics_truncated: false,
            error: Some(message.to_owned()),
            output: None,
        };
        return Outcome {
            status: ExitStatus::Usage,
            stdout: format!("{}\n", envelope.to_json()),
            stderr: String::new(),
        };
    }
    Outcome {
        status: ExitStatus::Usage,
        stdout: String::new(),
        stderr: format!("error: {message}\n\n{}\n", args::help(None)),
    }
}

/// Runs the CLI with the arguments that follow the program name.
///
/// The returned [`Outcome`] is always fully formed: this function never exits
/// the process or panics, so callers can test every branch. It does read local
/// files for the three subcommands; there is no network access.
///
/// # Examples
///
/// ```
/// use xbrl_cli::{ExitStatus, run};
///
/// let outcome = run(["--version"]);
/// assert_eq!(outcome.status(), ExitStatus::Success);
/// assert!(outcome.stdout().starts_with("xbrl "));
///
/// let outcome = run(["parse"]);
/// assert_eq!(outcome.status(), ExitStatus::Usage);
/// ```
#[must_use]
pub fn run<I, S>(args: I) -> Outcome
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let raw: Vec<String> = args
        .into_iter()
        .map(|argument| argument.as_ref().to_owned())
        .collect();
    let json_mode = raw.iter().any(|argument| argument == "--json");

    match args::parse_args(raw.iter().map(String::as_str)) {
        Ok(Command::Help(topic)) => Outcome::success(args::help(topic)),
        Ok(Command::Version) => {
            Outcome::success(format!("{PROGRAM} {}\n", env!("CARGO_PKG_VERSION")))
        }
        Ok(Command::Parse(command)) => finish(commands::parse(command), json_mode),
        Ok(Command::Validate(command)) => finish(commands::validate(command), json_mode),
        Ok(Command::Render(command)) => finish(commands::render(command), json_mode),
        Err(message) => usage(&message, json_mode, command_hint(&raw)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_are_distinct_and_stable() {
        assert_eq!(ExitStatus::Success.code(), 0);
        assert_eq!(ExitStatus::Findings.code(), 1);
        assert_eq!(ExitStatus::Usage.code(), 2);
        assert_eq!(ExitStatus::Input.code(), 4);
        assert_eq!(ExitStatus::Findings.to_string(), "findings");
    }

    #[test]
    fn no_arguments_is_a_usage_error() {
        let outcome = run(std::iter::empty::<&str>());
        assert_eq!(outcome.status(), ExitStatus::Usage);
        assert_eq!(outcome.stdout(), "");
        assert!(
            outcome.stderr().contains("missing command"),
            "{}",
            outcome.stderr()
        );
        assert!(outcome.stderr().contains("Usage: xbrl <COMMAND>"));
    }

    #[test]
    fn help_prints_usage_on_stdout() {
        for flag in ["-h", "--help"] {
            let outcome = run([flag]);
            assert_eq!(outcome.status(), ExitStatus::Success);
            assert!(outcome.stdout().contains("Usage: xbrl <COMMAND>"));
            assert!(outcome.stdout().contains("parse"));
            assert_eq!(outcome.stderr(), "");
        }
    }

    #[test]
    fn version_prints_program_and_package_version() {
        for flag in ["-V", "--version"] {
            let outcome = run([flag]);
            assert_eq!(outcome.status(), ExitStatus::Success);
            assert_eq!(
                outcome.stdout(),
                format!("{PROGRAM} {}\n", env!("CARGO_PKG_VERSION"))
            );
            assert_eq!(outcome.stderr(), "");
        }
    }
}
