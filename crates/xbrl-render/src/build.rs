//! Report model construction from the typed instance and presentation network.
//!
//! Presentation links: XBRL 2.1 §5.2.4. Only `Presentation` relationships are
//! used; each extended-link role (ELR) is a separate network, reported in
//! ELR-URI order. Children follow `order` (ties: input order). Roots are
//! concepts that are a `from` but never a `to` in the same ELR, in input order.
use std::collections::{BTreeMap, HashMap, HashSet};

use xbrl_core::instance::{Context, Fact, Instance};
use xbrl_core::qname::QName;

use crate::display::{format_accuracy, format_period, format_unit, qname_display};
use crate::label::{STANDARD_LABEL_ROLE, select_label};
use xbrl_taxonomy::model::{Concept, Label, LinkbaseKind, Relationship};

/// Everything the renderer reads.
pub struct RenderInput<'a> {
    /// Instance.
    pub instance: &'a Instance,
    /// Relationships.
    pub relationships: &'a [Relationship],
    /// Labels.
    pub labels: &'a [Label],
    /// Concepts.
    pub concepts: &'a [Concept],
}
/// Options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOptions {
    /// Label role.
    pub label_role: String,
    /// Languages.
    pub languages: Vec<String>,
    /// ELR filter.
    pub elr: Option<String>,
}
impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            label_role: STANDARD_LABEL_ROLE.to_owned(),
            languages: vec!["en".to_owned()],
            elr: None,
        }
    }
}
/// Report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// ELR.
    pub elr: String,
    /// Table.
    pub table: Table,
}
/// Table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    /// Headers.
    pub headers: Vec<String>,
    /// Rows.
    pub rows: Vec<Row>,
}
/// Row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Depth.
    pub depth: usize,
    /// Cells.
    pub cells: Vec<Cell>,
}
/// Cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// Text.
    pub text: String,
    /// Kind.
    pub kind: CellKind,
}
/// Kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellKind {
    /// Label/concept.
    Text,
    /// Empty.
    Empty,
    /// Value.
    Value,
    /// Nil.
    Nil,
    /// Missing.
    Missing,
    /// Duplicate.
    Duplicate,
    /// Unsupported.
    Unsupported,
}
const UNSUPPORTED: &str = "[unsupported: dimensional context]";

/// Builds one report per presentation ELR (or only `options.elr`).
///
/// Columns are the contexts (instance order) referenced by facts of concepts in
/// the network, then unresolved context references in sorted order. Contexts
/// with segment or scenario content are unsupported columns: their values are
/// withheld, never treated as valid dimensional data.
#[must_use]
pub fn build_reports(input: &RenderInput<'_>, options: &RenderOptions) -> Vec<Report> {
    let mut by_elr: BTreeMap<&str, Vec<&Relationship>> = BTreeMap::new();
    for r in input
        .relationships
        .iter()
        .filter(|r| r.kind == LinkbaseKind::Presentation)
    {
        if options.elr.as_deref().is_none_or(|e| e == r.elr) {
            by_elr.entry(r.elr.as_str()).or_default().push(r);
        }
    }
    by_elr
        .into_iter()
        .map(|(elr, rels)| Report {
            elr: elr.to_owned(),
            table: build_table(input, options, &rels),
        })
        .collect()
}

struct Walk<'a> {
    children: HashMap<&'a QName, Vec<&'a Relationship>>,
    /// `(depth, concept, preferred label role, cycle)` in display order.
    rows: Vec<(usize, &'a QName, Option<&'a str>, bool)>,
}

impl<'a> Walk<'a> {
    fn visit(
        &mut self,
        concept: &'a QName,
        depth: usize,
        pref: Option<&'a str>,
        path: &mut Vec<&'a QName>,
    ) {
        if path.contains(&concept) {
            self.rows.push((depth, concept, pref, true));
            return;
        }
        self.rows.push((depth, concept, pref, false));
        path.push(concept);
        let kids = self.children.get(concept).cloned().unwrap_or_default();
        for k in kids {
            self.visit(&k.to, depth + 1, k.preferred_label.as_deref(), path);
        }
        path.pop();
    }
}

