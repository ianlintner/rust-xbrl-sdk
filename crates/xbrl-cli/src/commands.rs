//! Command execution over the typed SDK crates.
//!
//! Each command runs locally: it reads an instance with `xbrl-xml`, optionally
//! discovers a DTS with `xbrl-taxonomy`, then calls `xbrl-validate` or
//! `xbrl-render`. No validation decision is made here and rendering never
//! re-parses XML. Failures are typed diagnostics (parse/DTS/validation) or
//! explicit input errors; neither is ever presented as a successful result.

use xbrl_core::diagnostic::{Diagnostic, Severity};
use xbrl_core::instance::Instance;
use xbrl_render::{
    RenderInput, RenderOptions, build_reports, render_csv, render_html, render_text,
};
use xbrl_taxonomy::dts::{Dts, DtsBuilder};
use xbrl_validate::{RuleSet, ValidationLimits, Validator};
use xbrl_xml::instance::parse_instance;
use xbrl_xml::limits::Limits;

use crate::ExitStatus;
use crate::args::{DtsArgs, Format, InstanceArgs, RenderArgs, ValidateArgs};
use crate::json::{OutputView, ParseSummary, Summary, ValidateSummary};

/// Everything one command needs in order to be rendered as human or JSON text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandResult {
    /// Stable subcommand name for the JSON envelope.
    pub command: &'static str,
    /// Instance path exactly as supplied by the caller.
    pub path: String,
    /// Process exit status.
    pub status: ExitStatus,
    /// Command-specific body.
    pub summary: Summary,
    /// Typed findings in deterministic order.
    pub diagnostics: Vec<Diagnostic>,
    /// Whether the producing component dropped findings.
    pub diagnostics_truncated: bool,
    /// Human-readable reason for [`ExitStatus::Input`].
    pub error: Option<String>,
    /// Rendered payload for `render`.
    pub output: Option<OutputView>,
    /// Human-readable summary (empty when the command failed).
    pub human_stdout: String,
}

impl CommandResult {
    fn success(
        command: &'static str,
        path: String,
        summary: Summary,
        human_stdout: String,
        output: Option<OutputView>,
    ) -> Self {
        Self {
            command,
            path,
            status: ExitStatus::Success,
            summary,
            diagnostics: Vec::new(),
            diagnostics_truncated: false,
            error: None,
            output,
            human_stdout,
        }
    }

    fn findings(
        command: &'static str,
        path: String,
        summary: Summary,
        diagnostics: Vec<Diagnostic>,
        truncated: bool,
        human_stdout: String,
    ) -> Self {
        let status = if diagnostics
            .iter()
            .any(|finding| finding.severity() == Severity::Error)
        {
            ExitStatus::Findings
        } else {
            ExitStatus::Success
        };
        Self {
            command,
            path,
            status,
            summary,
            diagnostics,
            diagnostics_truncated: truncated,
            error: None,
            output: None,
            human_stdout,
        }
    }

    fn input_error(command: &'static str, path: String, error: String) -> Self {
        Self {
            command,
            path,
            status: ExitStatus::Input,
            summary: Summary::Error {
                kind: "input".to_owned(),
            },
            diagnostics: Vec::new(),
            diagnostics_truncated: false,
            error: Some(error),
            output: None,
            human_stdout: String::new(),
        }
    }

    fn discovery_error(
        command: &'static str,
        path: String,
        summary: Summary,
        finding: Diagnostic,
    ) -> Self {
        Self::findings(command, path, summary, vec![finding], false, String::new())
    }
}

fn read_instance(command: &'static str, path: &str) -> Result<Instance, Box<CommandResult>> {
    let file = std::fs::File::open(path).map_err(|error| {
        Box::new(CommandResult::input_error(
            command,
            path.to_owned(),
            format!("cannot read instance `{path}`: {error}"),
        ))
    })?;
    parse_instance(file, Limits::default()).map_err(|finding| {
        Box::new(CommandResult::findings(
            command,
            path.to_owned(),
            Summary::Error {
                kind: "parse".to_owned(),
            },
            vec![finding],
            false,
            String::new(),
        ))
    })
}

fn discover(command: &'static str, path: &str, dts: &DtsArgs) -> Result<Dts, Box<CommandResult>> {
    let builder = DtsBuilder::new(&dts.root).map_err(|finding| {
        Box::new(CommandResult::discovery_error(
            command,
            path.to_owned(),
            empty_summary(command, Some(&dts.entry)),
            finding,
        ))
    })?;
    builder.discover(&dts.entry).map_err(|finding| {
        Box::new(CommandResult::discovery_error(
            command,
            path.to_owned(),
            empty_summary(command, Some(&dts.entry)),
            finding,
        ))
    })
}

