//! Local DTS discovery, schema metadata and common linkbase graphs.
//!
//! # Specification versions
//!
//! * XBRL 2.1 Recommendation 2003-12-31 with errata through 2013-02-20,
//!   §§3–7 (DTS, schema references, linkbases, labels, presentation and
//!   calculation): <https://www.xbrl.org/specification/xbrl-2.1/>
//! * XML Schema 1.0 Part 1 (2nd ed., 2004-10-28) §§3.2–3.4 — element and type
//!   declarations, `xs:include` / `xs:import`.
//! * XLink 1.0 (2001-06-27) — extended links, locators, arcs and resources.
//! * XML Base (2001-06-27), RFC 3986 (2005-01), XML Catalogs OASIS 1.1
//!   (2005-10-07) — URI resolution through the existing local resolver.
//!
//! # Scope and safety
//!
//! Only local files below the configured root are read; remote references are
//! blocked by [`xbrl_xml::resolver::Resolver`] unless a catalog maps them to
//! local files. Discovery has document/item budgets and uses a visited set for
//! termination (circular `xs:include`/`xs:import` is legal — XML Schema 1.0
//! §4.2.1/4.2.3). Every failure is a typed [`Diagnostic`] with a stable rule id
//! from [`crate::rules`]. Graphs are grouped by extended-link role (ELR), arcrole
//! and arc element name; cycle detection and traversal never cross networks,
//! and traversal order is deterministic (numeric `order`, then expanded name).
//! Unsupported arc attributes are reported, not silently accepted. Relationship
//! equivalence and priority-based arc override (`priority` values other than
//! 0/1) are not implemented; those inputs fail closed with `UNSUPPORTED`. No
//! validation of facts or filing profiles occurs here.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Cursor;
use std::path::PathBuf;

use xbrl_core::diagnostic::{Diagnostic, RuleId, Severity, SourceSpan};
use xbrl_core::namespaces::{XBRL_INSTANCE, XBRL_LINKBASE, XLINK};
use xbrl_core::qname::QName;
use xbrl_xml::limits::Limits;
use xbrl_xml::reader::{StartElement, XmlEvent, XmlReader};
use xbrl_xml::resolver::{DocumentRoot, ResolvedFile, Resolver};
use xbrl_xml::uri::Uri;

pub use crate::model::LinkbaseKind;
use crate::model::{
    Concept, DiscoveredDocument, DocumentKind, Elr, GraphChild, Label, Relationship, TypeDefinition,
};
use crate::rules;

const XS: &str = "http://www.w3.org/2001/XMLSchema";
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
const STANDARD_LABEL_ROLE: &str = "http://www.xbrl.org/2003/role/label";
const PARENT_CHILD: &str = "http://www.xbrl.org/2003/arcrole/parent-child";
const SUMMATION_ITEM: &str = "http://www.xbrl.org/2003/arcrole/summation-item";
const CONCEPT_LABEL: &str = "http://www.xbrl.org/2003/arcrole/concept-label";

fn diagnostic(rule: &str, message: impl Into<String>, span: Option<SourceSpan>) -> Diagnostic {
    Diagnostic::new(
        RuleId::new(rule).expect("a static rule id"),
        Severity::Error,
        message,
        span,
    )
}

/// Resource budgets for one DTS discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DtsLimits {
    /// Maximum number of local documents read, entry included. Default: 256.
    pub max_documents: usize,
    /// Maximum combined concepts, types, labels and relationships. Default: 100_000.
    pub max_items: usize,
    /// Maximum bytes per document. Default: 64 MiB (matches the XML resolver).
    pub max_document_bytes: u64,
}

