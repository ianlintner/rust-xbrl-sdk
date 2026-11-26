//! Data-driven tests for the authored local DTS fixtures.
//!
//! Provenance and spec versions: `fixtures/taxonomy/README.md`.

use std::path::{Path, PathBuf};

use xbrl_core::qname::QName;
use xbrl_taxonomy::dts::{Dts, DtsBuilder, DtsLimits, LinkbaseKind};
use xbrl_taxonomy::rules;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/taxonomy")
        .canonicalize()
        .expect("fixture directory")
}

fn load_subdir(subdir: &str, entry: &str) -> Result<Dts, xbrl_core::diagnostic::Diagnostic> {
    let root = fixtures().join(subdir);
    let mut builder = DtsBuilder::new(&root).expect("document root");
    builder.limits(DtsLimits {
        max_documents: 8,
        ..DtsLimits::default()
    });
    builder.discover(entry)
}

#[test]
fn discovers_local_dts_deterministically() {
    let dts = load_subdir(".", "example.xsd").expect("valid fixture DTS");
    // xs:include and all three linkbaseRefs are discovered, one document each.
    assert_eq!(dts.documents().len(), 5, "{}", dts.documents().len());
    let mut paths: Vec<_> = dts
        .documents()
        .iter()
        .map(|d| d.path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    paths.sort();
    assert_eq!(
        paths,
        vec![
            "example-calculation.xml",
            "example-label.xml",
            "example-presentation.xml",
            "example-types.xsd",
            "example.xsd"
        ]
    );
    // The entry schema is always the first document, in discovery order.
    assert_eq!(
        dts.documents()[0]
            .path
            .file_name()
            .unwrap()
            .to_string_lossy(),
        "example.xsd"
    );
}

#[test]
fn extracts_concepts_and_types_from_schemas() {
    let dts = load_subdir(".", "example.xsd").expect("valid fixture DTS");
    let namespace = "http://example.com/taxonomy";
    assert_eq!(dts.concepts().len(), 4);
    let revenue = dts
        .concept(&QName::new(None, namespace, "Revenue").expect("qname"))
        .expect("Revenue concept");
    assert!(!revenue.is_abstract);
    assert!(revenue.nillable);
    assert_eq!(revenue.period_type.as_deref(), Some("duration"));
    assert_eq!(revenue.type_name.local_name(), "MonetaryType");
    assert_eq!(revenue.id.as_deref(), Some("Revenue"));
    let totals = dts
        .concept(&QName::new(None, namespace, "Totals").expect("qname"))
        .expect("Totals concept");
    assert!(totals.is_abstract);
    assert!(!totals.nillable);
    let types: Vec<_> = dts.types().iter().map(|t| t.name.local_name()).collect();
    assert_eq!(types, vec!["ApprovalType", "MonetaryType"]);
    let monetary = &dts.types()[1];
    assert_eq!(monetary.derivation.as_deref(), Some("extension"));
    assert_eq!(monetary.base.as_deref(), Some("xs:decimal"));
}

#[test]
fn resolves_labels_by_concept_role_and_language() {
    let dts = load_subdir(".", "example.xsd").expect("valid fixture DTS");
    let namespace = "http://example.com/taxonomy";
    let revenue = QName::new(None, namespace, "Revenue").expect("qname");
    let labels = dts.labels(&revenue);
    assert_eq!(labels.len(), 2, "{labels:?}");
    assert_eq!(labels[0].role, "http://www.xbrl.org/2003/role/label");
    assert_eq!(labels[0].text, "Revenue & Sales");
    assert_eq!(labels[0].lang.as_deref(), Some("en"));
    assert_eq!(labels[1].role, "http://www.xbrl.org/2003/role/totalLabel");
    assert_eq!(labels[1].text, "Total revenue");
    assert_eq!(dts.preferred_label(&revenue, None), Some("Revenue & Sales"));
}

#[test]
fn traverses_presentation_graph_deterministically() {
    let dts = load_subdir(".", "example.xsd").expect("valid fixture DTS");
    let namespace = "http://example.com/taxonomy";
    let totals = QName::new(None, namespace, "Totals").expect("qname");
    let roots = dts.presentation_roots(LinkbaseKind::Presentation);
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].concept, totals);
    let children = dts.presentation_children(LinkbaseKind::Presentation, &totals);
    assert_eq!(children.len(), 2);
    assert_eq!(children[0].concept.local_name(), "Revenue");
    assert_eq!(children[1].concept.local_name(), "Assets");
    // order is numeric; preferredLabel is preserved for the renderer.
    assert_eq!(children[0].order, 1.0);
    assert_eq!(
        children[0].preferred_label.as_deref(),
        Some("http://www.xbrl.org/2003/role/totalLabel")
    );
    assert_eq!(
        dts.preferred_label(&children[0].concept, children[0].preferred_label.as_deref()),
        Some("Total revenue")
    );
}

