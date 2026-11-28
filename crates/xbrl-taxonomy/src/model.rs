//! Public typed model of a discovered DTS.
//!
//! Nothing here validates XBRL facts or filing profiles; this is the read-only
//! result of local schema and linkbase discovery. Spec versions and provenance
//! are documented in the crate root and `fixtures/taxonomy/README.md`.

use std::path::PathBuf;

use xbrl_core::diagnostic::SourceSpan;
use xbrl_core::qname::QName;

/// One document reached during DTS discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredDocument {
    /// Canonical local path, resolved through the bounded local resolver.
    pub path: PathBuf,
    /// Role of the document in the DTS.
    pub kind: DocumentKind,
    /// Absolute URI actually read (catalog mapping applied, fragment stripped).
    pub uri: String,
}

/// What a discovered document contributes to the DTS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DocumentKind {
    /// An XML Schema document reached from an entry schema or `xs:include`/`import`.
    Schema,
    /// A linkbase document reached from a `link:linkbaseRef`.
    Linkbase,
}

/// An XBRL global element declaration (`xs:element` in a schema's target namespace).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Concept {
    /// Expanded name; identity of the concept in the DTS.
    pub name: QName,
    /// XML ID declared on the element, when present.
    pub id: Option<String>,
    /// Type QName from the `type` attribute (lexically resolved).
    pub type_name: QName,
    /// `substitutionGroup` QName, when present.
    pub substitution_group: Option<QName>,
    /// Whether `abstract="true"` was declared.
    pub is_abstract: bool,
    /// Whether `nillable="true"` was declared.
    pub nillable: bool,
    /// XBRL `periodType`, when declared.
    pub period_type: Option<String>,
    /// Span of the element declaration in its source document.
    pub span: Option<SourceSpan>,
}

/// An XML Schema type definition discovered in the DTS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDefinition {
    /// Expanded name of the type.
    pub name: QName,
    /// XML ID, when present.
    pub id: Option<String>,
    /// `complexType` or `simpleType`.
    pub category: &'static str,
    /// Derivation method, e.g. `extension` or `restriction`.
    pub derivation: Option<String>,
    /// Lexical base type, retained for display and later validation.
    pub base: Option<String>,
    /// Span of the declaration in its source document.
    pub span: Option<SourceSpan>,
}

/// A `link:label` resource resolved to its concept and role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    /// Concept the label describes.
    pub concept: QName,
    /// Role URI from `xlink:role` (or the standard label role when absent).
    pub role: String,
    /// Language from `xml:lang`, when present.
    pub lang: Option<String>,
    /// Label text after XML character-data normalization.
    pub text: String,
}

/// Which standard linkbase graph an edge belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LinkbaseKind {
    /// `link:presentationLink` / `presentationArc`.
    Presentation,
    /// `link:calculationLink` / `calculationArc`.
    Calculation,
    /// `link:labelLink` / `labelArc` (labels are also exposed via [`Label`]).
    Label,
}

/// Extended-link role (XLink `xlink:role` on the extended link, XBRL 2.1
/// §3.5.3.9.7.3). Separate ELRs are separate networks: cycle detection and
/// graph traversal never cross an ELR boundary.
pub type Elr = String;

/// One directed relationship between resources in a linkbase graph.
#[derive(Debug, Clone, PartialEq)]
pub struct Relationship {
    /// Parent/summation concept (`xlink:from` resolved through a locator).
    pub from: QName,
    /// Child/summation-item concept (`xlink:to` resolved through a locator).
    pub to: QName,
    /// Which graph this edge belongs to.
    pub kind: LinkbaseKind,
    /// Extended-link role this arc belongs to.
    pub elr: Elr,
    /// Arcrole URI from the source, retained for provenance.
    pub arcrole: String,
    /// Numeric `order` (default 1.0).
    pub order: f64,
    /// Calculation `weight` (default 1.0). Only meaningful for calculations.
    pub weight: f64,
    /// `preferredLabel` role, when present (presentation only).
    pub preferred_label: Option<String>,
    /// Span of the arc in its source document.
    pub span: Option<SourceSpan>,
}

/// A child entry returned by graph traversal. The arc metadata lives only on
/// [`Relationship`]; this type adds the resolved child concept and nothing else,
/// so traversal cannot drift from the discovered edge.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphChild {
    /// Child concept.
    pub concept: QName,
    /// Edge that produced this child.
    pub edge: Relationship,
}

impl GraphChild {
    pub(crate) fn new(concept: QName, edge: Relationship) -> Self {
        Self { concept, edge }
    }

    /// The single source of truth for arc order, weight and preferred label.
    #[must_use]
    pub fn edge(&self) -> &Relationship {
        &self.edge
    }
}
