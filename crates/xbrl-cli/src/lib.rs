//! Command-line front end (`xbrl`) for the Rust XBRL SDK.
//!
//! Status: **scaffold baseline** (PLAN.md milestone 1; the full CLI is milestone
//! 7). Only `--help` and `--version` do real work. The planned `parse`,
//! `validate` and `render` subcommands are *recognised and refused* with exit
//! code 3, because the SDK cannot yet parse a document: printing a
//! success-looking result the crate did not compute would be worse than an
//! honest "not implemented".
//!
//! When milestone 7 lands, `parse` will drive `xbrl-xml`/`xbrl-core`, `validate`
//! will drive [`xbrl_validate`] and `render` will drive [`xbrl_render`], with
//! diagnostics printed as [`xbrl_core::diagnostic::Diagnostic`] lines and JSON.
//!
//! [`run`] is pure: it takes the arguments after the program name and returns
//! captured output plus an [`ExitStatus`] instead of touching the process, which
//! is what the unit tests exercise. The binary in `main.rs` is a thin wrapper.

use std::fmt;

/// Binary name as installed.
pub const PROGRAM: &str = "xbrl";

/// Subcommands that PLAN.md reserves but this baseline has not implemented.
pub const PLANNED_COMMANDS: [&str; 3] = ["parse", "validate", "render"];

/// Help text printed by `--help` (and after a usage error).
pub const USAGE: &str = "\
xbrl — Rust XBRL SDK command-line front end (scaffold baseline)

Usage: xbrl <COMMAND>

Commands:
  parse      Parse an XBRL instance into the typed model (not implemented)
  validate   Run validation rules over an instance (not implemented)
  render     Render report tables to text/HTML/CSV (not implemented)

Options:
  -h, --help       Print this help
  -V, --version    Print the version";

/// Exit status of a CLI invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitStatus {
    /// The requested action completed.
    Success,
    /// The command line was not usable (no command, unknown command or option).
    Usage,
    /// A recognised command that this baseline has not implemented.
    Unimplemented,
}

impl ExitStatus {
    /// Process exit code: `0` for success, `2` for a usage error, `3` for a
    /// recognised but unimplemented command.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Success => 0,
            Self::Usage => 2,
            Self::Unimplemented => 3,
        }
    }
}

impl fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Success => f.write_str("success"),
            Self::Usage => f.write_str("usage error"),
            Self::Unimplemented => f.write_str("not implemented"),
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

    fn usage(stderr: String) -> Self {
        Self {
            status: ExitStatus::Usage,
            stdout: String::new(),
            stderr,
        }
    }

    fn unimplemented(stderr: String) -> Self {
        Self {
            status: ExitStatus::Unimplemented,
            stdout: String::new(),
            stderr,
        }
    }
}

/// Runs the CLI with the arguments that follow the program name.
///
/// The returned [`Outcome`] is always fully formed: this function performs no
/// I/O, exits no process and never panics, so callers can test every branch.
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
/// let outcome = run(["parse", "instance.xbrl"]);
/// assert_eq!(outcome.status(), ExitStatus::Unimplemented);
///
/// let outcome = run(std::iter::empty::<&str>());
/// assert_eq!(outcome.status(), ExitStatus::Usage);
/// ```
#[must_use]
pub fn run<I, S>(args: I) -> Outcome
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut args = args.into_iter();
    let Some(first) = args.next() else {
        return Outcome::usage(format!("error: no command given\n\n{USAGE}\n"));
    };
    let command = first.as_ref();

    match command {
        "-h" | "--help" => Outcome::success(format!("{USAGE}\n")),
        "-V" | "--version" => {
            Outcome::success(format!("{PROGRAM} {}\n", env!("CARGO_PKG_VERSION")))
        }
        command if command.starts_with('-') => Outcome::usage(format!(
            "error: unrecognized option `{command}`\n\n{USAGE}\n"
        )),
        command if PLANNED_COMMANDS.contains(&command) => Outcome::unimplemented(format!(
            "error: `{command}` is not implemented in this scaffold baseline; \
                 see PLAN.md milestone 7\n"
        )),
        command => Outcome::usage(format!(
            "error: unrecognized command `{command}`\n\n{USAGE}\n"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_are_distinct_and_stable() {
        assert_eq!(ExitStatus::Success.code(), 0);
        assert_eq!(ExitStatus::Usage.code(), 2);
        assert_eq!(ExitStatus::Unimplemented.code(), 3);
        assert_eq!(ExitStatus::Unimplemented.to_string(), "not implemented");
    }

    #[test]
    fn no_arguments_is_a_usage_error() {
        let outcome = run(std::iter::empty::<&str>());
        assert_eq!(outcome.status(), ExitStatus::Usage);
        assert_eq!(outcome.stdout(), "");
        assert!(outcome.stderr().contains("no command given"));
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

    #[test]
    fn planned_commands_fail_closed_without_claiming_success() {
        for command in PLANNED_COMMANDS {
            let outcome = run([command, "instance.xbrl"]);
            assert_eq!(outcome.status(), ExitStatus::Unimplemented);
            assert_eq!(
                outcome.stdout(),
                "",
                "must not print a result it did not compute"
            );
            assert!(
                outcome.stderr().contains("not implemented"),
                "got {:?}",
                outcome.stderr()
            );
            assert!(outcome.stderr().contains(command));
        }
    }

    #[test]
    fn unknown_option_and_command_are_usage_errors() {
        let outcome = run(["--frobnicate"]);
        assert_eq!(outcome.status(), ExitStatus::Usage);
        assert!(
            outcome
                .stderr()
                .contains("unrecognized option `--frobnicate`")
        );

        let outcome = run(["frobnicate"]);
        assert_eq!(outcome.status(), ExitStatus::Usage);
        assert!(
            outcome
                .stderr()
                .contains("unrecognized command `frobnicate`")
        );
        assert!(outcome.stderr().contains("Usage: xbrl <COMMAND>"));
    }
}
