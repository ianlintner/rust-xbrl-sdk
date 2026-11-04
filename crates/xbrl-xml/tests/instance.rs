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
    let UnitMeasures::Simple(measures) = &instance.units[0].measures else {
        panic!("simple unit")
    };
    assert_eq!(measures[0].to_prefixed(), "iso:USD");
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
    assert_eq!(count, 66);
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
fn element_count_limit_rejects_many_small_nodes() {
    let xml = include_str!("../../../fixtures/instance/basic.xml");
    let exact = Limits {
        max_instance_elements: 25,
        ..Limits::default()
    };
    let under = Limits {
        max_instance_elements: 3,
        ..Limits::default()
    };
    let _ = parse_instance(xml.as_bytes(), exact);
    let err = parse_instance(xml.as_bytes(), under).unwrap_err();
    assert_eq!(err.rule().as_str(), xbrl_xml::instance::LIMIT_ELEMENTS);
}

#[test]
fn element_count_limit_is_inclusive_at_n_and_rejects_n_plus_one() {
    let xml = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance" xmlns:link="http://www.xbrl.org/2003/linkbase" xmlns:xlink="http://www.w3.org/1999/xlink"><link:schemaRef xlink:type="simple" xlink:href="a.xsd"/></xbrl>"#;
    let at_n = Limits {
        max_instance_elements: 2,
        ..Limits::default()
    };
    let below_n = Limits {
        max_instance_elements: 1,
        ..Limits::default()
    };
    assert_eq!(
        parse_instance(xml.as_bytes(), at_n)
            .unwrap()
            .schema_refs
            .len(),
        1
    );
    assert_eq!(
        parse_instance(xml.as_bytes(), below_n)
            .unwrap_err()
            .rule()
            .as_str(),
        xbrl_xml::instance::LIMIT_ELEMENTS
    );
}

#[test]
fn shared_namespace_scope_supports_many_measures_without_per_node_clones() {
    let mut xml = String::from(
        "<xbrl xmlns=\"http://www.xbrl.org/2003/instance\" xmlns:link=\"http://www.xbrl.org/2003/linkbase\" xmlns:xlink=\"http://www.w3.org/1999/xlink\"",
    );
    for i in 0..400 {
        xml.push_str(&format!(" xmlns:p{i}=\"urn:measure{i}\""));
    }
    xml.push_str("><link:schemaRef xlink:type=\"simple\" xlink:href=\"a.xsd\"/><unit id=\"u\">");
    for _ in 0..5000 {
        xml.push_str("<measure>p1:A</measure>");
    }
    xml.push_str("</unit></xbrl>");
    let parsed = parse_instance(xml.as_bytes(), Limits::default()).unwrap();
    let UnitMeasures::Simple(names) = &parsed.units[0].measures else {
        panic!("expected product")
    };
    assert_eq!(names.len(), 5000);
    assert_eq!(names[0].namespace(), "urn:measure1");
    assert_eq!(names[4999].namespace(), "urn:measure1");
}

#[test]
fn xsi_nil_and_precision_collapse_xml_whitespace() {
    let xml = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance" xmlns:link="http://www.xbrl.org/2003/linkbase" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:ex="urn:ex"><link:schemaRef xlink:type="simple" xlink:href="a.xsd"/><ex:A contextRef="c" xsi:nil=" true "/><ex:B contextRef="c" precision=" -0 ">1</ex:B></xbrl>"#;
    let parsed = parse_instance(xml.as_bytes(), Limits::default()).unwrap();
    assert_eq!(parsed.facts[0].value, None);
    assert_eq!(parsed.facts[1].accuracy, Some(Accuracy::Precision(0)));
}

#[test]
fn same_day_duration_and_empty_identifier_are_syntactically_accepted() {
    let xml = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance" xmlns:link="http://www.xbrl.org/2003/linkbase" xmlns:xlink="http://www.w3.org/1999/xlink"><link:schemaRef xlink:type="simple" xlink:href="a.xsd"/><context id="c"><entity><identifier scheme="urn:entity"/></entity><period><startDate>2025-01-05</startDate><endDate>2025-01-05</endDate></period></context></xbrl>"#;
    let parsed = parse_instance(xml.as_bytes(), Limits::default()).unwrap();
    assert_eq!(parsed.contexts[0].entity.identifier, "");
    assert_eq!(
        parsed.contexts[0].period,
        Period::Duration {
            start: "2025-01-05".into(),
            end: "2025-01-05".into()
        }
    );
}

