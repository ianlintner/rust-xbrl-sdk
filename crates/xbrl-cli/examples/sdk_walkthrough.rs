//! Executable SDK walkthrough: parse -> validate -> render, all in-process.
//!
//! Run from the workspace root:
//!
//! ```text
//! cargo run -p xbrl-cli --example sdk_walkthrough
//! ```
//!
//! It reads only the vendored fixtures under `fixtures/`, performs no network
//! access, and exits non-zero if any stage disagrees with what the fixtures
//! are documented to contain, so `cargo test --workspace` can build it and CI
//! can run it as a smoke check.

use std::fs::File;
use std::path::Path;
use std::process::ExitCode;

use xbrl_render::{RenderInput, RenderOptions, build_reports, render_text};
use xbrl_taxonomy::dts::DtsBuilder;
use xbrl_validate::{RuleSet, ValidationLimits, Validator};
use xbrl_xml::instance::parse_instance;
use xbrl_xml::limits::Limits;

fn main() -> ExitCode {
    match walkthrough() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("sdk_walkthrough failed: {message}");
            ExitCode::FAILURE
        }
    }
}

fn walkthrough() -> Result<(), String> {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");

    // 1. Parse. A parse failure is a typed Diagnostic, never a panic.
    let file = File::open(fixtures.join("instance/basic.xml")).map_err(|e| e.to_string())?;
    let instance = parse_instance(file, Limits::default()).map_err(|d| d.to_string())?;
    println!(
        "parsed: {} contexts, {} units, {} facts",
        instance.contexts.len(),
        instance.units.len(),
        instance.facts.len()
    );

    // 2. Discover the local DTS. Nothing is fetched remotely.
    let dts = DtsBuilder::new(fixtures.join("taxonomy"))
        .and_then(|builder| builder.discover("example.xsd"))
        .map_err(|d| d.to_string())?;

    // 3. Validate. Findings are data; a clean report lists which rules ran.
    let report = Validator::new(RuleSet::default(), ValidationLimits::default())
        .validate(&instance, Some(&dts));
    println!(
        "validated: {} finding(s), rules executed: {}",
        report.findings().len(),
        report.executed().join(", ")
    );
    for finding in report.findings() {
        println!("  {finding}");
    }
    if !report.is_clean() {
        return Err("fixture was expected to validate cleanly".to_owned());
    }

    // 4. Render. Rendering consumes the typed model; it never re-parses XML.
    let reports = build_reports(
        &RenderInput {
            instance: &instance,
            relationships: dts.relationships(),
            labels: dts.all_labels(),
            concepts: dts.concepts(),
        },
        &RenderOptions::default(),
    );
    println!("rendered {} report(s):\n", reports.len());
    print!("{}", render_text(&reports));
    Ok(())
}
