//! Bounded XBRL 2.1 syntax extraction from [`crate::reader::XmlReader`].
//!
//! Only direct item facts are supported. Tuples, footnotes, linkbase/role refs
//! and fraction content fail closed until their semantics are implemented.
//! Segment/scenario extension trees are preserved without DTS interpretation. No taxonomy is loaded and extraction never asserts validity.
//! Specification: XBRL 2.1 REC 2003-12-31, errata 2013-02-20, §§4.1–4.8.

use std::io::Read;

use chrono::{DateTime, NaiveDate, NaiveDateTime};
use xbrl_core::diagnostic::{Diagnostic, RuleId, Severity, SourceSpan};
use xbrl_core::instance::{
    Accuracy, Context, Entity, ExtensionNode, Fact, Instance, Period, SchemaRef, Unit, UnitMeasures,
};
use xbrl_core::namespaces::{XBRL_INSTANCE, XBRL_LINKBASE, XLINK};
use xbrl_core::qname::QName;

use crate::limits::Limits;
use crate::reader::{NamespaceBinding, StartElement, XML_NAMESPACE, XmlEvent, XmlReader};
use crate::uri::Uri;

/// The root element is not xbrli:xbrl.
pub const INVALID_ROOT: &str = "xbrl-instance:invalid-root";
/// Required syntax is missing, out of sequence or malformed.
pub const INVALID_STRUCTURE: &str = "xbrl-instance:invalid-structure";
/// A QName-valued measure has an unbound prefix or no representable namespace.
pub const INVALID_MEASURE: &str = "xbrl-instance:invalid-measure";
/// A date/dateTime or duration boundary is invalid.
pub const INVALID_PERIOD: &str = "xbrl-instance:invalid-period";
/// An accuracy attribute is malformed or mutually exclusive with another.
pub const INVALID_ACCURACY: &str = "xbrl-instance:invalid-accuracy";
/// A construct requires DTS semantics, or exceeds the supported numeric range.
pub const UNSUPPORTED: &str = "xbrl-instance:unsupported";

fn error(rule: &str, message: impl Into<String>, span: SourceSpan) -> Diagnostic {
    Diagnostic::new(
        RuleId::new(rule).expect("static rule id"),
        Severity::Error,
        message,
        Some(span),
    )
}

#[derive(Debug)]
struct Node {
    start: StartElement,
    bindings: Vec<NamespaceBinding>,
    children: Vec<Node>,
    text: String,
}

/// Extracts the supported XBRL 2.1 instance subset with XML limits enforced.
///
/// # Errors
///
/// Returns a rule-coded [`Diagnostic`] on malformed XML, resource bounds,
/// unsupported constructs, or malformed supported syntax. It does **not**
/// validate taxonomy types, IDREF targets, duplicates or filing profiles.
/// Storage for the extracted tree is bounded by the XML input byte budget.
pub fn parse_instance<R: Read>(source: R, limits: Limits) -> Result<Instance, Diagnostic> {
    let mut reader = XmlReader::new(source, limits);
    let mut stack: Vec<Node> = Vec::new();
    let mut root = None;
    while let Some(event) = reader.next_event()? {
        match event {
            XmlEvent::Start(start) => {
                // Only measure text needs in-scope bindings. Cloning every root
                // sibling's namespace stack would multiply memory by the
                // namespace count on long filings.
                let bindings = if start.name.namespace() == Some(XBRL_INSTANCE)
                    && start.name.local_name() == "measure"
                {
                    let mut bindings = Vec::new();
                    for ancestor in &stack {
                        bindings.extend(ancestor.start.namespaces.iter().cloned());
                    }
                    bindings.extend(start.namespaces.iter().cloned());
                    bindings
                } else {
                    Vec::new()
                };
                stack.push(Node {
                    start,
                    bindings,
                    children: Vec::new(),
                    text: String::new(),
                });
            }
            XmlEvent::Text(text) => {
                if let Some(node) = stack.last_mut() {
                    node.text.push_str(&text.text);
                }
            }
            XmlEvent::End(_) => {
                let node = stack
                    .pop()
                    .expect("well-formed reader balances starts and ends");
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else {
                    root = Some(node);
                }
            }
        }
    }
    let root = root.expect("reader requires root");
    parse_root(&root)
}