impl Default for DtsLimits {
    fn default() -> Self {
        Self {
            max_documents: 256,
            max_items: 100_000,
            max_document_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Builder that discovers one DTS from local files.
#[derive(Debug)]
pub struct DtsBuilder {
    resolver: Resolver,
    limits: DtsLimits,
}

impl DtsBuilder {
    /// Opens `root` as the sandbox for all DTS documents.
    ///
    /// # Errors
    ///
    /// [`xbrl_xml::rules::RESOLVER_ROOT_NOT_ACCESSIBLE`] when the path is not a
    /// readable directory.
    pub fn new(root: impl AsRef<std::path::Path>) -> Result<Self, Diagnostic> {
        Ok(Self {
            resolver: Resolver::new(DocumentRoot::new(root)?),
            limits: DtsLimits::default(),
        })
    }

    /// Replaces the discovery budgets.
    pub fn limits(&mut self, limits: DtsLimits) -> &mut Self {
        self.limits = limits;
        self
    }

    /// Loads an OASIS XML catalog mapping remote DTS names to local files.
    /// See [`Resolver::add_catalog_file`].
    ///
    /// # Errors
    ///
    /// Propagates resolver/catalog diagnostics.
    pub fn add_catalog_file(
        &mut self,
        reference: &str,
        base: Option<&Uri>,
    ) -> Result<(), Diagnostic> {
        self.resolver.add_catalog_file(reference, base)
    }

    /// Discovers a DTS starting at `entry` (a reference inside the root).
    ///
    /// # Errors
    ///
    /// Typed diagnostics for unreadable references, remote references without a
    /// catalog mapping, duplicate ids/concepts, unresolved linkbase locators,
    /// cyclic linkbase graphs within one network, unsupported constructs and
    /// exhausted budgets. Circular schema include/import terminates normally.
    pub fn discover(&self, entry: &str) -> Result<Dts, Diagnostic> {
        let mut state = Discovery {
            builder: self,
            documents: Vec::new(),
            visited: HashSet::new(),
            parsed: Vec::new(),
            aliases: HashMap::new(),
        };
        state.visit(entry, None)?;
        state.finish()
    }
}

/// Internal discovery state: a deterministic depth-first walk in reference
/// order. XML Schema 1.0 §4.2.1/4.2.3 permits circular include/import, so the
/// visited set both terminates the walk and removes duplicate documents.
struct Discovery<'a> {
    builder: &'a DtsBuilder,
    documents: Vec<DiscoveredDocument>,
    visited: HashSet<PathBuf>,
    parsed: Vec<(String, ParsedDocument)>,
    /// Every absolute document URI that resolved to a given local path,
    /// including catalog-rewrite source URIs and aliases seen only in later
    /// references. XML IDs are indexed under all of them so locator hrefs
    /// naming the remote name resolve to the same discovered schema
    /// (XML Catalogs 1.1, XBRL 2.1 §3.5.3.7.2).
    aliases: HashMap<PathBuf, HashSet<String>>,
}

impl Discovery<'_> {
    fn visit(&mut self, reference: &str, base: Option<&Uri>) -> Result<(), Diagnostic> {
        let file = self
            .builder
            .resolver
            .resolve(reference, base)
            .map_err(|error| {
                if error.rule().as_str() == xbrl_xml::rules::RESOLVER_NOT_FOUND {
                    diagnostic(
                        rules::REFERENCE_NOT_FOUND,
                        error.message().to_owned(),
                        error.span(),
                    )
                } else {
                    error
                }
            })?;
        let path = file.path.clone();
        // Record both names of this document. An extension linkbase may name
        // a catalog-mapped schema by its remote URI even if discovery first
        // reached it through the mapped local name (or vice versa).
        {
            let names = self.aliases.entry(path.clone()).or_default();
            names.insert(file.requested.without_fragment().to_string());
            names.insert(file.mapped.without_fragment().to_string());
        }
        if self.visited.insert(path.clone()) {
            if self.documents.len() >= self.builder.limits.max_documents {
                return Err(diagnostic(
                    rules::LIMIT_DOCUMENTS,
                    format!(
                        "DTS discovery exceeded its {} document limit",
                        self.builder.limits.max_documents
                    ),
                    None,
                ));
            }
            let document_uri = file.mapped.clone().without_fragment().to_string();
            let (kind, parsed) = self.parse_document(&file)?;
            self.documents.push(DiscoveredDocument {
                path: path.clone(),
                kind,
                uri: document_uri.clone(),
            });
            self.parsed.push((document_uri, parsed.clone()));
            match parsed {
                ParsedDocument::Schema(schema) => schema
                    .references
                    .into_iter()
                    .try_for_each(|r| self.visit(&r.href, Some(&r.base))),
                ParsedDocument::Linkbase(linkbase) => linkbase
                    .references
                    .into_iter()
                    .try_for_each(|r| self.visit(&r.href, Some(&r.base))),
            }?;
        }
        Ok(())
    }

    fn parse_document(
        &self,
        file: &ResolvedFile,
    ) -> Result<(DocumentKind, ParsedDocument), Diagnostic> {
        let reader_limits = Limits {
            max_document_bytes: self.builder.limits.max_document_bytes,
            ..Limits::default()
        };
        let mut reader = XmlReader::new(Cursor::new(&file.bytes), reader_limits);
        let mut namespaces: Vec<Vec<(Option<String>, String)>> = Vec::new();
        // Ancestor xml:base values, outermost first. The entry base is the
        // absolute URI the resolver read, so relative refs are unambiguous.
        let mut bases: Vec<Uri> = vec![file.mapped.clone().without_fragment()];
        let first = reader
            .next_event()?
            .ok_or_else(|| diagnostic(rules::INVALID_ROOT, "document has no root element", None))?;
        let XmlEvent::Start(root) = first else {
            return Err(diagnostic(
                rules::INVALID_ROOT,
                "DTS document must start with an element",
                None,
            ));
        };
        if root.name.namespace() == Some(XS) && root.name.local_name() == "schema" {
            let schema = parse_schema(&mut reader, root, &mut namespaces, &mut bases)?;
            return Ok((DocumentKind::Schema, ParsedDocument::Schema(schema)));
        }
        if root.name.namespace() == Some(XBRL_LINKBASE) && root.name.local_name() == "linkbase" {
            let linkbase = parse_linkbase(&mut reader, root, &mut namespaces, &mut bases)?;
            return Ok((DocumentKind::Linkbase, ParsedDocument::Linkbase(linkbase)));
        }
        Err(diagnostic(
            rules::INVALID_ROOT,
            format!(
                "expected xs:schema or link:linkbase root, found {{{}}}{}",
                root.name.namespace().unwrap_or(""),
                root.name.local_name()
            ),
            Some(root.span),
        ))
    }

    // The aggregator is built incrementally from bounded document parses; a
    // literal would duplicate the full field list across the error paths.
    #[allow(clippy::field_reassign_with_default)]
    fn finish(self) -> Result<Dts, Diagnostic> {
        let mut dts = Dts::default();
        dts.documents = self.documents.clone();
        let max = self.builder.limits.max_items;
        let mut count = 0_usize;

        // XML IDs are document-scoped (XML 1.0 §3.3.1), so index them by the
        // canonical local document, not by the URI used to name it. All URI
        // aliases (catalog source, mapped file URI, relative or absolute href)
        // are resolved back to that path before lookup.
        let mut ids: HashMap<(PathBuf, String), Option<QName>> = HashMap::new();
        let mut alias_paths: HashMap<String, PathBuf> = HashMap::new();
        let mut concepts: BTreeMap<String, Concept> = BTreeMap::new();
        let mut types: BTreeMap<String, TypeDefinition> = BTreeMap::new();
        for ((uri, parsed), document) in self.parsed.iter().zip(&self.documents) {
            let ParsedDocument::Schema(schema) = parsed else {
                continue;
            };
            // Every name that resolved to this schema (the mapped file URI,
            // catalog source URIs, and relative/absolute aliases) addresses
            // the same document-scoped ID table.
            for alias in self.aliases[&document.path]
                .iter()
                .chain(std::iter::once(uri))
            {
                alias_paths.insert(alias.clone(), document.path.clone());
            }
            for (id, name) in &schema.ids {
                count += 1;
                check_limit(count, max)?;
                if ids
                    .insert((document.path.clone(), id.clone()), name.clone())
                    .is_some()
                {
                    return Err(diagnostic(
                        rules::DUPLICATE_ID,
                        format!("XML ID {id:?} is declared more than once in {uri}"),
                        None,
                    ));
                }
            }
            for concept in &schema.concepts {
                count += 1;
                check_limit(count, max)?;
                if concepts
                    .insert(concept.name.to_clark(), concept.clone())
                    .is_some()
                {
                    return Err(diagnostic(
                        rules::DUPLICATE_CONCEPT,
                        format!(
                            "concept {} is declared more than once",
                            concept.name.to_clark()
                        ),
                        concept.span,
                    ));
                }
            }
            for ty in &schema.types {
                count += 1;
                check_limit(count, max)?;
                if types.insert(ty.name.to_clark(), ty.clone()).is_some() {
                    return Err(diagnostic(
                        rules::DUPLICATE_TYPE,
                        format!("type {} is declared more than once", ty.name.to_clark()),
                        ty.span,
                    ));
                }
            }
        }
        // Resolve a locator href against the linkbase's in-scope xml:base and
        // look the id up in the target document only (XBRL 2.1 §3.5.3.7.2).
        let resolve_loc = |loc: &ParsedLocator| -> Result<QName, Diagnostic> {
            let target = Uri::parse(&loc.href)
                .and_then(|reference| loc.base.resolve(&reference))
                .map_err(|error| {
                    diagnostic(
                        rules::INVALID_STRUCTURE,
                        error.message().to_owned(),
                        Some(loc.span),
                    )
                })?;
            let Some(id) = target.fragment() else {
                return Err(locator_error(&loc.href, loc.span));
            };
            let document = target.without_fragment().to_string();
            let Some(path) = alias_paths.get(&document) else {
                return Err(locator_error(&loc.href, loc.span));
            };
            match ids.get(&(path.clone(), id.to_owned())) {
                Some(Some(name)) => Ok(name.clone()),
                Some(None) => Err(locator_error(&loc.href, loc.span)),
                None => Err(locator_error(&loc.href, loc.span)),
            }
        };

        // Resolve linkbase links. XLink arc endpoints may name either locators
        // or resources (XBRL 2.1 §5.2.2.3, XLink 1.0 §5.1.3); for labels the
        // association exists only where a concept-label arc connects them.
        let mut relationships = Vec::new();
        for (_uri, parsed) in &self.parsed {
            let ParsedDocument::Linkbase(linkbase) = parsed else {
                continue;
            };
            for link in &linkbase.links {
                // XLink arcs address every resource sharing an endpoint
                // label (XLink 1.0 §5.1.3); keep all locators in document
                // order so a shared label fans out deterministically.
                let mut locs: BTreeMap<&str, Vec<QName>> = BTreeMap::new();
                for loc in &link.locs {
                    locs.entry(loc.label.as_str())
                        .or_default()
                        .push(resolve_loc(loc)?);
                }
                let mut resources: BTreeMap<&str, Vec<&ParsedLabelResource>> = BTreeMap::new();
                for resource in &link.resources {
                    resources
                        .entry(resource.label.as_str())
                        .or_default()
                        .push(resource);
                }
                let is_endpoint = |label: &str| -> bool {
                    locs.contains_key(label) || resources.contains_key(label)
                };
                // Label associations come only from concept-label arcs
                // (XBRL 2.1 §5.2.2.3); an orphan resource is not a label.
                for arc in &link.arcs {
                    // use="prohibited" and non-default priority change the
                    // network (XBRL 2.1 §3.5.3.9.7.4); accepting them as plain
                    // edges would report relationships the taxonomy forbids.
                    if arc.prohibited || arc.priority {
                        return Err(arc_error(
                            rules::UNSUPPORTED,
                            format!(
                                "arc `use`={:?}/priority={:?} is not supported",
                                arc.use_value, arc.priority_value
                            ),
                            arc.span,
                        ));
                    }
                    if arc.kind == LinkbaseKind::Label {
                        if arc.arcrole != CONCEPT_LABEL {
                            continue;
                        }
                        // XLink arc endpoints name either a locator or a
                        // resource; an unknown label is a typed error.
                        for endpoint in [&arc.from, &arc.to] {
                            if !is_endpoint(endpoint) {
                                return Err(arc_error(
                                    rules::ARC_UNRESOLVED,
                                    format!(
                                        "arc xlink endpoint {endpoint:?} has no locator or resource"
                                    ),
                                    arc.span,
                                ));
                            }
                        }
                        for (label, concepts) in &locs {
                            if *label != arc.from {
                                continue;
                            }
                            for concept in concepts {
                                for resource in resources.get(arc.to.as_str()).into_iter().flatten()
                                {
                                    count += 1;
                                    check_limit(count, max)?;
                                    dts.labels.push(Label {
                                        concept: concept.clone(),
                                        role: resource.role.clone(),
                                        lang: resource.lang.clone(),
                                        text: resource.text.clone(),
                                    });
                                }
                            }
                        }
                        continue;
                    }
                    let Some(froms) = locs.get(arc.from.as_str()) else {
                        return Err(arc_error(
                            rules::ARC_UNRESOLVED,
                            format!("arc xlink:from {:?} has no locator", arc.from),
                            arc.span,
                        ));
                    };
                    let Some(tos) = locs.get(arc.to.as_str()) else {
                        return Err(arc_error(
                            rules::ARC_UNRESOLVED,
                            format!("arc xlink:to {:?} has no locator", arc.to),
                            arc.span,
                        ));
                    };
                    for from in froms {
                        for to in tos {
                            count += 1;
                            check_limit(count, max)?;
                            relationships.push(Relationship {
                                from: from.clone(),
                                to: to.clone(),
                                kind: arc.kind,
                                elr: link.elr.clone(),
                                order: arc.order,
                                weight: arc.weight.unwrap_or(1.0),
                                preferred_label: arc.preferred_label.clone(),
                                arcrole: arc.arcrole.clone(),
                                span: Some(arc.span),
                            });
                        }
                    }
                }
            }
        }

        // A network is (kind, ELR, arcrole, arc element name). Cycle detection
        // is per network: A→B in one ELR and B→A in another is legal (XBRL 2.1
        // §3.5.3.9.7.3). Only parent-child presentation arcs must be acyclic;
        // summation-item explicitly allows cycles (§5.2.5.2).
        relationships.sort_by(|a, b| {
            (&a.kind, &a.elr, &a.arcrole, &a.from.to_clark())
                .cmp(&(&b.kind, &b.elr, &b.arcrole, &b.from.to_clark()))
                .then_with(|| a.order.total_cmp(&b.order))
                .then_with(|| a.to.to_clark().cmp(&b.to.to_clark()))
        });
        let mut adjacency: BTreeMap<(LinkbaseKind, Elr, String), BTreeMap<String, Vec<String>>> =
            BTreeMap::new();
        for edge in relationships
            .iter()
            .filter(|r| r.kind == LinkbaseKind::Presentation && r.arcrole == PARENT_CHILD)
        {
            adjacency
                .entry((edge.kind, edge.elr.clone(), edge.arcrole.clone()))
                .or_default()
                .entry(edge.from.to_clark())
                .or_default()
                .push(edge.to.to_clark());
        }
        for ((_, elr, _), graph) in &adjacency {
            let mut state: HashMap<String, u8> = HashMap::new();
            for node in graph.keys() {
                if state.get(node).copied().unwrap_or(0) == 0 {
                    detect_cycle(node, graph, &mut state, elr)?;
                }
            }
        }
        dts.relationships = relationships;
        dts.concepts = concepts.into_values().collect();
        dts.concept_index = dts
            .concepts
            .iter()
            .enumerate()
            .map(|(i, concept)| (concept.name.to_clark(), i))
            .collect();
        dts.types = types.into_values().collect();
        dts.labels.sort_by(|a, b| {
            (&a.concept.to_clark(), &a.role, &a.lang, &a.text).cmp(&(
                &b.concept.to_clark(),
                &b.role,
                &b.lang,
                &b.text,
            ))
        });
        dts.labels.dedup();
        Ok(dts)
    }
}

fn check_limit(count: usize, max: usize) -> Result<(), Diagnostic> {
    if count > max {
        Err(diagnostic(
            rules::LIMIT_ITEMS,
            format!("DTS item count exceeds the {max} item limit"),
            None,
        ))
    } else {
        Ok(())
    }
}

fn locator_error(href: &str, span: SourceSpan) -> Diagnostic {
    arc_error(
        rules::LOCATOR_UNRESOLVED,
        format!("locator href {href:?} has no matching XML ID in the discovered DTS"),
        span,
    )
}

fn arc_error(rule: &str, message: impl Into<String>, span: SourceSpan) -> Diagnostic {
    diagnostic(rule, message, Some(span))
}

/// Iterative depth-first cycle check. A linear chain well inside `max_items`
/// must return a typed diagnostic / success, never overflow the process stack.
/// `state`: 0/absent = unvisited, 1 = on the current stack, 2 = done.
fn detect_cycle(
    root: &str,
    adjacency: &BTreeMap<String, Vec<String>>,
    state: &mut HashMap<String, u8>,
    elr: &str,
) -> Result<(), Diagnostic> {
    if state.get(root).copied().unwrap_or(0) != 0 {
        return Ok(());
    }
    // Each frame is (node, index of the next child to examine). Keeping the
    // stack on the heap makes depth bounded by the item budget, not by the
    // native call stack.
    let mut stack: Vec<(String, usize)> = vec![(root.to_owned(), 0)];
    state.insert(root.to_owned(), 1);
    while let Some((node, next)) = stack.last_mut() {
        let children = adjacency.get(node.as_str());
        let child = children.and_then(|children| children.get(*next));
        if let Some(child) = child {
            *next += 1;
            match state.get(child.as_str()).copied().unwrap_or(0) {
                1 => {
                    return Err(diagnostic(
                        rules::GRAPH_CYCLE,
                        format!("presentation graph has a cycle through {node} in ELR {elr}"),
                        None,
                    ));
                }
                2 => {}
                _ => {
                    state.insert(child.clone(), 1);
                    stack.push((child.clone(), 0));
                }
            }
        } else {
            state.insert(node.clone(), 2);
            stack.pop();
        }
    }
    Ok(())
}

/// A fully discovered DTS. Query methods never parse or read files.
#[derive(Debug, Default)]
pub struct Dts {
    documents: Vec<DiscoveredDocument>,
    concepts: Vec<Concept>,
    concept_index: HashMap<String, usize>,
    types: Vec<TypeDefinition>,
    labels: Vec<Label>,
    relationships: Vec<Relationship>,
}

impl Dts {
    /// Documents in deterministic discovery order (entry first).
    #[must_use]
    pub fn documents(&self) -> &[DiscoveredDocument] {
        &self.documents
    }

