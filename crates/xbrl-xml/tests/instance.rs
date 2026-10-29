//! Instance extraction tests. Authored fixture: fixtures/instance/README.md.
use xbrl_core::instance::{Accuracy, Period, UnitMeasures};
use xbrl_xml::instance::parse_instance;
use xbrl_xml::limits::Limits;

#[test]
fn extracts_basic_instance() {
    let xml = include_str!("../../../fixtures/instance/basic.xml");
    let instance = parse_instance(xml.as_bytes(), Limits::default()).unwrap();
    assert_eq!(instance.schema_refs.len(), 1);
    assert_eq!(instance.schema_refs[0].href, "taxonomies/example.xsd");
    assert_eq!(instance.contexts.len(), 2);
    assert_eq!(instance.contexts[0].entity.identifier, "ACME");
    assert_eq!(
        instance.contexts[0].entity.scheme,
        "http://example.com/entity"
    );
    assert_eq!(
        instance.contexts[0].period,
        Period::Instant("2025-12-31".into())
    );
    assert_eq!(
        instance.contexts[1].period,
        Period::Duration {
            start: "2025-01-01".into(),
            end: "2025-12-31".into()
        }
    );
    assert_eq!(instance.units.len(), 2);
    assert_eq!(
        instance.units[0].measures,
        UnitMeasures::Simple(vec![
            xbrl_core::qname::QName::new(Some("iso"), "http://www.xbrl.org/2003/iso4217", "USD")
                .unwrap()
        ])
    );
    assert!(
        matches!(&instance.units[1].measures, UnitMeasures::Divide { numerator, denominator } if numerator[0].local_name() == "USD" && denominator[0].local_name() == "shares")
    );
    assert_eq!(instance.facts.len(), 4);
    assert_eq!(instance.facts[0].concept.local_name(), "Revenue");
    assert_eq!(instance.facts[0].context_ref, "c2");
    assert_eq!(instance.facts[0].unit_ref.as_deref(), Some("usd"));
    assert_eq!(instance.facts[0].accuracy, Some(Accuracy::Decimals(-3)));
    assert_eq!(instance.facts[0].value.as_deref(), Some("123000"));
    assert_eq!(instance.facts[1].accuracy, Some(Accuracy::Precision(4)));
    assert_eq!(instance.facts[2].value.as_deref(), Some("Hello & goodbye"));
    assert_eq!(instance.facts[3].value, None);
}

#[test]
fn preserves_context_extension_content() {
    let xml = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance" xmlns:link="http://www.xbrl.org/2003/linkbase" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:ex="urn:extension">
<link:schemaRef xlink:type="simple" xlink:href="extension.xsd"/>
<context id="c"><entity><identifier scheme="urn:entity"> A  B </identifier><segment><ex:region ex:code="W">West</ex:region></segment></entity><period><forever/></period><scenario><ex:adjusted/></scenario></context></xbrl>"#;
    let instance = parse_instance(xml.as_bytes(), Limits::default()).unwrap();
    let context = &instance.contexts[0];
    assert_eq!(context.entity.identifier, "A B");
    assert_eq!(context.period, Period::Forever);
    assert_eq!(
        context.entity.segment.as_ref().unwrap()[0]
            .name
            .local_name(),
        "region"
    );
    assert_eq!(context.entity.segment.as_ref().unwrap()[0].text, "West");
    assert_eq!(
        context.scenario.as_ref().unwrap()[0].name.namespace(),
        "urn:extension"
    );
}

#[test]
fn rejects_reversed_datetime_duration() {
    let xml = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance" xmlns:link="http://www.xbrl.org/2003/linkbase" xmlns:xlink="http://www.w3.org/1999/xlink"><link:schemaRef xlink:type="simple" xlink:href="a.xsd"/><context id="c"><entity><identifier scheme="urn:entity">E</identifier></entity><period><startDate>2025-06-02T12:00:00Z</startDate><endDate>2025-06-01T12:00:00Z</endDate></period></context></xbrl>"#;
    let err = parse_instance(xml.as_bytes(), Limits::default()).unwrap_err();
    assert_eq!(err.rule().as_str(), xbrl_xml::instance::INVALID_PERIOD);
}

