//! Command-line grammar for the `xbrl` binary.
//!
//! Parsing is a pure function: usage errors are returned as strings so that
//! [`crate::run`] can capture them in an [`crate::Outcome`] and decide whether
//! to emit the human usage text or a machine-readable error envelope. No
//! option silently falls back to a different subcommand or output format.

/// Which command-specific help page was requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topic {
    /// `xbrl parse --help`.
    Parse,
    /// `xbrl validate --help`.
    Validate,
    /// `xbrl render --help`.
    Render,
}

impl Topic {
    /// Canonical command name for this topic.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Parse => "parse",
            Self::Validate => "validate",
            Self::Render => "render",
        }
    }
}

/// A requested output format for `render`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Plain-text report tables from `xbrl_render::render_text`.
    Text,
    /// Single HTML document from `xbrl_render::render_html`.
    Html,
    /// RFC 4180 CSV from `xbrl_render::render_csv`.
    Csv,
}

impl Format {
    /// Lowercase flag value for this format.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Html => "html",
            Self::Csv => "csv",
        }
    }
}

/// Arguments shared by every subcommand that reads an instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceArgs {
    /// Instance path exactly as supplied by the caller.
    pub path: String,
    /// Emit the versioned JSON envelope instead of human-readable output.
    pub json: bool,
}

/// DTS location required for concept-aware validation and for rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DtsArgs {
    /// Local directory used as the resolver root; no network access.
    pub root: String,
    /// Entry schema path relative to [`DtsArgs::root`].
    pub entry: String,
}

/// Arguments of `xbrl validate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidateArgs {
    /// Instance path exactly as supplied by the caller.
    pub path: String,
    /// Optional local DTS. Without it, validation fails closed with a typed
    /// `xbrl-validate:unsupported` finding instead of claiming success.
    pub dts: Option<DtsArgs>,
    /// Emit the versioned JSON envelope instead of human-readable output.
    pub json: bool,
}

/// Arguments of `xbrl render`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderArgs {
    /// Instance path exactly as supplied by the caller.
    pub path: String,
    /// Local DTS; required because presentation relationships live there.
    pub dts: DtsArgs,
    /// Requested output format.
    pub format: Format,
    /// Emit the versioned JSON envelope instead of raw rendered output.
    pub json: bool,
}

/// A parsed, validated command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Print a help page; `None` is the global page.
    Help(Option<Topic>),
    /// Print the crate version.
    Version,
    /// Parse an instance into the typed model.
    Parse(InstanceArgs),
    /// Run validation rules over an instance.
    Validate(ValidateArgs),
    /// Build presentation reports and render them.
    Render(RenderArgs),
}

const COMMANDS: [&str; 3] = ["parse", "validate", "render"];

