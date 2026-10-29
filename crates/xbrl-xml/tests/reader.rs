//! Behavior tests for the bounded, namespace-aware streaming reader.
//!
//! Expectations derive from XML 1.0 (5th ed., 2008-11-26) and Namespaces in XML
//! 1.0 (3rd ed., 2009-12-08); each test names the clause it exercises. Failure
//! tests assert a specific stable rule id, not merely "an error occurred".

use xbrl_xml::limits::Limits;
use xbrl_xml::reader::{XmlEvent, XmlReader};
use xbrl_xml::rules;

const XBRLI: &str = "http://www.xbrl.org/2003/instance";
const XLINK: &str = "http://www.w3.org/1999/xlink";
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

fn read_all(xml: &str, limits: Limits) -> Result<Vec<XmlEvent>, xbrl_core::diagnostic::Diagnostic> {
    let mut reader = XmlReader::new(xml.as_bytes(), limits);
    let mut events = Vec::new();
    while let Some(event) = reader.next_event()? {
        events.push(event);
    }
    Ok(events)
}

fn ok(xml: &str) -> Vec<XmlEvent> {
    read_all(xml, Limits::default()).expect("document should be well-formed")
}

fn rule_of(xml: &str, limits: Limits) -> String {
    read_all(xml, limits)
        .expect_err("document should be rejected")
        .rule()
        .as_str()
        .to_owned()
}

fn rule(xml: &str) -> String {
    rule_of(xml, Limits::default())
}

fn starts(events: &[XmlEvent]) -> Vec<&xbrl_xml::reader::StartElement> {
    events
        .iter()
        .filter_map(|e| match e {
            XmlEvent::Start(s) => Some(s),
            _ => None,
        })
        .collect()
}

// ---- namespaces (Namespaces in XML 1.0 §6) -------------------------------

#[test]
fn resolves_prefixed_and_default_namespaces() {
    let events = ok(
        r#"<xbrli:xbrl xmlns:xbrli="http://www.xbrl.org/2003/instance" xmlns="urn:default">
        <item/>
    </xbrli:xbrl>"#,
    );
    let s = starts(&events);
    assert_eq!(s[0].name.namespace(), Some(XBRLI));
    assert_eq!(s[0].name.local_name(), "xbrl");
    assert_eq!(s[0].name.prefix(), Some("xbrli"));
    assert_eq!(s[1].name.namespace(), Some("urn:default"));
    assert_eq!(s[1].name.prefix(), None);
}