    /// Concepts sorted by expanded name.
    #[must_use]
    pub fn concepts(&self) -> &[Concept] {
        &self.concepts
    }

    /// One concept by expanded name.
    #[must_use]
    pub fn concept(&self, name: &QName) -> Option<&Concept> {
        self.concept_index
            .get(&name.to_clark())
            .and_then(|i| self.concepts.get(*i))
    }

    /// Schema type definitions sorted by expanded name.
    #[must_use]
    pub fn types(&self) -> &[TypeDefinition] {
        &self.types
    }

    /// All labels, sorted by concept, role, language and text.
    #[must_use]
    pub fn all_labels(&self) -> &[Label] {
        &self.labels
    }

    /// Labels of one concept, sorted by role, language and text.
    #[must_use]
    pub fn label_list(&self, concept: &QName) -> Vec<&Label> {
        self.labels
            .iter()
            .filter(|label| &label.concept == concept)
            .collect()
    }

    /// Labels of one concept (owned clones for API simplicity).
    #[must_use]
    pub fn labels(&self, concept: &QName) -> Vec<Label> {
        self.label_list(concept).into_iter().cloned().collect()
    }

    /// Label text for `role`, falling back to the standard label role.
    ///
    /// `preferred` is the `preferredLabel` role from a presentation arc.
    #[must_use]
    pub fn preferred_label(&self, concept: &QName, preferred: Option<&str>) -> Option<&str> {
        let roles: [&str; 2] = [
            preferred.unwrap_or(STANDARD_LABEL_ROLE),
            STANDARD_LABEL_ROLE,
        ];
        roles.iter().find_map(|role| {
            self.labels
                .iter()
                .find(|label| &label.concept == concept && label.role == *role)
                .map(|label| label.text.as_str())
        })
    }