fn is(node: &Node, ns: &str, local: &str) -> bool {
    node.start.name.namespace() == Some(ns) && node.start.name.local_name() == local
}

fn attr<'a>(node: &'a Node, ns: Option<&str>, local: &str) -> Option<&'a str> {
    node.start
        .attributes
        .iter()
        .find(|a| a.name.namespace() == ns && a.name.local_name() == local)
        .map(|a| a.value.as_str())
}

fn required<'a>(node: &'a Node, ns: Option<&str>, local: &str) -> Result<&'a str, Diagnostic> {
    attr(node, ns, local)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            error(
                INVALID_STRUCTURE,
                format!("missing or empty @{local}"),
                node.start.span,
            )
        })
}

fn only_whitespace(node: &Node) -> Result<(), Diagnostic> {
    if node.text.trim().is_empty() {
        Ok(())
    } else {
        Err(error(
            INVALID_STRUCTURE,
            "unexpected character data",
            node.start.span,
        ))
    }
}

fn expected_children(node: &Node, expected: &[(&str, &str)]) -> Result<(), Diagnostic> {
    only_whitespace(node)?;
    if node.children.len() != expected.len()
        || node
            .children
            .iter()
            .zip(expected)
            .any(|(n, &(ns, local))| !is(n, ns, local))
    {
        return Err(error(
            INVALID_STRUCTURE,
            format!("unexpected children in {}", node.start.name.local_name()),
            node.start.span,
        ));
    }
    Ok(())
}