#[test]
fn namespace_equality_ignores_prefix_choice() {
    // Two documents with different prefixes bind the same namespace + local name.
    let a = ok(r#"<a:r xmlns:a="urn:x"/>"#);
    let b = ok(r#"<b:r xmlns:b="urn:x"/>"#);
    assert_eq!(
        starts(&a)[0].name.to_qname().unwrap().to_clark(),
        starts(&b)[0].name.to_qname().unwrap().to_clark()
    );
}

#[test]
fn unprefixed_attributes_have_no_namespace_even_with_a_default_namespace() {
    // Namespaces §6.2: the default namespace does not apply to attributes.
    let events = ok(r#"<r xmlns="urn:d" id="1" xmlns:p="urn:p" p:id="2"/>"#);
    let attrs = &starts(&events)[0].attributes;
    assert_eq!(attrs.len(), 2);
    assert_eq!(attrs[0].name.namespace(), None);
    assert_eq!(attrs[0].value, "1");
    assert_eq!(attrs[1].name.namespace(), Some("urn:p"));
    assert_eq!(attrs[1].value, "2");
}

#[test]
fn xmlns_declarations_are_reported_separately_from_attributes() {
    let events = ok(r#"<r xmlns="urn:d" xmlns:p="urn:p" a="1"/>"#);
    let start = starts(&events)[0].clone();
    assert_eq!(start.attributes.len(), 1);
    assert_eq!(start.namespaces.len(), 2);
    assert_eq!(start.namespaces[0].prefix(), None);
    assert_eq!(start.namespaces[0].uri(), "urn:d");
    assert_eq!(start.namespaces[1].prefix(), Some("p"));
}

#[test]
fn inner_declarations_shadow_and_then_restore_outer_bindings() {
    let events = ok(r#"<p:a xmlns:p="urn:outer"><p:b xmlns:p="urn:inner"/><p:c/></p:a>"#);
    let s = starts(&events);
    assert_eq!(s[0].name.namespace(), Some("urn:outer"));
    assert_eq!(s[1].name.namespace(), Some("urn:inner"));
    assert_eq!(s[2].name.namespace(), Some("urn:outer"));
}

#[test]
fn default_namespace_can_be_undeclared() {
    // Namespaces §6.2: xmlns="" returns to "no namespace".
    let events = ok(r#"<r xmlns="urn:d"><c xmlns=""/></r>"#);
    let s = starts(&events);
    assert_eq!(s[0].name.namespace(), Some("urn:d"));
    assert_eq!(s[1].name.namespace(), None);
}

#[test]
fn xml_prefix_is_predefined() {
    let events = ok(r#"<r xml:lang="en"/>"#);
    let attr = &starts(&events)[0].attributes[0];
    assert_eq!(attr.name.namespace(), Some(XML_NS));
    assert_eq!(attr.name.local_name(), "lang");
}

#[test]
fn xlink_attributes_resolve_to_the_xlink_namespace() {
    let events = ok(
        r#"<l xmlns:xlink="http://www.w3.org/1999/xlink" xlink:type="simple" xlink:href="a.xsd#x"/>"#,
    );
    let attrs = &starts(&events)[0].attributes;
    assert_eq!(attrs[0].name.namespace(), Some(XLINK));
    assert_eq!(attrs[1].value, "a.xsd#x");
}

#[test]
fn unbound_element_prefix_is_rejected() {
    assert_eq!(rule("<p:r/>"), rules::UNBOUND_PREFIX);
}

#[test]
fn unbound_attribute_prefix_is_rejected() {
    assert_eq!(rule(r#"<r p:a="1"/>"#), rules::UNBOUND_PREFIX);
}

#[test]
fn prefix_binding_does_not_leak_out_of_its_element() {
    assert_eq!(
        rule(r#"<r><a xmlns:p="urn:p"/><p:b/></r>"#),
        rules::UNBOUND_PREFIX
    );
}

#[test]
fn xmlns_prefix_cannot_be_declared() {
    // Namespaces §3: the prefix xmlns MUST NOT be declared.
    assert_eq!(
        rule(r#"<r xmlns:xmlns="urn:x"/>"#),
        rules::INVALID_NAMESPACE_DECLARATION
    );
}

#[test]
fn xml_prefix_cannot_be_bound_to_another_namespace() {
    assert_eq!(
        rule(r#"<r xmlns:xml="urn:other"/>"#),
        rules::INVALID_NAMESPACE_DECLARATION
    );
    // Binding it to its own namespace is allowed.
    ok(r#"<r xmlns:xml="http://www.w3.org/XML/1998/namespace"/>"#);
}

#[test]
fn xml_namespace_cannot_be_bound_to_another_prefix() {
    assert_eq!(
        rule(r#"<r xmlns:p="http://www.w3.org/XML/1998/namespace"/>"#),
        rules::INVALID_NAMESPACE_DECLARATION
    );
}

#[test]
fn xmlns_namespace_cannot_be_bound_at_all() {
    assert_eq!(
        rule(r#"<r xmlns:p="http://www.w3.org/2000/xmlns/"/>"#),
        rules::INVALID_NAMESPACE_DECLARATION
    );
    assert_eq!(
        rule(r#"<r xmlns="http://www.w3.org/2000/xmlns/"/>"#),
        rules::INVALID_NAMESPACE_DECLARATION
    );
}

#[test]
fn prefix_cannot_be_undeclared_in_xml_1_0() {
    // Namespaces 1.0 §3: xmlns:p="" is not allowed (that is Namespaces 1.1).
    assert_eq!(
        rule(r#"<r xmlns:p=""/>"#),
        rules::INVALID_NAMESPACE_DECLARATION
    );
}

#[test]
fn attributes_with_the_same_expanded_name_are_rejected() {
    // Namespaces §6.3: no two attributes may have identical expanded names,
    // even when their prefixes differ.
    assert_eq!(
        rule(r#"<r xmlns:a="urn:x" xmlns:b="urn:x" a:k="1" b:k="2"/>"#),
        rules::DUPLICATE_ATTRIBUTE
    );
    // Same local name in different namespaces is fine.
    ok(r#"<r xmlns:a="urn:x" xmlns:b="urn:y" a:k="1" b:k="2"/>"#);
}

#[test]
fn duplicate_raw_attributes_are_rejected() {
    assert_eq!(rule(r#"<r a="1" a="2"/>"#), rules::DUPLICATE_ATTRIBUTE);
}

#[test]
fn names_with_more_than_one_colon_are_rejected() {
    assert_eq!(rule(r#"<a:b:c xmlns:a="urn:x"/>"#), rules::INVALID_NAME);
    assert_eq!(rule(r#"<:a/>"#), rules::INVALID_NAME);
}

// ---- events, text, whitespace --------------------------------------------

#[test]
fn empty_element_produces_start_then_end() {
    let events = ok("<r><e/></r>");
    let kinds: Vec<&str> = events
        .iter()
        .map(|e| match e {
            XmlEvent::Start(s) => s.name.local_name(),
            XmlEvent::End(e) => {
                if e.name.local_name() == "e" {
                    "/e"
                } else {
                    "/r"
                }
            }
            XmlEvent::Text(_) => "text",
        })
        .collect();
    assert_eq!(kinds, ["r", "e", "/e", "/r"]);
}

#[test]
fn text_cdata_and_predefined_entities_coalesce_into_one_text_event() {
    let events = ok("<r>a &lt; b &amp; c <![CDATA[<d>]]> &#x41;&#66;</r>");
    let texts: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            XmlEvent::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["a < b & c <d> AB"]);
}

#[test]
fn attribute_values_are_unescaped_and_whitespace_normalized() {
    // XML 1.0 §3.3.3: tab/newline in attribute values become spaces.
    let events = ok("<r a=\"x&amp;y\tz\n&#65;\"/>");
    assert_eq!(starts(&events)[0].attributes[0].value, "x&y z A");
}

#[test]
fn line_endings_in_text_are_normalized() {
    // XML 1.0 §2.11: \r\n and bare \r become \n.
    let events = ok("<r>a\r\nb\rc</r>");
    match &events[1] {
        XmlEvent::Text(t) => assert_eq!(t.text, "a\nb\nc"),
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn comments_processing_instructions_and_declaration_are_skipped() {
    let events =
        ok("<?xml version=\"1.0\" encoding=\"UTF-8\"?><!-- c --><?pi data?><r><!-- x --></r>");
    assert_eq!(events.len(), 2);
}

#[test]
fn utf8_bom_is_accepted() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(b"<r/>");
    let mut reader = XmlReader::new(bytes.as_slice(), Limits::default());
    assert!(matches!(reader.next_event(), Ok(Some(XmlEvent::Start(_)))));
}

#[test]
fn spans_are_byte_offsets_into_the_source() {
    let xml = "<r><e/></r>";
    let events = ok(xml);
    let XmlEvent::Start(r) = &events[0] else {
        panic!("start expected")
    };
    assert_eq!((r.span.start(), r.span.end()), (0, 3));
    let XmlEvent::Start(e) = &events[1] else {
        panic!("start expected")
    };
    assert_eq!(&xml[e.span.start()..e.span.end()], "<e/>");
}

#[test]
fn non_ascii_names_and_values_round_trip() {
    let events = ok("<r Ünï=\"日本語\"><Ñame/></r>");
    assert_eq!(starts(&events)[0].attributes[0].name.local_name(), "Ünï");
    assert_eq!(starts(&events)[0].attributes[0].value, "日本語");
}

// ---- well-formedness failures ---------------------------------------------

#[test]
fn mismatched_and_unclosed_tags_are_malformed() {
    assert_eq!(rule("<a></b>"), rules::MALFORMED);
    assert_eq!(rule("<a><b></a>"), rules::MALFORMED);
    assert_eq!(rule("<a>"), rules::MALFORMED);
}

#[test]
fn empty_and_rootless_documents_are_malformed() {
    assert_eq!(rule(""), rules::MALFORMED);
    assert_eq!(rule("<!-- only a comment -->"), rules::MALFORMED);
}

#[test]
fn content_after_the_root_element_is_malformed() {
    assert_eq!(rule("<a/><b/>"), rules::MALFORMED);
    assert_eq!(rule("<a/>text"), rules::MALFORMED);
    // Whitespace, comments and PIs are legal in the epilog.
    ok("<a/>\n<!-- c -->\n<?pi?>\n");
}

#[test]
fn text_before_the_root_element_is_malformed() {
    assert_eq!(rule("text<a/>"), rules::MALFORMED);
}

#[test]
fn lt_in_attribute_value_is_malformed() {
    assert_eq!(rule(r#"<r a="<"/>"#), rules::MALFORMED);
}

#[test]
fn invalid_utf8_is_rejected() {
    let mut reader = XmlReader::new(&b"<r>\xff\xfe</r>"[..], Limits::default());
    let mut err = None;
    loop {
        match reader.next_event() {
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(e) => {
                err = Some(e);
                break;
            }
        }
    }
    assert_eq!(err.expect("error").rule().as_str(), rules::INVALID_ENCODING);
}

#[test]
fn non_utf8_declared_encodings_are_unsupported() {
    assert_eq!(
        rule(r#"<?xml version="1.0" encoding="ISO-8859-1"?><r/>"#),
        rules::UNSUPPORTED_ENCODING
    );
    ok(r#"<?xml version="1.0" encoding="utf-8"?><r/>"#);
}

#[test]
fn xml_1_1_is_unsupported() {
    assert_eq!(
        rule(r#"<?xml version="1.1"?><r/>"#),
        rules::UNSUPPORTED_VERSION
    );
}

#[test]
fn illegal_xml_characters_are_rejected() {
    // &#0; and &#x1; are not XML 1.0 Chars (production [2]).
    assert_eq!(rule("<r>&#0;</r>"), rules::INVALID_CHARACTER_REFERENCE);
    assert_eq!(rule("<r>&#x1;</r>"), rules::INVALID_CHARACTER_REFERENCE);
    assert_eq!(rule("<r>&#xD800;</r>"), rules::INVALID_CHARACTER_REFERENCE);
    assert_eq!(
        rule(r#"<r a="&#x8;"/>"#),
        rules::INVALID_CHARACTER_REFERENCE
    );
    ok("<r>&#x9;&#xA;&#x10000;</r>");
}

// ---- entities / DTD: fail closed ------------------------------------------

#[test]
fn doctype_is_forbidden() {
    assert_eq!(rule("<!DOCTYPE r><r/>"), rules::DOCTYPE_FORBIDDEN);
}

#[test]
fn billion_laughs_is_rejected_before_any_expansion() {
    let xml = r#"<?xml version="1.0"?>
<!DOCTYPE lolz [
 <!ENTITY lol "lol">
 <!ENTITY lol2 "&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;">
 <!ENTITY lol3 "&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;">
 <!ENTITY lol4 "&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;">
]>
<lolz>&lol4;</lolz>"#;
    assert_eq!(rule(xml), rules::DOCTYPE_FORBIDDEN);
}

#[test]
fn external_entities_are_never_read() {
    let xml = r#"<!DOCTYPE r [<!ENTITY x SYSTEM "file:///etc/passwd">]><r>&x;</r>"#;
    assert_eq!(rule(xml), rules::DOCTYPE_FORBIDDEN);
}

#[test]
fn undeclared_entity_references_are_rejected() {
    assert_eq!(rule("<r>&nbsp;</r>"), rules::UNDEFINED_ENTITY);
    assert_eq!(rule(r#"<r a="&nbsp;"/>"#), rules::UNDEFINED_ENTITY);
}

// ---- resource limits -------------------------------------------------------

#[test]
fn oversized_document_is_rejected_by_the_byte_limit() {
    let limits = Limits {
        max_document_bytes: 64,
        ..Limits::default()
    };
    let xml = format!("<r>{}</r>", "x".repeat(200));
    assert_eq!(rule_of(&xml, limits), rules::LIMIT_DOCUMENT_SIZE);
}

#[test]
fn document_exactly_at_the_byte_limit_is_accepted() {
    let xml = "<r>0123456789</r>";
    let limits = Limits {
        max_document_bytes: xml.len() as u64,
        ..Limits::default()
    };
    read_all(xml, limits).expect("at-limit document is accepted");
    let tight = Limits {
        max_document_bytes: xml.len() as u64 - 1,
        ..Limits::default()
    };
    assert_eq!(rule_of(xml, tight), rules::LIMIT_DOCUMENT_SIZE);
}

#[test]
fn deep_nesting_is_rejected_by_the_depth_limit() {
    let depth = 40;
    let xml = format!("{}{}", "<a>".repeat(depth), "</a>".repeat(depth));
    let limits = Limits {
        max_depth: 32,
        ..Limits::default()
    };
    assert_eq!(rule_of(&xml, limits.clone()), rules::LIMIT_DEPTH);
    let at_limit = format!("{}{}", "<a>".repeat(32), "</a>".repeat(32));
    read_all(&at_limit, limits).expect("depth equal to the limit is accepted");
}

#[test]
fn deeply_nested_hostile_input_fails_fast_without_stack_growth() {
    // 1M levels would blow the stack of a recursive parser; the default limit
    // must stop us after `max_depth` levels.
    let xml = "<a>".repeat(1_000_000);
    assert_eq!(rule(&xml), rules::LIMIT_DEPTH);
}

#[test]
fn too_many_attributes_are_rejected() {
    let attrs: String = (0..20).map(|i| format!(" a{i}=\"v\"")).collect();
    let limits = Limits {
        max_attributes_per_element: 10,
        ..Limits::default()
    };
    assert_eq!(
        rule_of(&format!("<r{attrs}/>"), limits),
        rules::LIMIT_ATTRIBUTES
    );
}

#[test]
fn oversized_tokens_are_rejected() {
    let limits = Limits {
        max_token_bytes: 32,
        ..Limits::default()
    };
    let text = format!("<r>{}</r>", "t".repeat(100));
    assert_eq!(rule_of(&text, limits.clone()), rules::LIMIT_TOKEN_SIZE);
    let attr = format!(r#"<r a="{}"/>"#, "v".repeat(100));
    assert_eq!(rule_of(&attr, limits.clone()), rules::LIMIT_TOKEN_SIZE);
    let name = format!("<{}/>", "n".repeat(100));
    assert_eq!(rule_of(&name, limits), rules::LIMIT_TOKEN_SIZE);
}

#[test]
fn coalesced_text_is_bounded_even_when_split_by_entities() {
    // Each piece is tiny; only the running total can trip the limit.
    let limits = Limits {
        max_token_bytes: 32,
        ..Limits::default()
    };
    let xml = format!("<r>{}</r>", "&amp;".repeat(100));
    assert_eq!(rule_of(&xml, limits), rules::LIMIT_TOKEN_SIZE);
}

#[test]
fn too_many_namespace_bindings_are_rejected() {
    let decls: String = (0..20)
        .map(|i| format!(" xmlns:p{i}=\"urn:{i}\""))
        .collect();
    let limits = Limits {
        max_namespace_bindings: 10,
        ..Limits::default()
    };
    assert_eq!(
        rule_of(&format!("<r{decls}/>"), limits),
        rules::LIMIT_NAMESPACE_BINDINGS
    );
}

#[test]
fn failures_carry_a_source_span() {
    let err = read_all("<a><b></a>", Limits::default()).unwrap_err();
    assert!(err.span().is_some());
}

#[test]
fn reader_is_fused_after_an_error() {
    let mut reader = XmlReader::new(&b"<a></b>"[..], Limits::default());
    let mut saw_error = false;
    for _ in 0..10 {
        match reader.next_event() {
            Err(_) => saw_error = true,
            Ok(None) => {
                assert!(saw_error, "end of stream must not mask the error");
                return;
            }
            Ok(Some(_)) => {}
        }
        if saw_error {
            // After the first error every call keeps failing.
            assert!(reader.next_event().is_err());
            return;
        }
    }
    panic!("never reached an error");
}