    /// Root concepts of a graph (nodes never used as an arc target), sorted by
    /// expanded name. Collects roots across all extended-link networks; use
    /// [`Dts::graph_roots_in_elr`] for one network.
    #[must_use]
    pub fn graph_roots(&self, kind: LinkbaseKind) -> Vec<GraphChild> {
        self.graph_roots_filtered(kind, None)
    }

    /// Root concepts of one extended-link network.
    #[must_use]
    pub fn graph_roots_in_elr(&self, kind: LinkbaseKind, elr: &str) -> Vec<GraphChild> {
        self.graph_roots_filtered(kind, Some(elr))
    }

    fn graph_roots_filtered(&self, kind: LinkbaseKind, elr: Option<&str>) -> Vec<GraphChild> {
        let in_network = |r: &Relationship| r.kind == kind && elr.is_none_or(|e| r.elr == e);
        let targets: HashSet<String> = self
            .relationships
            .iter()
            .filter(|r| in_network(r))
            .map(|r| r.to.to_clark())
            .collect();
        let mut roots: Vec<GraphChild> = self
            .relationships
            .iter()
            .filter(|r| in_network(r) && !targets.contains(&r.from.to_clark()))
            .map(|edge| GraphChild::new(edge.from.clone(), edge.clone()))
            .collect();
        roots.sort_by(|a, b| {
            a.concept
                .to_clark()
                .cmp(&b.concept.to_clark())
                .then_with(|| a.edge.elr.cmp(&b.edge.elr))
        });
        roots.dedup_by(|a, b| a.concept == b.concept && a.edge.elr == b.edge.elr);
        roots
    }