#[test]
fn authored_negative_fixtures_report_specific_rule() {
    let rows = include_str!("../../../fixtures/instance/invalid.tsv");
    let mut count = 0;
    for line in rows
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 3, "malformed fixture row: {line}");
        let xml = format!(
            r#"<xbrl xmlns="http://www.xbrl.org/2003/instance" xmlns:link="http://www.xbrl.org/2003/linkbase" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:iso="http://www.xbrl.org/2003/iso4217" xmlns:ex="urn:example" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><link:schemaRef xlink:type="simple" xlink:href="a.xsd"/>{}</xbrl>"#,
            fields[1]
        );
        let diagnostic = parse_instance(xml.as_bytes(), Limits::default()).expect_err(fields[0]);
        assert_eq!(
            diagnostic.rule().as_str(),
            fields[2],
            "{}: {diagnostic}",
            fields[0]
        );
        assert!(
            diagnostic.span().is_some(),
            "{} must locate error",
            fields[0]
        );
        count += 1;
    }
    assert_eq!(count, 25);
}

#[test]
fn predefined_xml_prefix_is_resolved_in_measure_content() {
    let xml = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance" xmlns:link="http://www.xbrl.org/2003/linkbase" xmlns:xlink="http://www.w3.org/1999/xlink"><link:schemaRef xlink:type="simple" xlink:href="a.xsd"/><unit id="u"><measure>xml:lang</measure></unit></xbrl>"#;
    let instance = parse_instance(xml.as_bytes(), Limits::default()).unwrap();
    let UnitMeasures::Simple(measures) = &instance.units[0].measures else {
        panic!("simple unit expected");
    };
    assert_eq!(
        measures[0].namespace(),
        "http://www.w3.org/XML/1998/namespace"
    );
}

#[test]
fn accuracy_union_collapses_xml_schema_whitespace() {
    let xml = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance" xmlns:link="http://www.xbrl.org/2003/linkbase" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:ex="urn:ex"><link:schemaRef xlink:type="simple" xlink:href="a.xsd"/><ex:Amount contextRef="c" unitRef="u" decimals=" INF ">10</ex:Amount></xbrl>"#;
    let instance = parse_instance(xml.as_bytes(), Limits::default()).unwrap();
    assert_eq!(instance.facts[0].accuracy, Some(Accuracy::Infinite));
}

#[test]
fn accepts_xml_schema_date_with_timezone() {
    let xml = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance" xmlns:link="http://www.xbrl.org/2003/linkbase" xmlns:xlink="http://www.w3.org/1999/xlink"><link:schemaRef xlink:type="simple" xlink:href="a.xsd"/><context id="c"><entity><identifier scheme="urn:e">E</identifier></entity><period><instant>2024-02-29+05:00</instant></period></context></xbrl>"#;
    let parsed = parse_instance(xml.as_bytes(), Limits::default()).unwrap();
    assert_eq!(
        parsed.contexts[0].period,
        Period::Instant("2024-02-29+05:00".into())
    );
}

#[test]
fn reader_limits_apply_to_instance_extraction() {
    let xml = include_str!("../../../fixtures/instance/basic.xml");
    let limits = Limits {
        max_document_bytes: 100,
        ..Limits::default()
    };
    let diagnostic = parse_instance(xml.as_bytes(), limits).unwrap_err();
    assert_eq!(
        diagnostic.rule().as_str(),
        xbrl_xml::rules::LIMIT_DOCUMENT_SIZE
    );
}

#[test]
fn official_suite_extraction_smoke_when_explicitly_supplied() {
    // The upstream archive is All Rights Reserved; it is not vendored. This
    // optional local check does not run in CI and is NOT a conformance claim.
    let Ok(dir) = std::env::var("XBRL_21_SUITE_DIR") else {
        return;
    };
    let base = std::path::Path::new(&dir).join("Common/300-instance");
    for (file, contexts, units, facts) in [
        ("301-01-IdScopeValid.xml", 2, 1, 2),
        ("303-01-PeriodInstantValid.xml", 1, 1, 1),
        ("305-01-DecimalOnlySpecified.xml", 1, 1, 1),
        ("307-01-SchemaRefExample.xml", 0, 0, 0),
    ] {
        let bytes = std::fs::read(base.join(file)).unwrap();
        let instance = parse_instance(bytes.as_slice(), Limits::default()).unwrap();
        assert_eq!(
            (
                instance.contexts.len(),
                instance.units.len(),
                instance.facts.len()
            ),
            (contexts, units, facts),
            "{file}"
        );
    }
    for (file, expected) in [
        (
            "302-10-PeriodDateTimeInvalid.xml",
            xbrl_xml::instance::INVALID_PERIOD,
        ),
        (
            "305-04-BothDecimalAndPrecisionSpecified.xml",
            xbrl_xml::instance::INVALID_ACCURACY,
        ),
    ] {
        let bytes = std::fs::read(base.join(file)).unwrap();
        let err = parse_instance(bytes.as_slice(), Limits::default()).unwrap_err();
        assert_eq!(err.rule().as_str(), expected, "{file}: {err}");
    }
}