#[test]
fn traverses_calculation_graph_with_weights() {
    let dts = load_subdir(".", "example.xsd").expect("valid fixture DTS");
    let namespace = "http://example.com/taxonomy";
    let assets = QName::new(None, namespace, "Assets").expect("qname");
    let summands = dts.calculation_summands(&assets);
    assert_eq!(summands.len(), 2);
    // Sorted by numeric order, not by source position (Revenue was written first).
    assert_eq!(summands[0].concept.local_name(), "Expenses");
    assert_eq!(summands[0].weight, -1.0);
    assert_eq!(summands[0].order, 1.0);
    assert_eq!(summands[1].concept.local_name(), "Revenue");
    assert_eq!(summands[1].weight, 1.0);
}

#[test]
fn missing_include_is_a_typed_error() {
    let error = load_subdir("broken", "missing-include.xsd").expect_err("missing include");
    assert_eq!(error.rule().as_str(), rules::REFERENCE_NOT_FOUND);
}

#[test]
fn cyclic_include_is_detected() {
    let error = load_subdir("broken", "cycle.xsd").expect_err("cycle");
    assert_eq!(error.rule().as_str(), rules::DISCOVERY_CYCLE);
}

#[test]
fn remote_reference_is_blocked_without_network() {
    let error = load_subdir("broken", "remote.xsd").expect_err("remote blocked");
    assert_eq!(
        error.rule().as_str(),
        xbrl_xml::rules::RESOLVER_REMOTE_BLOCKED
    );
}

#[test]
fn unresolved_locator_is_a_typed_error() {
    let error = load_subdir("broken", "unresolved.xsd").expect_err("unresolved locator");
    assert_eq!(error.rule().as_str(), rules::LOCATOR_UNRESOLVED);
}

#[test]
fn duplicate_ids_are_rejected() {
    let error = load_subdir("broken", "duplicate.xsd").expect_err("duplicate id");
    assert_eq!(error.rule().as_str(), rules::DUPLICATE_ID);
}

#[test]
fn presentation_cycles_are_detected() {
    let error = load_subdir("broken", "presentation-cycle.xsd")
        .expect_err("presentation graph must be acyclic");
    assert_eq!(error.rule().as_str(), rules::GRAPH_CYCLE);
}

#[test]
fn document_limit_is_enforced() {
    let root = fixtures();
    let mut builder = DtsBuilder::new(&root).expect("document root");
    builder.limits(DtsLimits {
        max_documents: 2,
        ..DtsLimits::default()
    });
    let error = builder.discover("example.xsd").expect_err("limit");
    assert_eq!(error.rule().as_str(), rules::LIMIT_DOCUMENTS);
}

#[test]
fn discovery_is_repeatable() {
    let first = load_subdir(".", "example.xsd").expect("first");
    let second = load_subdir(".", "example.xsd").expect("second");
    assert_eq!(first.concepts(), second.concepts());
    assert_eq!(first.documents(), second.documents());
    assert_eq!(
        first.presentation_children(
            LinkbaseKind::Presentation,
            &QName::new(None, "http://example.com/taxonomy", "Totals").expect("qname")
        ),
        second.presentation_children(
            LinkbaseKind::Presentation,
            &QName::new(None, "http://example.com/taxonomy", "Totals").expect("qname")
        )
    );
}
