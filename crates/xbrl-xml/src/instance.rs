//! Bounded XBRL 2.1 syntax extraction from [`crate::reader::XmlReader`].
//!
//! Only direct item facts are supported. Tuples, footnotes, linkbase/role refs
//! and fraction content fail closed until their semantics are implemented.
//! Segment/scenario extension trees are preserved without DTS interpretation.
//! No taxonomy is loaded and extraction never asserts validity.
//! Specification: XBRL 2.1 REC 2003-12-31, errata 2013-02-20, §§4.1–4.8;
//! lexical forms follow XML Schema 1.0 Part 2.
//!
//! # Memory model
//!
//! Extraction is a single pass with no document-sized DOM:
//!
//! * `schemaRef` and item facts (direct children of the root) are *leaves*:
//!   one start tag plus one text buffer, finalized at the end tag.
//! * Only a `context` or `unit` subtree is buffered as a small tree, and it is
//!   dropped as soon as it is converted into the typed model.
//! * Namespace bindings live in one shared scope stack that grows by the
//!   declarations of each open element and shrinks at its end tag. Measure
//!   QNames are resolved at the measure's end tag against that stack, so no
//!   per-element copy of in-scope bindings exists.
//! * The number of elements is capped by [`Limits::max_instance_elements`]
//!   (reported as [`LIMIT_ELEMENTS`]); document bytes, depth, attribute and
//!   token sizes are capped by the reader.
//!
//! Retained memory is therefore the typed output plus the open `context`/`unit`
//! subtree, both bounded by the byte and element limits.

use std::collections::HashSet;
use std::io::Read;

use xbrl_core::diagnostic::{Diagnostic, RuleId, Severity, SourceSpan};
use xbrl_core::instance::{
    Accuracy, Context, Entity, ExtensionNode, Fact, Instance, Period, SchemaRef, Unit, UnitMeasures,
};
use xbrl_core::namespaces::{XBRL_INSTANCE, XBRL_LINKBASE, XLINK};
use xbrl_core::qname::QName;

use crate::limits::Limits;
use crate::reader::{StartElement, XML_NAMESPACE, XmlEvent, XmlReader};
use crate::uri::Uri;
use crate::xsd::{Temporal, TemporalError, collapse, duration_is_reversed, is_blank};

/// The root element is not xbrli:xbrl.
pub const INVALID_ROOT: &str = "xbrl-instance:invalid-root";
/// Required syntax is missing, out of sequence, duplicated or malformed.
pub const INVALID_STRUCTURE: &str = "xbrl-instance:invalid-structure";
/// A QName-valued measure has an unbound prefix or no representable namespace.
pub const INVALID_MEASURE: &str = "xbrl-instance:invalid-measure";
/// A date/dateTime or duration boundary is invalid.
pub const INVALID_PERIOD: &str = "xbrl-instance:invalid-period";
/// An accuracy attribute is malformed or mutually exclusive with another.
pub const INVALID_ACCURACY: &str = "xbrl-instance:invalid-accuracy";
/// A construct requires DTS semantics, or exceeds the supported range.
pub const UNSUPPORTED: &str = "xbrl-instance:unsupported";
/// Too many instance elements were materialized.
pub const LIMIT_ELEMENTS: &str = "xbrl-instance:limit-elements";

const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";

fn error(rule: &str, message: impl Into<String>, span: SourceSpan) -> Diagnostic {
    Diagnostic::new(
        RuleId::new(rule).expect("static rule id"),
        Severity::Error,
        message,
        Some(span),
    )
}

