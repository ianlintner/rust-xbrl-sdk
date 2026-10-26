//! Fixture-driven behavior tests for [`xbrl_core::qname`].
//!
//! The cases live in `fixtures/qname/lexical.tsv` (provenance: authored for this
//! repository from XML Namespaces 1.0 / XML Schema Part 2 and XBRL 2.1 §3). This
//! test is the single place that turns those rows into assertions, so the fixture
//! and the parser cannot drift apart silently.

use xbrl_core::qname::{QName, QNameError};

const FIXTURE: &str = include_str!("../../../fixtures/qname/lexical.tsv");

#[derive(Debug)]
struct Case {
    id: String,
    input: String,
    namespace: String,
    local_name: String,
    error_kind: String,
}

impl Case {
    fn expects_error(&self) -> bool {
        self.error_kind != "-"
    }
}

fn cases() -> Vec<Case> {
    FIXTURE
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            assert_eq!(
                fields.len(),
                5,
                "fixture row must have 5 tab-separated fields: {line:?}"
            );
            Case {
                id: fields[0].to_owned(),
                input: fields[1].to_owned(),
                namespace: fields[2].to_owned(),
                local_name: fields[3].to_owned(),
                error_kind: fields[4].to_owned(),
            }
        })
        .collect()
}

/// Maps an error to the snake_case kind used by the fixture file.
fn error_kind(error: &QNameError) -> &'static str {
    match error {
        QNameError::EmptyNamespace => "empty_namespace",
        QNameError::InvalidNamespace { .. } => "invalid_namespace",
        QNameError::EmptyLocalName => "empty_local_name",
        QNameError::InvalidLocalName { .. } => "invalid_local_name",
        QNameError::InvalidPrefix { .. } => "invalid_prefix",
        QNameError::MalformedClarkNotation { .. } => "malformed_clark",
    }
}

#[test]
fn fixture_file_is_non_trivial_and_has_unique_ids() {
    let cases = cases();
    assert!(
        cases.len() >= 10,
        "expected a meaningful fixture set, found {} rows",
        cases.len()
    );
    assert!(
        cases.iter().any(Case::expects_error),
        "fixture must cover error cases"
    );
    assert!(
        cases.iter().any(|case| !case.expects_error()),
        "fixture must cover success cases"
    );

    let mut ids: Vec<&str> = cases.iter().map(|case| case.id.as_str()).collect();
    ids.sort_unstable();
    let before = ids.len();
    ids.dedup();
    assert_eq!(before, ids.len(), "fixture case ids must be unique");
}

#[test]
fn every_fixture_case_matches_the_parser() {
    let mut failures = Vec::new();

    for case in cases() {
        match (QName::parse_clark(&case.input), case.expects_error()) {
            (Err(error), true) => {
                if error_kind(&error) != case.error_kind {
                    failures.push(format!(
                        "{}: expected error kind {:?}, got {:?} ({error})",
                        case.id,
                        case.error_kind,
                        error_kind(&error)
                    ));
                }
            }
            (Err(error), false) => {
                failures.push(format!(
                    "{}: expected a parsed name, got error {error}",
                    case.id
                ));
            }
            (Ok(name), true) => {
                failures.push(format!(
                    "{}: expected error kind {:?}, got parsed name {}",
                    case.id, case.error_kind, name
                ));
            }
            (Ok(name), false) => {
                if name.namespace() != case.namespace || name.local_name() != case.local_name {
                    failures.push(format!(
                        "{}: expected {{{}}}{}, got {name}",
                        case.id, case.namespace, case.local_name
                    ));
                }
                if name.prefix().is_some() {
                    failures.push(format!(
                        "{}: Clark notation must not carry a prefix",
                        case.id
                    ));
                }
                if QName::parse_clark(&name.to_clark()).as_ref() != Ok(&name) {
                    failures.push(format!("{}: {} is not round-trip stable", case.id, name));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} fixture cases failed:\n{}",
        failures.len(),
        cases().len(),
        failures.join("\n")
    );
}