fn build_table(input: &RenderInput<'_>, options: &RenderOptions, rels: &[&Relationship]) -> Table {
    let mut children: HashMap<&QName, Vec<&Relationship>> = HashMap::new();
    for r in rels {
        children.entry(&r.from).or_default().push(r);
    }
    for kids in children.values_mut() {
        // Stable sort: equal `order` keeps input order.
        kids.sort_by(|a, b| a.order.total_cmp(&b.order));
    }
    let targets: HashSet<&QName> = rels.iter().map(|r| &r.to).collect();
    let mut roots: Vec<&QName> = Vec::new();
    for r in rels {
        if !targets.contains(&r.from) && !roots.contains(&&r.from) {
            roots.push(&r.from);
        }
    }
    let mut walk = Walk {
        children,
        rows: Vec::new(),
    };
    for root in roots {
        walk.visit(root, 0, None, &mut Vec::new());
    }

    let in_tree: HashSet<&QName> = walk.rows.iter().map(|r| r.1).collect();
    let mut facts: HashMap<(&QName, &str), Vec<&Fact>> = HashMap::new();
    for f in &input.instance.facts {
        if in_tree.contains(&f.concept) {
            facts
                .entry((&f.concept, f.context_ref.as_str()))
                .or_default()
                .push(f);
        }
    }
    let used: HashSet<&str> = facts.keys().map(|k| k.1).collect();
    let known: HashSet<&str> = input
        .instance
        .contexts
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    let mut columns: Vec<(&str, Option<&Context>)> = Vec::new();
    for c in &input.instance.contexts {
        if used.contains(c.id.as_str()) && !columns.iter().any(|x| x.0 == c.id) {
            columns.push((c.id.as_str(), Some(c)));
        }
    }
    let mut unresolved: Vec<&str> = used
        .iter()
        .copied()
        .filter(|u| !known.contains(u))
        .collect();
    unresolved.sort_unstable();
    columns.extend(unresolved.into_iter().map(|u| (u, None)));

    let mut headers = vec!["Label".to_owned(), "Concept".to_owned()];
    for (id, c) in &columns {
        headers.push(match c {
            Some(c) if is_dimensional(c) => {
                format!("{id} {} {UNSUPPORTED}", format_period(&c.period))
            }
            Some(c) => format!("{id} {}", format_period(&c.period)),
            None => format!("{id} [unresolved context]"),
        });
    }
    let abstract_names: HashSet<&QName> = input
        .concepts
        .iter()
        .filter(|c| c.is_abstract)
        .map(|c| &c.name)
        .collect();

    let rows = walk
        .rows
        .iter()
        .map(|&(depth, concept, pref, cycle)| {
            let role = pref.unwrap_or(&options.label_role);
            let mut label = select_label(input.labels, concept, role, &options.languages)
                .map_or_else(|| qname_display(concept), |l| l.text.clone());
            if cycle {
                label.push_str(" [cycle]");
            }
            let mut cells = vec![
                Cell {
                    text: label,
                    kind: CellKind::Text,
                },
                Cell {
                    text: qname_display(concept),
                    kind: CellKind::Text,
                },
            ];
            for (id, c) in &columns {
                cells.push(cell_for(
                    input,
                    c.as_ref().copied(),
                    facts.get(&(concept, *id)),
                    abstract_names.contains(concept),
                    cycle,
                ));
            }
            Row { depth, cells }
        })
        .collect();
    Table { headers, rows }
}

fn is_dimensional(c: &Context) -> bool {
    c.entity.segment.as_ref().is_some_and(|s| !s.is_empty())
        || c.scenario.as_ref().is_some_and(|s| !s.is_empty())
}

fn cell_for(
    input: &RenderInput<'_>,
    context: Option<&Context>,
    facts: Option<&Vec<&Fact>>,
    is_abstract: bool,
    cycle: bool,
) -> Cell {
    let simple = |text: &str, kind| Cell {
        text: text.to_owned(),
        kind,
    };
    if context.is_some_and(is_dimensional) {
        return simple(UNSUPPORTED, CellKind::Unsupported);
    }
    if is_abstract || cycle {
        return simple("", CellKind::Empty);
    }
    match facts.map(Vec::as_slice) {
        None | Some([]) => simple("[missing]", CellKind::Missing),
        Some([one]) => match &one.value {
            None => simple("[nil]", CellKind::Nil),
            Some(v) => Cell {
                text: value_text(input, one, v),
                kind: CellKind::Value,
            },
        },
        Some(many) => Cell {
            text: format!("[duplicate: {} facts]", many.len()),
            kind: CellKind::Duplicate,
        },
    }
}

fn value_text(input: &RenderInput<'_>, fact: &Fact, value: &str) -> String {
    let mut out = value.to_owned();
    if let Some(unit_ref) = &fact.unit_ref {
        match input.instance.units.iter().find(|u| u.id == *unit_ref) {
            Some(u) => {
                out.push(' ');
                out.push_str(&format_unit(u));
            }
            None => out.push_str(&format!(" [unit {unit_ref} unresolved]")),
        }
    }
    if let Some(a) = fact.accuracy {
        out.push_str(&format!(" [{}]", format_accuracy(a)));
    }
    out
}