/// A buffered element inside a `context` or `unit` subtree.
#[derive(Debug)]
struct Node {
    start: StartElement,
    /// Resolved at the end tag for `xbrli:measure` only.
    measure: Option<QName>,
    children: Vec<Node>,
    text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LeafKind {
    Fact,
    SchemaRef,
}

/// A direct root child that is finalized without buffering a subtree.
#[derive(Debug)]
struct Leaf {
    start: StartElement,
    kind: LeafKind,
    text: String,
}

/// Extracts the supported XBRL 2.1 instance subset with XML limits enforced.
///
/// # Errors
///
/// Returns a rule-coded [`Diagnostic`] on malformed XML, resource bounds,
/// unsupported constructs, or malformed supported syntax. It does **not**
/// validate taxonomy types, IDREF targets or filing profiles. It does enforce
/// document-wide uniqueness of `context`, `unit` and fact `id` values.
pub fn parse_instance<R: Read>(source: R, limits: Limits) -> Result<Instance, Diagnostic> {
    let max_elements = limits.max_instance_elements;
    let mut reader = XmlReader::new(source, limits);
    let mut elements: u32 = 0;
    let mut depth: usize = 0;
    // Shared in-scope namespace bindings and the length to restore per element.
    let mut scope: Vec<(Option<String>, String)> = Vec::new();
    let mut marks: Vec<usize> = Vec::new();
    let mut stack: Vec<Node> = Vec::new();
    let mut leaf: Option<Leaf> = None;
    let mut ids: HashSet<String> = HashSet::new();
    let mut root_base: Option<String> = None;
    let mut root_span: Option<SourceSpan> = None;
    let mut past_refs = false;
    let mut instance = Instance {
        schema_refs: Vec::new(),
        contexts: Vec::new(),
        units: Vec::new(),
        facts: Vec::new(),
    };

    while let Some(event) = reader.next_event()? {
        match event {
            XmlEvent::Start(start) => {
                elements = elements.saturating_add(1);
                if elements > max_elements {
                    return Err(error(
                        LIMIT_ELEMENTS,
                        "instance element count exceeds limit",
                        start.span,
                    ));
                }
                marks.push(scope.len());
                for binding in &start.namespaces {
                    scope.push((
                        binding.prefix().map(str::to_owned),
                        binding.uri().to_owned(),
                    ));
                }
                depth += 1;
                match depth {
                    1 => {
                        if !is(&start, XBRL_INSTANCE, "xbrl") {
                            return Err(error(INVALID_ROOT, "expected xbrli:xbrl", start.span));
                        }
                        root_base = attr(&start, Some(XML_NAMESPACE), "base").map(str::to_owned);
                        root_span = Some(start.span);
                    }
                    2 => {
                        if is(&start, XBRL_LINKBASE, "schemaRef") {
                            if past_refs {
                                return Err(error(
                                    INVALID_STRUCTURE,
                                    "schemaRef must precede other root children",
                                    start.span,
                                ));
                            }
                            leaf = Some(Leaf {
                                start,
                                kind: LeafKind::SchemaRef,
                                text: String::new(),
                            });
                            continue;
                        }
                        past_refs = true;
                        if is(&start, XBRL_INSTANCE, "context") || is(&start, XBRL_INSTANCE, "unit")
                        {
                            stack.push(node(start));
                        } else if start.name.namespace() == Some(XBRL_LINKBASE)
                            || start.name.namespace() == Some(XBRL_INSTANCE)
                        {
                            return Err(error(
                                UNSUPPORTED,
                                format!("unsupported root child {}", start.name.local_name()),
                                start.span,
                            ));
                        } else {
                            leaf = Some(Leaf {
                                start,
                                kind: LeafKind::Fact,
                                text: String::new(),
                            });
                        }
                    }
                    _ => {
                        if let Some(open) = &leaf {
                            return Err(match open.kind {
                                LeafKind::Fact => error(
                                    UNSUPPORTED,
                                    "tuple or complex fact content requires taxonomy type information",
                                    open.start.span,
                                ),
                                LeafKind::SchemaRef => error(
                                    INVALID_STRUCTURE,
                                    "schemaRef cannot contain elements",
                                    open.start.span,
                                ),
                            });
                        }
                        stack.push(node(start));
                    }
                }
            }
            XmlEvent::Text(text) => {
                if let Some(open) = &mut leaf {
                    open.text.push_str(&text.text);
                } else if let Some(top) = stack.last_mut() {
                    top.text.push_str(&text.text);
                } else if depth == 1 && !is_blank(&text.text) {
                    return Err(error(
                        INVALID_STRUCTURE,
                        "unexpected character data",
                        text.span,
                    ));
                }
            }
            XmlEvent::End(end) => {
                match depth {
                    1 => {
                        if instance.schema_refs.is_empty() {
                            return Err(error(
                                INVALID_STRUCTURE,
                                "at least one schemaRef is required",
                                end.span,
                            ));
                        }
                    }
                    2 => {
                        if let Some(done) = leaf.take() {
                            match done.kind {
                                LeafKind::SchemaRef => instance
                                    .schema_refs
                                    .push(schema_ref(&done, root_base.as_deref())?),
                                LeafKind::Fact => {
                                    let fact = fact(&done)?;
                                    if let Some(id) = &fact.id {
                                        claim(&mut ids, id, fact.span)?;
                                    }
                                    instance.facts.push(fact);
                                }
                            }
                        } else {
                            let done = stack.pop().expect("reader balances start and end");
                            if is(&done.start, XBRL_INSTANCE, "context") {
                                let ctx = context(&done)?;
                                claim(&mut ids, &ctx.id, ctx.span)?;
                                instance.contexts.push(ctx);
                            } else {
                                let u = unit(&done)?;
                                claim(&mut ids, &u.id, u.span)?;
                                instance.units.push(u);
                            }
                        }
                    }
                    _ => {
                        let mut done = stack.pop().expect("reader balances start and end");
                        if is(&done.start, XBRL_INSTANCE, "measure") {
                            done.measure = Some(measure_qname(&done, &scope)?);
                        }
                        stack
                            .last_mut()
                            .expect("subtree has an open parent")
                            .children
                            .push(done);
                    }
                }
                depth -= 1;
                scope.truncate(marks.pop().expect("one mark per open element"));
            }
        }
    }
    if root_span.is_none() {
        return Err(error(
            INVALID_ROOT,
            "document has no root element",
            SourceSpan::new(0, 0).expect("empty span"),
        ));
    }
    Ok(instance)
}

fn node(start: StartElement) -> Node {
    Node {
        start,
        measure: None,
        children: Vec::new(),
        text: String::new(),
    }
}

/// Records an XML ID, rejecting a second use anywhere in the instance.
fn claim(ids: &mut HashSet<String>, id: &str, span: SourceSpan) -> Result<(), Diagnostic> {
    if ids.insert(id.to_owned()) {
        Ok(())
    } else {
        Err(error(
            INVALID_STRUCTURE,
            format!("duplicate XML ID {id:?}"),
            span,
        ))
    }
}

fn is(start: &StartElement, ns: &str, local: &str) -> bool {
    start.name.namespace() == Some(ns) && start.name.local_name() == local
}

fn node_is(node: &Node, ns: &str, local: &str) -> bool {
    is(&node.start, ns, local)
}

fn attr<'a>(start: &'a StartElement, ns: Option<&str>, local: &str) -> Option<&'a str> {
    start
        .attributes
        .iter()
        .find(|a| a.name.namespace() == ns && a.name.local_name() == local)
        .map(|a| a.value.as_str())
}