    /// Presentation roots, the generic graph API with the kind fixed to
    /// [`LinkbaseKind::Presentation`]. Passing a kind here was a misleading
    /// second entry point into the same traversal.
    #[must_use]
    pub fn presentation_roots(&self) -> Vec<GraphChild> {
        self.graph_roots(LinkbaseKind::Presentation)
    }

    /// Presentation roots of one extended-link network.
    #[must_use]
    pub fn presentation_roots_in_elr(&self, elr: &str) -> Vec<GraphChild> {
        self.graph_roots_in_elr(LinkbaseKind::Presentation, elr)
    }

    /// Children of `concept` in a graph, sorted by numeric `order` then name.
    #[must_use]
    pub fn graph_children(&self, kind: LinkbaseKind, concept: &QName) -> Vec<GraphChild> {
        self.graph_children_filtered(kind, concept, None)
    }

    /// Children of `concept` in one extended-link network.
    #[must_use]
    pub fn graph_children_in_elr(
        &self,
        kind: LinkbaseKind,
        concept: &QName,
        elr: &str,
    ) -> Vec<GraphChild> {
        self.graph_children_filtered(kind, concept, Some(elr))
    }

    fn graph_children_filtered(
        &self,
        kind: LinkbaseKind,
        concept: &QName,
        elr: Option<&str>,
    ) -> Vec<GraphChild> {
        let mut edges: Vec<&Relationship> = self
            .relationships
            .iter()
            .filter(|r| r.kind == kind && &r.from == concept && elr.is_none_or(|e| r.elr == e))
            .collect();
        edges.sort_by(|a, b| {
            a.order
                .total_cmp(&b.order)
                .then_with(|| a.to.to_clark().cmp(&b.to.to_clark()))
                .then_with(|| a.arcrole.cmp(&b.arcrole))
                .then_with(|| a.elr.cmp(&b.elr))
        });
        edges
            .into_iter()
            .map(|edge| GraphChild::new(edge.to.clone(), edge.clone()))
            .collect()
    }

    /// Presentation children of `concept`, the generic graph API with the kind
    /// fixed to [`LinkbaseKind::Presentation`].
    #[must_use]
    pub fn presentation_children(&self, concept: &QName) -> Vec<GraphChild> {
        self.graph_children(LinkbaseKind::Presentation, concept)
    }

    /// Presentation children of `concept` in one extended-link network.
    #[must_use]
    pub fn presentation_children_in_elr(&self, concept: &QName, elr: &str) -> Vec<GraphChild> {
        self.graph_children_in_elr(LinkbaseKind::Presentation, concept, elr)
    }

    /// Calculation summands of `concept`, sorted by numeric `order`.
    #[must_use]
    pub fn calculation_summands(&self, concept: &QName) -> Vec<GraphChild> {
        self.graph_children(LinkbaseKind::Calculation, concept)
    }

    /// Calculation summands of `concept` in one extended-link network.
    #[must_use]
    pub fn calculation_summands_in_elr(&self, concept: &QName, elr: &str) -> Vec<GraphChild> {
        self.graph_children_in_elr(LinkbaseKind::Calculation, concept, elr)
    }