fn token(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn id(node: &Node) -> Result<String, Diagnostic> {
    let id = token(required(node, None, "id")?);
    if !ncname(&id) {
        return Err(error(INVALID_STRUCTURE, "invalid XML ID", node.start.span));
    }
    Ok(id)
}

fn ncname(s: &str) -> bool {
    let mut it = s.chars();
    matches!(it.next(), Some(c) if c.is_alphabetic() || c == '_')
        && it.all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

fn parse_root(root: &Node) -> Result<Instance, Diagnostic> {
    if !is(root, XBRL_INSTANCE, "xbrl") {
        return Err(error(INVALID_ROOT, "expected xbrli:xbrl", root.start.span));
    }
    only_whitespace(root)?;
    let mut instance = Instance {
        schema_refs: Vec::new(),
        contexts: Vec::new(),
        units: Vec::new(),
        facts: Vec::new(),
    };
    let mut past_refs = false;
    for node in &root.children {
        if is(node, XBRL_LINKBASE, "schemaRef") {
            if past_refs {
                return Err(error(
                    INVALID_STRUCTURE,
                    "schemaRef must precede other root children",
                    node.start.span,
                ));
            }
            instance.schema_refs.push(schema_ref(root, node)?);
        } else {
            past_refs = true;
            if is(node, XBRL_INSTANCE, "context") {
                instance.contexts.push(context(node)?);
            } else if is(node, XBRL_INSTANCE, "unit") {
                instance.units.push(unit(node)?);
            } else if node.start.name.namespace() == Some(XBRL_LINKBASE)
                || node.start.name.namespace() == Some(XBRL_INSTANCE)
            {
                return Err(error(
                    UNSUPPORTED,
                    format!("unsupported root child {}", node.start.name.local_name()),
                    node.start.span,
                ));
            } else {
                instance.facts.push(fact(node)?);
            }
        }
    }
    if instance.schema_refs.is_empty() {
        return Err(error(
            INVALID_STRUCTURE,
            "at least one schemaRef is required",
            root.start.span,
        ));
    }
    Ok(instance)
}

fn schema_ref(root: &Node, node: &Node) -> Result<SchemaRef, Diagnostic> {
    expected_children(node, &[])?;
    if required(node, Some(XLINK), "type")? != "simple" {
        return Err(error(
            INVALID_STRUCTURE,
            "schemaRef xlink:type must be simple",
            node.start.span,
        ));
    }
    let href = required(node, Some(XLINK), "href")?;
    Uri::parse(href)
        .map_err(|_| error(INVALID_STRUCTURE, "invalid schemaRef URI", node.start.span))?;
    let base_chain = [
        attr(root, Some(XML_NAMESPACE), "base"),
        attr(node, Some(XML_NAMESPACE), "base"),
    ]
    .into_iter()
    .flatten()
    .map(str::to_owned)
    .collect();
    Ok(SchemaRef {
        href: href.to_owned(),
        base_chain,
        span: node.start.span,
    })
}

fn context(node: &Node) -> Result<Context, Diagnostic> {
    only_whitespace(node)?;
    if node.children.len() < 2
        || node.children.len() > 3
        || !is(&node.children[0], XBRL_INSTANCE, "entity")
        || !is(&node.children[1], XBRL_INSTANCE, "period")
        || node
            .children
            .get(2)
            .is_some_and(|n| !is(n, XBRL_INSTANCE, "scenario"))
    {
        return Err(error(
            INVALID_STRUCTURE,
            "context requires entity, period, optional scenario",
            node.start.span,
        ));
    }
    let entity_node = &node.children[0];
    only_whitespace(entity_node)?;
    if entity_node.children.is_empty()
        || entity_node.children.len() > 2
        || !is(&entity_node.children[0], XBRL_INSTANCE, "identifier")
        || entity_node
            .children
            .get(1)
            .is_some_and(|n| !is(n, XBRL_INSTANCE, "segment"))
    {
        return Err(error(
            INVALID_STRUCTURE,
            "entity requires identifier, optional segment",
            entity_node.start.span,
        ));
    }
    let identifier = &entity_node.children[0];
    if !identifier.children.is_empty() {
        return Err(error(
            INVALID_STRUCTURE,
            "identifier cannot contain elements",
            identifier.start.span,
        ));
    }
    let scheme = token(required(identifier, None, "scheme")?);
    if scheme.is_empty() {
        return Err(error(
            INVALID_STRUCTURE,
            "empty identifier scheme",
            identifier.start.span,
        ));
    }
    let entity = Entity {
        scheme,
        identifier: token(&identifier.text),
        segment: entity_node
            .children
            .get(1)
            .map(extension_content)
            .transpose()?,
    };
    Ok(Context {
        id: id(node)?,
        entity,
        period: period(&node.children[1])?,
        scenario: node.children.get(2).map(extension_content).transpose()?,
        span: node.start.span,
    })
}

fn extension_content(node: &Node) -> Result<Vec<ExtensionNode>, Diagnostic> {
    only_whitespace(node)?;
    if node.children.is_empty() {
        return Err(error(
            INVALID_STRUCTURE,
            "segment/scenario must contain extension elements",
            node.start.span,
        ));
    }
    node.children.iter().map(extension_node).collect()
}

fn extension_node(node: &Node) -> Result<ExtensionNode, Diagnostic> {
    if node.start.name.namespace() == Some(XBRL_INSTANCE) {
        return Err(error(
            INVALID_STRUCTURE,
            "XBRL instance element inside segment/scenario",
            node.start.span,
        ));
    }
    let name = node.start.name.to_qname().ok_or_else(|| {
        error(
            UNSUPPORTED,
            "extension element without a namespace",
            node.start.span,
        )
    })?;
    let attributes = node
        .start
        .attributes
        .iter()
        .map(|a| {
            (
                a.name.to_qname(),
                a.name.local_name().to_owned(),
                a.value.clone(),
            )
        })
        .collect();
    Ok(ExtensionNode {
        name,
        attributes,
        text: node.text.clone(),
        children: node
            .children
            .iter()
            .map(extension_node)
            .collect::<Result<_, _>>()?,
    })
}

fn period(node: &Node) -> Result<Period, Diagnostic> {
    only_whitespace(node)?;
    let names: Vec<&str> = node
        .children
        .iter()
        .map(|n| n.start.name.local_name())
        .collect();
    if node
        .children
        .iter()
        .any(|n| n.start.name.namespace() != Some(XBRL_INSTANCE))
    {
        return Err(error(
            INVALID_PERIOD,
            "unexpected period namespace",
            node.start.span,
        ));
    }
    match names.as_slice() {
        ["instant"] => Ok(Period::Instant(date(&node.children[0])?)),
        ["forever"] => {
            expected_children(&node.children[0], &[])?;
            Ok(Period::Forever)
        }
        ["startDate", "endDate"] => {
            let start = date(&node.children[0])?;
            let end = date(&node.children[1])?;
            let date_start = NaiveDate::parse_from_str(&start, "%Y-%m-%d");
            let date_end = NaiveDate::parse_from_str(&end, "%Y-%m-%d");
            let reversed = if let (Ok(s), Ok(e)) = (date_start, date_end) {
                // A date-only end implies midnight of the following day (§4.7.2).
                e < s
            } else if let (Ok(s), Ok(e)) = (
                DateTime::parse_from_rfc3339(&start),
                DateTime::parse_from_rfc3339(&end),
            ) {
                e <= s
            } else if let (Ok(s), Ok(e)) = (
                NaiveDateTime::parse_from_str(&start, "%Y-%m-%dT%H:%M:%S%.f"),
                NaiveDateTime::parse_from_str(&end, "%Y-%m-%dT%H:%M:%S%.f"),
            ) {
                e <= s
            } else {
                false // mixed timezone representations: comparison deferred to validation
            };
            if reversed {
                return Err(error(
                    INVALID_PERIOD,
                    "endDate does not follow startDate",
                    node.start.span,
                ));
            }
            Ok(Period::Duration { start, end })
        }
        _ => Err(error(
            INVALID_PERIOD,
            "period must contain instant, forever or startDate then endDate",
            node.start.span,
        )),
    }
}

fn date(node: &Node) -> Result<String, Diagnostic> {
    if !node.children.is_empty() {
        return Err(error(
            INVALID_PERIOD,
            "date has child elements",
            node.start.span,
        ));
    }
    let value = token(&node.text);
    let zoned_date = value
        .get(..10)
        .zip(value.get(10..))
        .is_some_and(|(day, zone)| {
            NaiveDate::parse_from_str(day, "%Y-%m-%d").is_ok()
                && !zone.is_empty()
                && DateTime::parse_from_rfc3339(&format!("{day}T00:00:00{zone}")).is_ok()
        });
    let valid = zoned_date
        || NaiveDate::parse_from_str(&value, "%Y-%m-%d").is_ok()
        || NaiveDateTime::parse_from_str(&value, "%Y-%m-%dT%H:%M:%S%.f").is_ok()
        || DateTime::parse_from_rfc3339(&value).is_ok();
    if !valid {
        return Err(error(
            INVALID_PERIOD,
            format!("unsupported or invalid date/dateTime {value:?}"),
            node.start.span,
        ));
    }
    Ok(value)
}

fn unit(node: &Node) -> Result<Unit, Diagnostic> {
    only_whitespace(node)?;
    let measures = if node
        .children
        .iter()
        .all(|n| is(n, XBRL_INSTANCE, "measure"))
        && !node.children.is_empty()
    {
        UnitMeasures::Simple(measures(node)?)
    } else {
        expected_children(node, &[(XBRL_INSTANCE, "divide")])?;
        let divide = &node.children[0];
        expected_children(
            divide,
            &[
                (XBRL_INSTANCE, "unitNumerator"),
                (XBRL_INSTANCE, "unitDenominator"),
            ],
        )?;
        UnitMeasures::Divide {
            numerator: measures(&divide.children[0])?,
            denominator: measures(&divide.children[1])?,
        }
    };
    Ok(Unit {
        id: id(node)?,
        measures,
        span: node.start.span,
    })
}

fn measures(node: &Node) -> Result<Vec<QName>, Diagnostic> {
    only_whitespace(node)?;
    if node.children.is_empty()
        || node
            .children
            .iter()
            .any(|n| !is(n, XBRL_INSTANCE, "measure"))
    {
        return Err(error(
            INVALID_STRUCTURE,
            "unit product requires one or more measures",
            node.start.span,
        ));
    }
    node.children.iter().map(measure).collect()
}

fn measure(node: &Node) -> Result<QName, Diagnostic> {
    if !node.children.is_empty() {
        return Err(error(
            INVALID_MEASURE,
            "measure cannot contain elements",
            node.start.span,
        ));
    }
    let value = token(&node.text);
    let (prefix, local) = match value.split_once(':') {
        Some((p, l)) if ncname(p) && ncname(l) => (Some(p), l),
        None if ncname(&value) => (None, value.as_str()),
        _ => {
            return Err(error(
                INVALID_MEASURE,
                "invalid lexical measure QName",
                node.start.span,
            ));
        }
    };
    let namespace = node
        .bindings
        .iter()
        .rev()
        .find(|b| b.prefix() == prefix)
        .map(NamespaceBinding::uri)
        .or_else(|| (prefix == Some("xml")).then_some(XML_NAMESPACE));
    let Some(namespace) = namespace.filter(|n| !n.is_empty()) else {
        return Err(error(
            INVALID_MEASURE,
            "measure QName has no bound namespace",
            node.start.span,
        ));
    };
    QName::new(prefix, namespace, local)
        .map_err(|_| error(INVALID_MEASURE, "invalid measure QName", node.start.span))
}

fn fact(node: &Node) -> Result<Fact, Diagnostic> {
    if !node.children.is_empty() {
        return Err(error(
            UNSUPPORTED,
            "tuple or complex fact content requires taxonomy type information",
            node.start.span,
        ));
    }
    let concept = node.start.name.to_qname().ok_or_else(|| {
        error(
            UNSUPPORTED,
            "fact concept has no usable namespace",
            node.start.span,
        )
    })?;
    let context_ref = token(required(node, None, "contextRef")?);
    if !ncname(&context_ref) {
        return Err(error(
            INVALID_STRUCTURE,
            "invalid contextRef IDREF",
            node.start.span,
        ));
    }
    let unit_ref = attr(node, None, "unitRef").map(token);
    if unit_ref.as_ref().is_some_and(|s| !ncname(s)) {
        return Err(error(
            INVALID_STRUCTURE,
            "invalid unitRef IDREF",
            node.start.span,
        ));
    }
    let accuracy = accuracy(node)?;
    let nil = match attr(
        node,
        Some("http://www.w3.org/2001/XMLSchema-instance"),
        "nil",
    ) {
        Some("true" | "1") => true,
        Some("false" | "0") | None => false,
        Some(_) => {
            return Err(error(
                INVALID_STRUCTURE,
                "invalid xsi:nil boolean",
                node.start.span,
            ));
        }
    };
    if nil && (accuracy.is_some() || !node.text.trim().is_empty()) {
        return Err(error(
            INVALID_STRUCTURE,
            "nil item cannot have accuracy or content",
            node.start.span,
        ));
    }
    let id = attr(node, None, "id").map(token);
    if id.as_ref().is_some_and(|s| !ncname(s)) {
        return Err(error(INVALID_STRUCTURE, "invalid fact ID", node.start.span));
    }
    Ok(Fact {
        concept,
        context_ref,
        unit_ref,
        accuracy,
        value: if nil { None } else { Some(node.text.clone()) },
        id,
        span: node.start.span,
    })
}

fn accuracy(node: &Node) -> Result<Option<Accuracy>, Diagnostic> {
    let decimals = attr(node, None, "decimals");
    let precision = attr(node, None, "precision");
    match (decimals, precision) {
        (Some(_), Some(_)) => Err(error(
            INVALID_ACCURACY,
            "decimals and precision are mutually exclusive",
            node.start.span,
        )),
        (None, None) => Ok(None),
        (Some(value), None) if value.trim() == "INF" => Ok(Some(Accuracy::Infinite)),
        (None, Some(value)) if value.trim() == "INF" => Ok(Some(Accuracy::Infinite)),
        (Some(value), None) => {
            let v = value.trim();
            if !signed_digits(v) {
                return Err(error(
                    INVALID_ACCURACY,
                    "decimals must be an integer or INF",
                    node.start.span,
                ));
            }
            let parsed = v.parse::<i64>().map_err(|_| {
                error(
                    UNSUPPORTED,
                    "decimals integer exceeds i64 range",
                    node.start.span,
                )
            })?;
            Ok(Some(Accuracy::Decimals(parsed)))
        }
        (None, Some(value)) => {
            let v = value.trim();
            if !unsigned_digits(v) {
                return Err(error(
                    INVALID_ACCURACY,
                    "precision must be a non-negative integer or INF",
                    node.start.span,
                ));
            }
            let parsed = v.trim_start_matches('+').parse::<u64>().map_err(|_| {
                error(
                    UNSUPPORTED,
                    "precision integer exceeds u64 range",
                    node.start.span,
                )
            })?;
            Ok(Some(Accuracy::Precision(parsed)))
        }
    }
}

fn unsigned_digits(value: &str) -> bool {
    let value = value.strip_prefix('+').unwrap_or(value);
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())
}
fn signed_digits(value: &str) -> bool {
    unsigned_digits(value.strip_prefix('-').unwrap_or(value))
}