#[test]
fn instance_rule_ids_are_stable_and_valid() {
    use xbrl_core::diagnostic::RuleId;
    for id in [
        xbrl_xml::instance::INVALID_ROOT,
        xbrl_xml::instance::INVALID_STRUCTURE,
        xbrl_xml::instance::INVALID_MEASURE,
        xbrl_xml::instance::INVALID_PERIOD,
        xbrl_xml::instance::INVALID_ACCURACY,
        xbrl_xml::instance::UNSUPPORTED,
        xbrl_xml::instance::LIMIT_ELEMENTS,
    ] {
        assert!(RuleId::new(id).is_ok(), "invalid rule id {id}");
    }
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

#[test]
fn namespace_uris_are_shared_not_copied_per_node() {
    // B1': a long root-declared URI must be one allocation however many facts,
    // measures and segment children reference it.
    let uri = format!("http://example.com/{}", "a".repeat(60_000));
    let mut xml = format!(
        "<xbrli:xbrl xmlns:xbrli=\"http://www.xbrl.org/2003/instance\" \
         xmlns:iso=\"http://www.xbrl.org/2003/iso4217\" xmlns:e=\"{uri}\" \
         xmlns:link=\"http://www.xbrl.org/2003/linkbase\" xmlns:xlink=\"http://www.w3.org/1999/xlink\">\
         <link:schemaRef xlink:type=\"simple\" xlink:href=\"t.xsd\"/>"
    );
    xml.push_str(
        "<xbrli:context id=\"c\"><xbrli:entity><xbrli:identifier scheme=\"http://s\">A\
         </xbrli:identifier><xbrli:segment><e:x/><e:x/></xbrli:segment></xbrli:entity>\
         <xbrli:period><xbrli:instant>2025-12-31</xbrli:instant></xbrli:period></xbrli:context>\
         <xbrli:unit id=\"u\"><xbrli:measure>iso:USD</xbrli:measure></xbrli:unit>",
    );
    for _ in 0..50 {
        xml.push_str("<e:f contextRef=\"c\" unitRef=\"u\" decimals=\"0\">1</e:f>");
    }
    xml.push_str("</xbrli:xbrl>");
    let instance = parse_instance(xml.as_bytes(), Limits::default()).unwrap();
    assert_eq!(instance.facts.len(), 50);
    let first = instance.facts[0].concept.namespace().as_ptr();
    assert!(
        instance
            .facts
            .iter()
            .all(|f| f.concept.namespace().as_ptr() == first),
        "fact namespaces must share one allocation"
    );
    let seg = instance.contexts[0].entity.segment.as_ref().unwrap();
    assert_eq!(seg[0].name.namespace().as_ptr(), first);
    assert_eq!(seg[1].name.namespace().as_ptr(), first);
}

fn minimal(extra_root_child: &str, fact: &str) -> String {
    format!(
        "<x:xbrl xmlns:x=\"http://www.xbrl.org/2003/instance\" \
         xmlns:link=\"http://www.xbrl.org/2003/linkbase\" \
         xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
         xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" \
         xmlns:e=\"http://example.com/e\">\
         <link:schemaRef xlink:type=\"simple\" xlink:href=\"t.xsd\" id=\"dup\"/>\
         <x:context id=\"c\"><x:entity><x:identifier scheme=\"http://s\">A</x:identifier></x:entity>\
         <x:period><x:instant>2025-12-31</x:instant></x:period></x:context>\
         {extra_root_child}{fact}</x:xbrl>"
    )
}

#[test]
fn schema_ref_id_participates_in_id_uniqueness() {
    let xml = minimal("", "<e:f contextRef=\"c\" id=\"dup\">1</e:f>");
    let err = parse_instance(xml.as_bytes(), Limits::default()).unwrap_err();
    assert!(err.message().contains("duplicate XML ID"), "{err:?}");
}

#[test]
fn nil_fact_rejects_whitespace_content() {
    let xml = minimal("", "<e:f contextRef=\"c\" xsi:nil=\"true\"> </e:f>");
    let err = parse_instance(xml.as_bytes(), Limits::default()).unwrap_err();
    assert!(err.message().contains("nil item"), "{err:?}");
    let ok = minimal("", "<e:f contextRef=\"c\" xsi:nil=\"true\"/>");
    assert!(parse_instance(ok.as_bytes(), Limits::default()).is_ok());
}

#[test]
fn id_uses_xml_ncname_character_classes() {
    for (id, ok) in [
        ("a\u{B7}b", true),
        ("\u{6E2C}x", true),
        ("a\u{B2}", false),
        ("\u{B7}a", false),
        ("1a", false),
    ] {
        let xml = minimal("", &format!("<e:f contextRef=\"c\" id=\"{id}\">1</e:f>"));
        let result = parse_instance(xml.as_bytes(), Limits::default());
        assert_eq!(result.is_ok(), ok, "id {id:?}: {result:?}");
    }
}

#[test]
fn schema_ref_href_whitespace_is_collapsed_for_validation_only() {
    let xml = minimal("", "").replace("xlink:href=\"t.xsd\"", "xlink:href=\" t.xsd \"");
    let instance = parse_instance(xml.as_bytes(), Limits::default()).unwrap();
    assert_eq!(instance.schema_refs[0].href, " t.xsd ");
}

#[test]
fn extension_nodes_keep_in_scope_namespace_declarations() {
    let xml = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance" xmlns:link="http://www.xbrl.org/2003/linkbase" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:r="urn:root">
<link:schemaRef xlink:type="simple" xlink:href="x.xsd"/>
<context id="c"><entity><identifier scheme="urn:s">A</identifier><segment><ex:m xmlns:ex="urn:ex" xmlns:t="urn:types" xsi:type="t:T"><ex:n xmlns:t="urn:inner" xsi:type="t:U"/></ex:m></segment></entity><period><forever/></period></context></xbrl>"#;
    let instance = parse_instance(xml.as_bytes(), Limits::default()).unwrap();
    let node = &instance.contexts[0].entity.segment.as_ref().unwrap()[0];
    let lookup = |n: &xbrl_core::instance::ExtensionNode, p: &str| {
        n.namespaces.resolve(Some(p)).map(|uri| uri.to_string())
    };
    assert_eq!(lookup(node, "t").as_deref(), Some("urn:types"));
    assert_eq!(lookup(node, "r").as_deref(), Some("urn:root"));
    assert_eq!(lookup(&node.children[0], "t").as_deref(), Some("urn:inner"));
}

#[test]
fn namespace_uri_is_validated_once_per_declaration_not_per_node() {
    let uri = format!("urn:{}", "u".repeat(100_000));
    let facts: String = (0..2_000)
        .map(|_| "<big:f contextRef=\"c\">1</big:f>")
        .collect();
    let xml = minimal("", &facts).replacen("<x:xbrl ", &format!("<x:xbrl xmlns:big=\"{uri}\" "), 1);
    xbrl_core::qname::reset_namespace_validations();
    parse_instance(xml.as_bytes(), Limits::default()).expect("parses");
    let count = xbrl_core::qname::namespace_validations();
    assert!(
        count < 20,
        "namespace validated {count} times for 2000 nodes"
    );
}

#[test]
fn duplicate_attributes_with_equal_but_distinct_namespace_allocations_are_rejected() {
    let fact = "<e:f contextRef=\"c\" xmlns:a=\"urn:same\" xmlns:b=\"urn:same\" a:x=\"1\" b:x=\"2\">1</e:f>";
    let xml = minimal("", fact);
    let err = parse_instance(xml.as_bytes(), Limits::default()).unwrap_err();
    assert!(err.message().contains("same name"), "{err:?}");
}

#[test]
fn extension_scope_is_shared_not_cloned_per_node() {
    // Non-timing bound: 300 root prefixes, 1000 sibling extension nodes that
    // declare nothing. Structural sharing means every child reuses one scope
    // allocation, so memory does not grow with nodes x prefixes.
    let decls: String = (0..300)
        .map(|i| format!(" xmlns:p{i}=\"urn:p{i}\""))
        .collect();
    let kids: String = (0..1_000).map(|_| "<e:m/>").collect();
    let xml = format!(
        r#"<x:xbrl xmlns:x="http://www.xbrl.org/2003/instance" xmlns:l="http://www.xbrl.org/2003/linkbase" xmlns:xl="http://www.w3.org/1999/xlink" xmlns:e="urn:e"{decls}>
<l:schemaRef xl:type="simple" xl:href="x.xsd"/>
<x:context id="c"><x:entity><x:identifier scheme="urn:s">A</x:identifier><x:segment>{kids}</x:segment></x:entity><x:period><x:forever/></x:period></x:context></x:xbrl>"#
    );
    let instance = parse_instance(xml.as_bytes(), Limits::default()).unwrap();
    let seg = instance.contexts[0].entity.segment.as_ref().unwrap();
    assert_eq!(seg.len(), 1_000);
    assert!(
        seg.iter().all(|n| n.namespaces.ptr_eq(&seg[0].namespaces)),
        "siblings must share one scope allocation"
    );
    assert_eq!(
        seg[0].namespaces.resolve(Some("p299")).map(|u| &**u),
        Some("urn:p299")
    );
}

#[test]
fn redeclaring_the_same_namespace_uri_is_validated_once() {
    let uri = format!("urn:{}", "u".repeat(100_000));
    let facts: String = (0..500)
        .map(|_| format!("<big:f xmlns:big=\"{uri}\" contextRef=\"c\">1</big:f>"))
        .collect();
    let xml = minimal("", &facts);
    xbrl_core::qname::reset_namespace_validations();
    parse_instance(xml.as_bytes(), Limits::default()).expect("parses");
    let count = xbrl_core::qname::namespace_validations();
    assert!(
        count < 20,
        "namespace validated {count} times for 500 redeclarations"
    );
}
