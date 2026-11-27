//! Fixture smoke: local files only (`fixtures/instance/basic.xml` and
//! `fixtures/taxonomy/*`) are loaded into the typed model, rendered, and compared
//! with the checked-in goldens in `fixtures/render/`.
//!
//! Regenerate goldens (then review the diff by hand) with
//! `XBRL_RENDER_BLESS=1 cargo test -p xbrl-render --test smoke`.

use std::fs::File;
use std::path::{Path, PathBuf};

use xbrl_render::{
    RenderInput, RenderOptions, build_reports, render_csv, render_html, render_text,
};
use xbrl_taxonomy::dts::DtsBuilder;
use xbrl_xml::instance::parse_instance;
use xbrl_xml::limits::Limits;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn check(name: &str, actual: &str) {
    let path = fixtures().join("render").join(name);
    if std::env::var_os("XBRL_RENDER_BLESS").is_some() {
        std::fs::write(&path, actual).unwrap();
    }
    let expected = std::fs::read_to_string(&path).unwrap();
    assert_eq!(expected, actual, "golden mismatch: {name}");
}

#[test]
fn example_taxonomy_and_instance_match_goldens() {
    let instance = parse_instance(
        File::open(fixtures().join("instance/basic.xml")).unwrap(),
        Limits::default(),
    )
    .unwrap();
    let dts = DtsBuilder::new(fixtures().join("taxonomy"))
        .unwrap()
        .discover("example.xsd")
        .unwrap();
    let reports = build_reports(
        &RenderInput {
            instance: &instance,
            relationships: dts.relationships(),
            labels: dts.all_labels(),
            concepts: dts.concepts(),
        },
        &RenderOptions::default(),
    );
    assert!(!reports.is_empty());
    check("example.txt", &render_text(&reports));
    check("example.html", &render_html(&reports));
    check("example.csv", &render_csv(&reports[0]));
}