    /// All discovered relationships in deterministic order.
    #[must_use]
    pub fn relationships(&self) -> &[Relationship] {
        &self.relationships
    }
}

impl PartialEq for Dts {
    fn eq(&self, other: &Self) -> bool {
        self.documents == other.documents
            && self.concepts == other.concepts
            && self.types == other.types
            && self.labels == other.labels
            && self.relationships == other.relationships
    }
}
impl Eq for Dts {}

#[derive(Debug, Clone)]
enum ParsedDocument {
    Schema(ParsedSchema),
    Linkbase(ParsedLinkbase),
}

#[derive(Debug, Clone)]
struct Reference {
    href: String,
    base: Uri,
}

#[derive(Debug, Clone, Default)]
struct ParsedSchema {
    references: Vec<Reference>,
    concepts: Vec<Concept>,
    types: Vec<TypeDefinition>,
    /// Document-scoped XML IDs. `None` means an id on a nested element that
    /// does not name a top-level concept or type (not a valid arc target).
    ids: HashMap<String, Option<QName>>,
}

#[derive(Debug, Clone, Default)]
struct ParsedLinkbase {
    references: Vec<Reference>,
    links: Vec<ParsedExtendedLink>,
}

#[derive(Debug, Clone, Default)]
struct ParsedExtendedLink {
    elr: String,
    locs: Vec<ParsedLocator>,
    resources: Vec<ParsedLabelResource>,
    arcs: Vec<ParsedArc>,
}

#[derive(Debug, Clone)]
struct ParsedLocator {
    label: String,
    href: String,
    /// In-scope base at the locator, used to resolve the href document.
    base: Uri,
    span: SourceSpan,
}

#[derive(Debug, Clone)]
struct ParsedLabelResource {
    label: String,
    role: String,
    lang: Option<String>,
    text: String,
}

#[derive(Debug, Clone)]
struct ParsedArc {
    from: String,
    to: String,
    kind: LinkbaseKind,
    arcrole: String,
    order: f64,
    weight: Option<f64>,
    preferred_label: Option<String>,
    use_value: String,
    priority_value: Option<String>,
    prohibited: bool,
    priority: bool,
    span: SourceSpan,
}

fn attr<'a>(element: &'a StartElement, namespace: Option<&str>, local: &str) -> Option<&'a str> {
    element
        .attributes
        .iter()
        .find(|a| a.name.namespace() == namespace && a.name.local_name() == local)
        .map(|a| a.value.as_str())
}

fn bool_attr(element: &StartElement, local: &str) -> bool {
    attr(element, None, local) == Some("true") || attr(element, None, local) == Some("1")
}

fn push_bindings(
    stack: &mut Vec<Vec<(Option<String>, String)>>,
    element: &StartElement,
    bases: &mut Vec<Uri>,
) -> Result<(), Diagnostic> {
    let bindings: Vec<_> = element
        .namespaces
        .iter()
        .map(|b| (b.prefix().map(str::to_owned), b.uri().to_owned()))
        .collect();
    stack.push(bindings);
    if let Some(base) = attr(element, Some(XML_NS), "base") {
        let current = bases
            .last()
            .expect("the entry base is always present")
            .resolve(&Uri::parse(base)?)?;
        bases.push(current);
    } else {
        let current = bases
            .last()
            .expect("the entry base is always present")
            .clone();
        bases.push(current);
    }
    Ok(())
}

fn pop_bindings(stack: &mut Vec<Vec<(Option<String>, String)>>, bases: &mut Vec<Uri>) {
    stack.pop();
    if bases.len() > 1 {
        bases.pop();
    }
}

fn resolve_qname(
    lexical: &str,
    stack: &[Vec<(Option<String>, String)>],
    span: SourceSpan,
) -> Result<QName, Diagnostic> {
    let (prefix, local) = match lexical.split_once(':') {
        Some((prefix, local)) => (Some(prefix), local),
        None => (None, lexical),
    };
    let namespace = stack
        .iter()
        .rev()
        .flat_map(|frame| frame.iter().rev())
        .find(|(bound, _)| bound.as_deref() == prefix)
        .map(|(_, uri)| uri.as_str())
        .ok_or_else(|| {
            diagnostic(
                rules::INVALID_QNAME,
                format!("QName {lexical:?} uses an unbound prefix"),
                Some(span),
            )
        })?;
    QName::new(prefix, namespace, local)
        .map_err(|e| diagnostic(rules::INVALID_QNAME, e.to_string(), Some(span)))
}

fn reference(href: &str, bases: &[Uri], span: SourceSpan) -> Result<Reference, Diagnostic> {
    let base = bases
        .last()
        .expect("the entry base is always present")
        .clone();
    // Validate the reference early so a malformed href is a typed error at the
    // element that declared it rather than a failure deep in the resolver.
    Uri::parse(href)
        .map_err(|e| diagnostic(rules::INVALID_STRUCTURE, e.to_string(), Some(span)))?;
    Ok(Reference {
        href: href.to_owned(),
        base,
    })
}

fn insert_id(
    schema: &mut ParsedSchema,
    id: &str,
    name: Option<QName>,
    span: SourceSpan,
) -> Result<(), Diagnostic> {
    if schema.ids.insert(id.to_owned(), name).is_some() {
        return Err(diagnostic(
            rules::DUPLICATE_ID,
            format!("XML ID {id:?} is declared more than once in one schema"),
            Some(span),
        ));
    }
    Ok(())
}

