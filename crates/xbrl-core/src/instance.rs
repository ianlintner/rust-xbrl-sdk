//! Syntax-level XBRL 2.1 instance model. Extraction is not taxonomy or DTS validation.
//!
//! XBRL 2.1 REC 2003-12-31 with errata to 2013-02-20, §§4.1–4.8.

use std::sync::Arc;

use crate::diagnostic::SourceSpan;
use crate::qname::QName;

/// Extracted instance. References are lexical; no taxonomy is loaded here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instance {
    /// Schema references in document order, not fetched or checked as schemas.
    pub schema_refs: Vec<SchemaRef>,
    /// Contexts in document order.
    pub contexts: Vec<Context>,
    /// Units in document order.
    pub units: Vec<Unit>,
    /// Item facts in document order; tuples require taxonomy discovery and are unsupported.
    pub facts: Vec<Fact>,
}

/// A link:schemaRef simple link, with raw href (no I/O or URI resolution).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaRef {
    /// Literal xlink:href; resolve with xml:base and an explicit resolver later.
    pub href: String,
    /// Optional xml:base from the root, then schemaRef, in ancestor order.
    pub base_chain: Vec<String>,
    /// Byte span of the start element.
    pub span: SourceSpan,
}

/// A context identifying the entity and period for an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    /// XML ID.
    pub id: String,
    /// The entity identifier and scheme.
    pub entity: Entity,
    /// Instant, bounded duration or forever.
    pub period: Period,
    /// Optional extension content (not interpreted as dimensions).
    pub scenario: Option<Vec<ExtensionNode>>,
    /// Byte span of the start element.
    pub span: SourceSpan,
}

/// An entity's scheme and lexical identifier (token whitespace collapsed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entity {
    /// Nonempty identification scheme URI text.
    pub scheme: String,
    /// Identifier token text.
    pub identifier: String,
    /// Optional extension content (not interpreted as dimensions).
    pub segment: Option<Vec<ExtensionNode>>,
}

/// Structurally shared in-scope namespace declarations.
///
/// A scope is a parent-linked chain: extending it allocates only the new
/// declarations, and an element that declares nothing reuses its parent's
/// `Arc`. Memory is therefore proportional to declarations in the document,
/// not to nodes × in-scope prefixes.
#[derive(Debug, Clone, Default)]
pub struct NamespaceScope(Option<Arc<ScopeLink>>);

#[derive(Debug)]
struct ScopeLink {
    parent: NamespaceScope,
    /// `(prefix, uri)`; `None` prefix is the default namespace.
    declarations: Vec<(Option<String>, Arc<str>)>,
}

impl NamespaceScope {
    /// An empty scope.
    #[must_use]
    pub fn new() -> Self {
        Self(None)
    }

    /// A child scope with `declarations` shadowing this one. Returns a cheap
    /// clone of `self` when there are none.
    #[must_use]
    pub fn extended(&self, declarations: Vec<(Option<String>, Arc<str>)>) -> Self {
        if declarations.is_empty() {
            return self.clone();
        }
        Self(Some(Arc::new(ScopeLink {
            parent: self.clone(),
            declarations,
        })))
    }

    /// Resolves `prefix` (`None` = default namespace); innermost wins. The
    /// implicit `xml` prefix is not recorded here.
    #[must_use]
    pub fn resolve(&self, prefix: Option<&str>) -> Option<&Arc<str>> {
        let mut link = self.0.as_ref();
        while let Some(l) = link {
            if let Some((_, uri)) = l
                .declarations
                .iter()
                .rev()
                .find(|(p, _)| p.as_deref() == prefix)
            {
                return Some(uri);
            }
            link = l.parent.0.as_ref();
        }
        None
    }

    /// Whether both scopes are the same shared allocation (or both empty).
    #[must_use]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }

    /// All declarations, outermost first (later entries shadow earlier ones).
    #[must_use]
    pub fn declarations(&self) -> Vec<(Option<String>, Arc<str>)> {
        let mut chain = Vec::new();
        let mut link = self.0.as_ref();
        while let Some(l) = link {
            chain.push(l);
            link = l.parent.0.as_ref();
        }
        chain
            .iter()
            .rev()
            .flat_map(|l| l.declarations.iter().cloned())
            .collect()
    }
}

impl PartialEq for NamespaceScope {
    fn eq(&self, other: &Self) -> bool {
        self.declarations() == other.declarations()
    }
}
impl Eq for NamespaceScope {}

/// Extension subtree in segment/scenario, preserved without taxonomy interpretation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionNode {
    /// Expanded element name.
    pub name: QName,
    /// In-scope namespace declarations at this element, outermost first, as
    /// `(prefix, uri)` with `None` for the default namespace. Later entries
    /// shadow earlier ones. The implicit `xml` prefix is not listed.
    ///
    /// This is what makes a QName-valued attribute such as `xsi:type="t:T"`
    /// interpretable: `t` resolves to the URI recorded here.
    pub namespaces: NamespaceScope,
    /// Expanded attributes where possible; no-namespace attributes retain their local names.
    pub attributes: Vec<(Option<QName>, String, String)>,
    /// Direct character data.
    pub text: String,
    /// Nested extension elements in source order.
    pub children: Vec<ExtensionNode>,
}

/// XBRL period; date/dateTime text retains timezone and lexical representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Period {
    /// Date or dateTime instant.
    Instant(String),
    /// Start and end dates/dateTimes.
    Duration {
        /// Start boundary.
        start: String,
        /// End boundary.
        end: String,
    },
    /// Indefinite duration.
    Forever,
}

/// A unit of measurement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    /// XML ID.
    pub id: String,
    /// Ordered measure products (multiplicity is significant).
    pub measures: UnitMeasures,
    /// Byte span of the start element.
    pub span: SourceSpan,
}

/// A simple product or a ratio of two products.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnitMeasures {
    /// One or more measures multiplied together.
    Simple(Vec<QName>),
    /// Numerator and denominator each contain at least one measure.
    Divide {
        /// Numerator product.
        numerator: Vec<QName>,
        /// Denominator product.
        denominator: Vec<QName>,
    },
}

/// Precision or decimal-place accuracy, not a rounding or value conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accuracy {
    /// xbrli:decimalsType integer (bounded to i64 in this extraction API).
    Decimals(i64),
    /// xbrli:precisionType non-negative integer (bounded to u64).
    Precision(u64),
    /// Exact lexical numeric value (`INF`).
    Infinite,
}

/// A syntax-level item fact. Numeric type is not inferred from a unitRef alone:
/// actual concept type and constraints are deferred to DTS/validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    /// Expanded concept name (taxonomy membership is not verified).
    pub concept: QName,
    /// Required context reference, not resolved by this extraction layer.
    pub context_ref: String,
    /// Optional unit reference, not resolved or type-checked here.
    pub unit_ref: Option<String>,
    /// Optional accuracy; absent on nonnumeric or nil facts.
    pub accuracy: Option<Accuracy>,
    /// Lexical content with XML entities decoded; `None` for xsi:nil="true".
    pub value: Option<String>,
    /// Optional fact ID.
    pub id: Option<String>,
    /// Byte span of the start element.
    pub span: SourceSpan,
}
