//! Binary entry point for the `xbrl` command.
//!
//! All logic lives in [`xbrl_cli::run`]; this wrapper only moves the captured
//! output to the real streams and turns the [`xbrl_cli::ExitStatus`] into a
//! process exit code.

use std::process::ExitCode;

fn main() -> ExitCode {
    let outcome = xbrl_cli::run(std::env::args().skip(1));

    if !outcome.stdout().is_empty() {
        print!("{}", outcome.stdout());
    }
    if !outcome.stderr().is_empty() {
        eprint!("{}", outcome.stderr());
    }

    ExitCode::from(outcome.status().code())
}