fn parse_schema<R: std::io::Read>(
    reader: &mut XmlReader<R>,
    root: StartElement,
    namespaces: &mut Vec<Vec<(Option<String>, String)>>,
    bases: &mut Vec<Uri>,
) -> Result<ParsedSchema, Diagnostic> {
    let target_namespace = attr(&root, None, "targetNamespace")
        .unwrap_or("")
        .to_owned();
    let mut schema = ParsedSchema::default();
    push_bindings(namespaces, &root, bases)?;
    // An id on the schema document root does not name a concept/type: arc
    // locators targeting it must fail closed, not resolve to a fabricated
    // {targetNamespace}schema concept.
    if let Some(id) = attr(&root, None, "id") {
        insert_id(&mut schema, id, None, root.span)?;
    }
    let mut depth = 1_usize;
    let mut in_appinfo = false;
    let mut active_type: Option<usize> = None;
    while let Some(event) = reader.next_event()? {
        match event {
            XmlEvent::Start(start) => {
                depth += 1;
                push_bindings(namespaces, &start, bases)?;
                let is_xs = start.name.namespace() == Some(XS);
                let local = start.name.local_name();
                if is_xs && local == "appinfo" && depth >= 3 {
                    in_appinfo = true;
                }
                if in_appinfo
                    && start.name.namespace() == Some(XBRL_LINKBASE)
                    && local == "linkbaseRef"
                {
                    let href = attr(&start, Some(XLINK), "href").ok_or_else(|| {
                        diagnostic(
                            rules::INVALID_STRUCTURE,
                            "link:linkbaseRef requires xlink:href",
                            Some(start.span),
                        )
                    })?;
                    schema.references.push(reference(href, bases, start.span)?);
                }
                if is_xs && depth == 2 && matches!(local, "include" | "import" | "redefine") {
                    if let Some(href) = attr(&start, None, "schemaLocation") {
                        schema.references.push(reference(href, bases, start.span)?);
                    }
                }
                let id = attr(&start, None, "id").or_else(|| attr(&start, Some(XML_NS), "id"));
                if is_xs && depth == 2 && local == "element" {
                    let name_local = attr(&start, None, "name").ok_or_else(|| {
                        diagnostic(
                            rules::UNSUPPORTED,
                            "xs:element without name (ref) is not supported",
                            Some(start.span),
                        )
                    })?;
                    let name = QName::new(None, &target_namespace, name_local).map_err(|e| {
                        diagnostic(rules::INVALID_QNAME, e.to_string(), Some(start.span))
                    })?;
                    let type_name = match attr(&start, None, "type") {
                        Some(lexical) => resolve_qname(lexical, namespaces, start.span)?,
                        None => QName::new(None, XS, "anyType").expect("static qname"),
                    };
                    let substitution_group = attr(&start, None, "substitutionGroup")
                        .map(|lexical| resolve_qname(lexical, namespaces, start.span))
                        .transpose()?;
                    let period_type =
                        attr(&start, Some(XBRL_INSTANCE), "periodType").map(str::to_owned);
                    schema.concepts.push(Concept {
                        name: name.clone(),
                        id: id.map(str::to_owned),
                        type_name,
                        substitution_group,
                        is_abstract: bool_attr(&start, "abstract"),
                        nillable: bool_attr(&start, "nillable"),
                        period_type,
                        span: Some(start.span),
                    });
                }
                if is_xs && depth == 2 && matches!(local, "complexType" | "simpleType") {
                    let name_local = attr(&start, None, "name").ok_or_else(|| {
                        diagnostic(
                            rules::UNSUPPORTED,
                            "anonymous xs:type definitions are not supported",
                            Some(start.span),
                        )
                    })?;
                    let name = QName::new(None, &target_namespace, name_local).map_err(|e| {
                        diagnostic(rules::INVALID_QNAME, e.to_string(), Some(start.span))
                    })?;
                    schema.types.push(TypeDefinition {
                        name: name.clone(),
                        id: id.map(str::to_owned),
                        category: if local == "complexType" {
                            "complexType"
                        } else {
                            "simpleType"
                        },
                        derivation: None,
                        base: None,
                        span: Some(start.span),
                    });
                    active_type = Some(schema.types.len() - 1);
                }
                if is_xs
                    && matches!(local, "extension" | "restriction")
                    && let Some(index) = active_type
                {
                    let ty = &mut schema.types[index];
                    ty.derivation = Some(local.to_owned());
                    ty.base = attr(&start, None, "base").map(str::to_owned);
                }
                if let Some(id) = id {
                    // Only a top-level element id names a concept and may be
                    // a linkbase arc target. Named types remain in the type
                    // table but their ids are not arc targets (XBRL 2.1
                    // §5.2.2.3/§5.2.4.2/§5.2.5.2); every other id is retained
                    // for uniqueness checks only (XBRL 2.1 §3.2).
                    let name = schema
                        .concepts
                        .last()
                        .filter(|c| c.id.as_deref() == Some(id))
                        .map(|c| c.name.clone());
                    insert_id(&mut schema, id, name, start.span)?;
                }
            }
            XmlEvent::End(_) => {
                if in_appinfo && depth == 3 {
                    in_appinfo = false;
                }
                if depth == 2 {
                    active_type = None;
                }
                pop_bindings(namespaces, bases);
                depth -= 1;
            }
            XmlEvent::Text(_) => {}
        }
    }
    Ok(schema)
}