fn empty_summary(command: &'static str, dts_entry: Option<&str>) -> Summary {
    match command {
        "validate" => Summary::Validate(ValidateSummary {
            rules_executed: Vec::new(),
            findings: 0,
            errors: 0,
            warnings: 0,
            info: 0,
            findings_truncated: false,
            dts_entry: dts_entry.map(ToOwned::to_owned),
        }),
        // Checked by the callers of `discover`.
        _ => Summary::Render {
            reports: 0,
            dts_entry: dts_entry.unwrap_or_default().to_owned(),
        },
    }
}

/// Runs `xbrl parse <INSTANCE>`.
pub(crate) fn parse(args: InstanceArgs) -> CommandResult {
    let path = args.path.clone();
    let instance = match read_instance("parse", &path) {
        Ok(instance) => instance,
        Err(result) => return *result,
    };
    let summary = ParseSummary {
        schema_refs: instance.schema_refs.len(),
        contexts: instance.contexts.len(),
        units: instance.units.len(),
        facts: instance.facts.len(),
    };
    let human = format!(
        "parsed {path}\nschemaRefs: {}\ncontexts: {}\nunits: {}\nfacts: {}\n",
        summary.schema_refs, summary.contexts, summary.units, summary.facts
    );
    CommandResult::success("parse", path, Summary::Parse(summary), human, None)
}

/// Runs `xbrl validate <INSTANCE> [--dts DIR --entry REF]`.
pub(crate) fn validate(args: ValidateArgs) -> CommandResult {
    let path = args.path.clone();
    let instance = match read_instance("validate", &path) {
        Ok(instance) => instance,
        Err(result) => return *result,
    };
    let dts = match args.dts.as_ref() {
        Some(dts) => match discover("validate", &path, dts) {
            Ok(dts) => Some(dts),
            Err(result) => return *result,
        },
        None => None,
    };
    let report = Validator::new(RuleSet::default(), ValidationLimits::default())
        .validate(&instance, dts.as_ref());
    let summary = ValidateSummary {
        rules_executed: report
            .executed()
            .iter()
            .map(|rule| (*rule).to_owned())
            .collect(),
        findings: report.findings().len(),
        errors: report.error_count(),
        warnings: report
            .findings()
            .iter()
            .filter(|finding| finding.severity() == Severity::Warning)
            .count(),
        info: report
            .findings()
            .iter()
            .filter(|finding| finding.severity() == Severity::Info)
            .count(),
        findings_truncated: report.findings_truncated(),
        dts_entry: args.dts.as_ref().map(|dts| dts.entry.clone()),
    };
    let human = format!(
        "validated {path}: {} finding(s) ({} error(s), {} warning(s), {} info), rules: {}\n",
        summary.findings,
        summary.errors,
        summary.warnings,
        summary.info,
        summary.rules_executed.join(", ")
    );
    CommandResult::findings(
        "validate",
        path,
        Summary::Validate(summary),
        report.findings().to_vec(),
        report.findings_truncated(),
        human,
    )
}

/// Runs `xbrl render <INSTANCE> --dts DIR --entry REF [--format FMT]`.
pub(crate) fn render(args: RenderArgs) -> CommandResult {
    let path = args.path.clone();
    let instance = match read_instance("render", &path) {
        Ok(instance) => instance,
        Err(result) => return *result,
    };
    let dts = match discover("render", &path, &args.dts) {
        Ok(dts) => dts,
        Err(result) => return *result,
    };
    let reports = build_reports(
        &RenderInput {
            instance: &instance,
            relationships: dts.relationships(),
            labels: dts.all_labels(),
            concepts: dts.concepts(),
        },
        &RenderOptions::default(),
    );
    let text = match args.format {
        Format::Text => render_text(&reports),
        Format::Html => render_html(&reports),
        Format::Csv => reports
            .iter()
            .map(render_csv)
            .collect::<Vec<_>>()
            .join("\n"),
    };
    let summary = Summary::Render {
        reports: reports.len(),
        dts_entry: args.dts.entry.clone(),
    };
    CommandResult::success(
        "render",
        path,
        summary,
        text.clone(),
        Some(OutputView {
            format: args.format.as_str().to_owned(),
            text,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_drives_the_exit_status() {
        let finding = Diagnostic::new(
            xbrl_core::diagnostic::RuleId::new("xbrl-core:test").unwrap(),
            Severity::Warning,
            "warning only",
            None,
        );
        let result = CommandResult::findings(
            "validate",
            "x.xbrl".to_owned(),
            Summary::Error {
                kind: "usage".to_owned(),
            },
            vec![finding],
            false,
            String::new(),
        );
        assert_eq!(result.status, ExitStatus::Success);
        assert_eq!(result.diagnostics.len(), 1);
    }
}