fn required<'a>(
    start: &'a StartElement,
    ns: Option<&str>,
    local: &str,
) -> Result<&'a str, Diagnostic> {
    attr(start, ns, local)
        .filter(|v| !is_blank(v))
        .ok_or_else(|| {
            error(
                INVALID_STRUCTURE,
                format!("missing or empty @{local}"),
                start.span,
            )
        })
}

fn only_whitespace(node: &Node) -> Result<(), Diagnostic> {
    if is_blank(&node.text) {
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
            .any(|(n, &(ns, local))| !node_is(n, ns, local))
    {
        return Err(error(
            INVALID_STRUCTURE,
            format!("unexpected children in {}", node.start.name.local_name()),
            node.start.span,
        ));
    }
    Ok(())
}

fn ncname(s: &str) -> bool {
    let mut it = s.chars();
    matches!(it.next(), Some(c) if c.is_alphabetic() || c == '_')
        && it.all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

fn id_attr(start: &StartElement) -> Result<String, Diagnostic> {
    let id = collapse(required(start, None, "id")?);
    if !ncname(&id) {
        return Err(error(INVALID_STRUCTURE, "invalid XML ID", start.span));
    }
    Ok(id)
}

fn schema_ref(leaf: &Leaf, root_base: Option<&str>) -> Result<SchemaRef, Diagnostic> {
    let start = &leaf.start;
    if !is_blank(&leaf.text) {
        return Err(error(
            INVALID_STRUCTURE,
            "unexpected character data",
            start.span,
        ));
    }
    if collapse(required(start, Some(XLINK), "type")?) != "simple" {
        return Err(error(
            INVALID_STRUCTURE,
            "schemaRef xlink:type must be simple",
            start.span,
        ));
    }
    let href = required(start, Some(XLINK), "href")?;
    Uri::parse(href).map_err(|_| error(INVALID_STRUCTURE, "invalid schemaRef URI", start.span))?;
    let base_chain = [root_base, attr(start, Some(XML_NAMESPACE), "base")]
        .into_iter()
        .flatten()
        .map(str::to_owned)
        .collect();
    Ok(SchemaRef {
        href: href.to_owned(),
        base_chain,
        span: start.span,
    })
}

fn context(node: &Node) -> Result<Context, Diagnostic> {
    only_whitespace(node)?;
    if node.children.len() < 2
        || node.children.len() > 3
        || !node_is(&node.children[0], XBRL_INSTANCE, "entity")
        || !node_is(&node.children[1], XBRL_INSTANCE, "period")
        || node
            .children
            .get(2)
            .is_some_and(|n| !node_is(n, XBRL_INSTANCE, "scenario"))
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
        || !node_is(&entity_node.children[0], XBRL_INSTANCE, "identifier")
        || entity_node
            .children
            .get(1)
            .is_some_and(|n| !node_is(n, XBRL_INSTANCE, "segment"))
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
    let scheme = collapse(required(&identifier.start, None, "scheme")?);
    if scheme.is_empty() || scheme.chars().any(char::is_whitespace) || Uri::parse(&scheme).is_err()
    {
        return Err(error(
            INVALID_STRUCTURE,
            "identifier scheme must be a nonempty URI without whitespace",
            identifier.start.span,
        ));
    }
    let entity = Entity {
        scheme,
        identifier: collapse(&identifier.text),
        segment: entity_node
            .children
            .get(1)
            .map(extension_content)
            .transpose()?,
    };
    Ok(Context {
        id: id_attr(&node.start)?,
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
    let names: Vec<&str> = node
        .children
        .iter()
        .map(|n| n.start.name.local_name())
        .collect();
    match names.as_slice() {
        ["instant"] => Ok(Period::Instant(date(&node.children[0])?.0)),
        ["forever"] => {
            expected_children(&node.children[0], &[])?;
            Ok(Period::Forever)
        }
        ["startDate", "endDate"] => {
            let (start, start_t) = date(&node.children[0])?;
            let (end, end_t) = date(&node.children[1])?;
            if duration_is_reversed(&start_t, &end_t) {
                return Err(error(
                    INVALID_PERIOD,
                    "endDate must be later than startDate (XBRL 2.1 §4.7.2)",
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

fn date(node: &Node) -> Result<(String, Temporal), Diagnostic> {
    if !node.children.is_empty() {
        return Err(error(
            INVALID_PERIOD,
            "date has child elements",
            node.start.span,
        ));
    }
    let value = collapse(&node.text);
    match Temporal::parse(&value) {
        Ok(parsed) => Ok((value, parsed)),
        Err(TemporalError::Invalid) => Err(error(
            INVALID_PERIOD,
            format!("invalid xsd:date/dateTime {value:?}"),
            node.start.span,
        )),
        Err(TemporalError::Unsupported) => Err(error(
            UNSUPPORTED,
            format!("date/dateTime {value:?} is outside the supported year range"),
            node.start.span,
        )),
    }
}

fn unit(node: &Node) -> Result<Unit, Diagnostic> {
    only_whitespace(node)?;
    let measures = if !node.children.is_empty()
        && node
            .children
            .iter()
            .all(|n| node_is(n, XBRL_INSTANCE, "measure"))
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
        id: id_attr(&node.start)?,
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
            .any(|n| !node_is(n, XBRL_INSTANCE, "measure"))
    {
        return Err(error(
            INVALID_STRUCTURE,
            "unit product requires one or more measures",
            node.start.span,
        ));
    }
    Ok(node
        .children
        .iter()
        .map(|n| n.measure.clone().expect("measures resolved at end tag"))
        .collect())
}

/// Resolves a measure's QName content against the bindings in scope at its end
/// tag (the measure's own declarations, then its ancestors').
fn measure_qname(node: &Node, scope: &[(Option<String>, String)]) -> Result<QName, Diagnostic> {
    if !node.children.is_empty() {
        return Err(error(
            INVALID_MEASURE,
            "measure cannot contain elements",
            node.start.span,
        ));
    }
    let value = collapse(&node.text);
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
    let namespace = scope
        .iter()
        .rev()
        .find(|(p, _)| p.as_deref() == prefix)
        .map(|(_, uri)| uri.as_str())
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

fn fact(leaf: &Leaf) -> Result<Fact, Diagnostic> {
    let start = &leaf.start;
    let concept = start.name.to_qname().ok_or_else(|| {
        error(
            UNSUPPORTED,
            "fact concept has no usable namespace",
            start.span,
        )
    })?;
    let context_ref = collapse(required(start, None, "contextRef")?);
    if !ncname(&context_ref) {
        return Err(error(
            INVALID_STRUCTURE,
            "invalid contextRef IDREF",
            start.span,
        ));
    }
    let unit_ref = attr(start, None, "unitRef").map(collapse);
    if unit_ref.as_ref().is_some_and(|s| !ncname(s)) {
        return Err(error(
            INVALID_STRUCTURE,
            "invalid unitRef IDREF",
            start.span,
        ));
    }
    let accuracy = accuracy(start)?;
    let nil = match attr(start, Some(XSI), "nil").map(collapse).as_deref() {
        Some("true" | "1") => true,
        Some("false" | "0") | None => false,
        Some(_) => {
            return Err(error(
                INVALID_STRUCTURE,
                "invalid xsi:nil boolean",
                start.span,
            ));
        }
    };
    if nil && (accuracy.is_some() || !is_blank(&leaf.text)) {
        return Err(error(
            INVALID_STRUCTURE,
            "nil item cannot have accuracy or content",
            start.span,
        ));
    }
    let id = attr(start, None, "id").map(collapse);
    if id.as_ref().is_some_and(|s| !ncname(s)) {
        return Err(error(INVALID_STRUCTURE, "invalid fact ID", start.span));
    }
    Ok(Fact {
        concept,
        context_ref,
        unit_ref,
        accuracy,
        value: if nil { None } else { Some(leaf.text.clone()) },
        id,
        span: start.span,
    })
}

fn accuracy(start: &StartElement) -> Result<Option<Accuracy>, Diagnostic> {
    let decimals = attr(start, None, "decimals").map(collapse);
    let precision = attr(start, None, "precision").map(collapse);
    match (decimals.as_deref(), precision.as_deref()) {
        (Some(_), Some(_)) => Err(error(
            INVALID_ACCURACY,
            "decimals and precision are mutually exclusive",
            start.span,
        )),
        (None, None) => Ok(None),
        (Some("INF"), None) | (None, Some("INF")) => Ok(Some(Accuracy::Infinite)),
        (Some(v), None) => {
            let digits = xsd_integer(v).ok_or_else(|| {
                error(
                    INVALID_ACCURACY,
                    "decimals must be an integer or INF",
                    start.span,
                )
            })?;
            let parsed = digits.parse::<i64>().map_err(|_| {
                error(
                    UNSUPPORTED,
                    "decimals integer exceeds i64 range",
                    start.span,
                )
            })?;
            Ok(Some(Accuracy::Decimals(parsed)))
        }
        (None, Some(v)) => {
            let digits = xsd_integer(v)
                .filter(|d| !d.starts_with('-') || d[1..].bytes().all(|b| b == b'0'))
                .ok_or_else(|| {
                    error(
                        INVALID_ACCURACY,
                        "precision must be a non-negative integer or INF",
                        start.span,
                    )
                })?;
            let parsed = digits
                .trim_start_matches(['+', '-'])
                .parse::<u64>()
                .map_err(|_| {
                    error(
                        UNSUPPORTED,
                        "precision integer exceeds u64 range",
                        start.span,
                    )
                })?;
            Ok(Some(Accuracy::Precision(parsed)))
        }
    }
}

/// XSD `integer` lexical form: one optional sign followed by one or more
/// ASCII digits. Returns the input when valid.
fn xsd_integer(value: &str) -> Option<&str> {
    let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
    (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())).then_some(value)
}