fn parse_linkbase<R: std::io::Read>(
    reader: &mut XmlReader<R>,
    root: StartElement,
    namespaces: &mut Vec<Vec<(Option<String>, String)>>,
    bases: &mut Vec<Uri>,
) -> Result<ParsedLinkbase, Diagnostic> {
    let mut linkbase = ParsedLinkbase::default();
    push_bindings(namespaces, &root, bases)?;
    let mut depth = 1_usize;
    // Depth of the current extended link (None outside one).
    let mut extended: Option<usize> = None;
    // Depth of the current label resource and the text being accumulated.
    let mut label_resource: Option<(usize, usize, ParsedLabelResource)> = None;
    while let Some(event) = reader.next_event()? {
        match event {
            XmlEvent::Start(start) => {
                depth += 1;
                push_bindings(namespaces, &start, bases)?;
                let in_linkbase = start.name.namespace() == Some(XBRL_LINKBASE);
                let local = start.name.local_name();
                if in_linkbase
                    && depth == 2
                    && matches!(local, "labelLink" | "presentationLink" | "calculationLink")
                {
                    if extended.is_some() {
                        return Err(diagnostic(
                            rules::UNSUPPORTED,
                            "nested extended links are not supported",
                            Some(start.span),
                        ));
                    }
                    extended = Some(depth);
                    linkbase.links.push(ParsedExtendedLink {
                        // The extended link role identifies the network
                        // (XBRL 2.1 §3.5.3.9.7.3); arcs in different ELRs are
                        // never joined when building graphs.
                        elr: attr(&start, Some(XLINK), "role")
                            .unwrap_or("http://www.xbrl.org/2003/role/link")
                            .to_owned(),
                        ..ParsedExtendedLink::default()
                    });
                }
                let in_extended = extended.is_some_and(|d| d < depth);
                if in_extended {
                    let link_index = linkbase.links.len() - 1;
                    if in_linkbase && local == "loc" {
                        let label = required_xlink(&start, "label")?.to_owned();
                        let href = required_xlink(&start, "href")?.to_owned();
                        let base = bases
                            .last()
                            .expect("the entry base is always present")
                            .clone();
                        linkbase.links[link_index].locs.push(ParsedLocator {
                            label,
                            href,
                            base,
                            span: start.span,
                        });
                    } else if in_linkbase && local == "label" {
                        let label = required_xlink(&start, "label")?.to_owned();
                        let role = attr(&start, Some(XLINK), "role")
                            .unwrap_or(STANDARD_LABEL_ROLE)
                            .to_owned();
                        let lang = attr(&start, Some(XML_NS), "lang").map(str::to_owned);
                        label_resource = Some((
                            depth,
                            linkbase.links.len() - 1,
                            ParsedLabelResource {
                                label,
                                role,
                                lang,
                                text: String::new(),
                            },
                        ));
                    } else if in_linkbase
                        && matches!(local, "labelArc" | "presentationArc" | "calculationArc")
                    {
                        let kind = match local {
                            "labelArc" => LinkbaseKind::Label,
                            "presentationArc" => LinkbaseKind::Presentation,
                            _ => LinkbaseKind::Calculation,
                        };
                        let order = attr(&start, None, "order")
                            .map(parse_decimal)
                            .transpose()?
                            .unwrap_or(1.0);
                        let weight = attr(&start, None, "weight")
                            .map(parse_decimal)
                            .transpose()?;
                        // XBRL 2.1 §3.5.3.9.7.4 defines `use` and `priority`
                        // as unprefixed attributes on xl:arcType
                        // (xl-2003-12-31.xsd); XLink 1.0 has no `use`.
                        let use_value = attr(&start, None, "use").unwrap_or("normal").to_owned();
                        let priority_value = attr(&start, None, "priority").map(str::to_owned);
                        let prohibited = use_value == "prohibited";
                        let priority = priority_value
                            .as_deref()
                            .is_some_and(|v| v != "0" && v != "1");
                        linkbase.links[link_index].arcs.push(ParsedArc {
                            from: required_xlink(&start, "from")?.to_owned(),
                            to: required_xlink(&start, "to")?.to_owned(),
                            kind,
                            arcrole: attr(&start, Some(XLINK), "arcrole")
                                .unwrap_or(match kind {
                                    LinkbaseKind::Presentation => PARENT_CHILD,
                                    LinkbaseKind::Calculation => SUMMATION_ITEM,
                                    LinkbaseKind::Label => CONCEPT_LABEL,
                                })
                                .to_owned(),
                            order,
                            weight,
                            preferred_label: attr(&start, None, "preferredLabel")
                                .map(str::to_owned),
                            use_value,
                            priority_value,
                            prohibited,
                            priority,
                            span: start.span,
                        });
                    }
                }
            }
            XmlEvent::Text(text) => {
                if let Some((_, _, resource)) = &mut label_resource {
                    resource.text.push_str(&text.text);
                }
            }
            XmlEvent::End(_) => {
                if let Some((resource_depth, link_index, resource)) = label_resource.take() {
                    if resource_depth == depth {
                        linkbase.links[link_index].resources.push(resource);
                    } else {
                        label_resource = Some((resource_depth, link_index, resource));
                    }
                }
                if extended == Some(depth) {
                    extended = None;
                }
                pop_bindings(namespaces, bases);
                depth -= 1;
            }
        }
    }
    Ok(linkbase)
}

fn required_xlink<'a>(element: &'a StartElement, local: &str) -> Result<&'a str, Diagnostic> {
    attr(element, Some(XLINK), local).ok_or_else(|| {
        diagnostic(
            rules::INVALID_STRUCTURE,
            format!("an extended-link element requires xlink:{local}"),
            Some(element.span),
        )
    })
}

fn parse_decimal(value: &str) -> Result<f64, Diagnostic> {
    // xs:decimal (XML Schema 1.0 §3.2.3) is a finite base-10 number with no
    // exponent and no INF/NaN lexical forms. Parse strictly, then verify the
    // float is finite so overflow fails closed rather than becoming infinity.
    let text = value.trim();
    let valid = !text.is_empty()
        && text
            .strip_prefix(['+', '-'])
            .unwrap_or(text)
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.')
        && text.chars().filter(|&c| c == '.').count() <= 1
        && text
            .strip_prefix(['+', '-'])
            .unwrap_or(text)
            .chars()
            .any(|c| c.is_ascii_digit());
    let parsed = if valid {
        text.parse::<f64>().ok().filter(|v| v.is_finite())
    } else {
        None
    };
    parsed.ok_or_else(|| {
        diagnostic(
            rules::INVALID_NUMERIC,
            format!("{value:?} is not a valid xs:decimal"),
            None,
        )
    })
}