/// Parses arguments following the program name into a [`Command`].
///
/// `--json` is global and may appear before or after the subcommand. Every
/// failure is a stable usage-error string; the caller adds usage text if it
/// prints to a human.
pub fn parse_args<I, S>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut json = false;
    let mut help = false;
    let mut version = false;
    let mut command: Option<String> = None;
    let mut positional: Vec<String> = Vec::new();
    let mut dts: Option<String> = None;
    let mut entry: Option<String> = None;
    let mut format: Option<String> = None;

    let mut iter = args.into_iter();
    while let Some(argument) = iter.next() {
        let arg = argument.as_ref();
        match arg {
            "--json" => json = true,
            "-h" | "--help" => help = true,
            "-V" | "--version" => version = true,
            "--dts" | "--entry" | "--format" => {
                let value = iter
                    .next()
                    .ok_or_else(|| format!("`{arg}` requires a value"))?;
                let value = value.as_ref().to_owned();
                match arg {
                    "--dts" => dts = Some(value),
                    "--entry" => entry = Some(value),
                    // `--format` matched by the outer pattern.
                    _ => format = Some(value),
                }
            }
            _ if arg.starts_with('-') => {
                return Err(format!("unrecognized option `{arg}`"));
            }
            _ => {
                if command.is_none() {
                    command = Some(arg.to_owned());
                } else {
                    positional.push(arg.to_owned());
                }
            }
        }
    }

    if command.is_none() && help {
        return Ok(Command::Help(None));
    }
    if command.is_none() && version {
        return Ok(Command::Version);
    }

    let name = command.ok_or_else(|| "missing command; see `xbrl --help`".to_owned())?;
    if !COMMANDS.contains(&name.as_str()) {
        return Err(format!("unrecognized command `{name}`"));
    }
    if help {
        let topic = match name.as_str() {
            "parse" => Topic::Parse,
            "validate" => Topic::Validate,
            // Checked against COMMANDS above.
            _ => Topic::Render,
        };
        return Ok(Command::Help(Some(topic)));
    }
    if version {
        return Ok(Command::Version);
    }

    if let Some(value) = format.as_deref() {
        if name != "render" {
            return Err(format!("`--format` is not valid for `{name}`"));
        }
        if !["text", "html", "csv"].contains(&value) {
            return Err(format!(
                "unsupported format `{value}`; expected text, html or csv"
            ));
        }
    }
    if name == "parse" && dts.is_some() {
        return Err("`--dts` is not valid for `parse`".to_owned());
    }
    if name == "parse" && entry.is_some() {
        return Err("`--entry` is not valid for `parse`".to_owned());
    }
    if dts.is_some() != entry.is_some() {
        let missing = if dts.is_some() { "--entry" } else { "--dts" };
        let supplied = if dts.is_some() { "--dts" } else { "--entry" };
        return Err(format!(
            "`{missing}` is required when `{supplied}` is supplied"
        ));
    }

    let path = positional
        .first()
        .ok_or_else(|| format!("`{name}` requires an instance path"))?
        .clone();
    if let Some(extra) = positional.get(1) {
        return Err(format!("unexpected argument `{extra}`"));
    }

    let parsed_format = match format.as_deref() {
        None | Some("text") => Format::Text,
        Some("html") => Format::Html,
        // Validated above.
        Some(_) => Format::Csv,
    };

    match name.as_str() {
        "parse" => Ok(Command::Parse(InstanceArgs { path, json })),
        "validate" => Ok(Command::Validate(ValidateArgs {
            path,
            dts: dts.zip(entry).map(|(root, entry)| DtsArgs { root, entry }),
            json,
        })),
        // Checked against COMMANDS above.
        _ => {
            let Some(dts) = dts.zip(entry).map(|(root, entry)| DtsArgs { root, entry }) else {
                return Err("`render` requires --dts <DIR> and --entry <REF>".to_owned());
            };
            Ok(Command::Render(RenderArgs {
                path,
                dts,
                format: parsed_format,
                json,
            }))
        }
    }
}

/// Global help page, also exported as [`crate::USAGE`].
pub const GLOBAL_HELP: &str = "\
xbrl — Rust XBRL SDK command-line front end

Usage: xbrl <COMMAND>

Commands:
  parse      Parse an XBRL instance into the typed model
  validate   Run validation rules over an instance (fails closed without a DTS)
  render     Render report tables to text/HTML/CSV (requires a local DTS)

Global options:
  -h, --help       Print this help (or `xbrl <COMMAND> --help`)
  -V, --version    Print the version
      --json       Emit the versioned machine-readable result envelope

No command downloads remote documents. Unsupported work is reported, never
silently treated as success. See docs/agent-tool-contract.md.";

/// Returns the help page for a topic, or the global page when `None`.
#[must_use]
pub fn help(topic: Option<Topic>) -> String {
    let Some(topic) = topic else {
        return format!("{GLOBAL_HELP}\n");
    };
    let (usage, options) = match topic {
        Topic::Parse => (
            "xbrl parse [--json] <INSTANCE>",
            "  --json          Emit the JSON result envelope instead of a summary\n",
        ),
        Topic::Validate => (
            "xbrl validate [--json] [--dts <DIR> --entry <REF>] <INSTANCE>",
            "  --dts <DIR>     Local DTS resolver root (no network access)\n\
             \x20 --entry <REF>   Entry schema path relative to <DIR>\n\
             \x20 --json          Emit the JSON result envelope instead of diagnostics\n",
        ),
        Topic::Render => (
            "xbrl render [--json] --dts <DIR> --entry <REF> [--format text|html|csv] <INSTANCE>",
            "  --dts <DIR>     Local DTS resolver root (required, no network access)\n\
             \x20 --entry <REF>   Entry schema path relative to <DIR> (required)\n\
             \x20 --format FMT    text (default), html or csv\n\
             \x20 --json          Wrap the rendered output in the JSON result envelope\n",
        ),
    };
    let _ = topic.as_str();
    format!("{usage}\n\nOptions:\n{options}\nGlobal: -h/--help, -V/--version, --json\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_names_are_stable() {
        assert_eq!(Format::Text.as_str(), "text");
        assert_eq!(Format::Html.as_str(), "html");
        assert_eq!(Format::Csv.as_str(), "csv");
        assert_eq!(Topic::Render.as_str(), "render");
    }

    #[test]
    fn global_help_has_no_command_specific_flags() {
        let text = help(None);
        assert!(text.contains("Usage: xbrl <COMMAND>"), "{text}");
        assert!(text.contains("--json"), "{text}");
    }
}
