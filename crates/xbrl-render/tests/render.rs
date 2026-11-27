//! Behavior tests over the public API, built from hand-made typed models.

use xbrl_core::diagnostic::SourceSpan;
use xbrl_core::instance::{
    Accuracy, Context, Entity, ExtensionNode, Fact, Instance, NamespaceScope, Period, Unit,
    UnitMeasures,
};
use xbrl_core::qname::QName;
use xbrl_render::{
    CellKind, RenderInput, RenderOptions, STANDARD_LABEL_ROLE, build_reports, format_accuracy,
    format_period, format_unit, qname_display, render_csv, render_html, render_text, select_label,
};
use xbrl_taxonomy::model::{Concept, Label, LinkbaseKind, Relationship};

const NS: &str = "http://example.com/t";
const ISO: &str = "http://www.xbrl.org/2003/iso4217";
const PRES: &str = "http://www.xbrl.org/2003/role/presentationLink";
const TOTAL: &str = "http://www.xbrl.org/2003/role/totalLabel";

fn q(local: &str) -> QName {
    QName::new(Some("ex"), NS, local).unwrap()
}
fn span() -> SourceSpan {
    SourceSpan::new(0, 0).unwrap()
}
fn label(c: &str, role: &str, lang: Option<&str>, text: &str) -> Label {
    Label {
        concept: q(c),
        role: role.into(),
        lang: lang.map(Into::into),
        text: text.into(),
    }
}
fn rel(elr: &str, from: &str, to: &str, order: f64, pref: Option<&str>) -> Relationship {
    Relationship {
        from: q(from),
        to: q(to),
        kind: LinkbaseKind::Presentation,
        elr: elr.into(),
        arcrole: "http://www.xbrl.org/2003/arcrole/parent-child".into(),
        order,
        weight: 1.0,
        preferred_label: pref.map(Into::into),
        span: None,
    }
}
fn concept(name: &str, is_abstract: bool) -> Concept {
    Concept {
        name: q(name),
        id: None,
        type_name: q("T"),
        substitution_group: None,
        is_abstract,
        nillable: true,
        period_type: None,
        span: None,
    }
}
fn ctx(id: &str, period: Period, segment: bool) -> Context {
    Context {
        id: id.into(),
        entity: Entity {
            scheme: "http://example.com/e".into(),
            identifier: "E".into(),
            segment: segment.then(|| {
                vec![ExtensionNode {
                    name: q("explicitMember"),
                    namespaces: NamespaceScope::new(),
                    attributes: vec![],
                    text: String::new(),
                    children: vec![],
                }]
            }),
        },
        period,
        scenario: None,
        span: span(),
    }
}
fn usd() -> Unit {
    Unit {
        id: "usd".into(),
        measures: UnitMeasures::Simple(vec![QName::new(Some("iso"), ISO, "USD").unwrap()]),
        span: span(),
    }
}
fn fact(c: &str, cx: &str, unit: Option<&str>, value: Option<&str>, acc: Option<Accuracy>) -> Fact {
    Fact {
        concept: q(c),
        context_ref: cx.into(),
        unit_ref: unit.map(Into::into),
        accuracy: acc,
        value: value.map(Into::into),
        id: None,
        span: span(),
    }
}
fn instance(contexts: Vec<Context>, facts: Vec<Fact>) -> Instance {
    Instance {
        schema_refs: vec![],
        contexts,
        units: vec![usd()],
        facts,
    }
}
fn instant(d: &str) -> Period {
    Period::Instant(d.into())
}

fn render(
    inst: &Instance,
    rels: &[Relationship],
    labels: &[Label],
    concepts: &[Concept],
    opts: &RenderOptions,
) -> Vec<xbrl_render::Report> {
    build_reports(
        &RenderInput {
            instance: inst,
            relationships: rels,
            labels,
            concepts,
        },
        opts,
    )
}

// ---- formatting helpers ----

#[test]
fn period_instant_duration_forever() {
    assert_eq!(format_period(&instant("2025-12-31")), "2025-12-31");
    assert_eq!(
        format_period(&Period::Duration {
            start: "2025-01-01".into(),
            end: "2025-12-31".into()
        }),
        "2025-01-01 to 2025-12-31"
    );
    assert_eq!(format_period(&Period::Forever), "forever");
}

#[test]
fn unit_simple_product_and_divide() {
    assert_eq!(format_unit(&usd()), "iso:USD");
    let shares = QName::new(Some("x"), "http://example.com/s", "shares").unwrap();
    let iso = QName::new(Some("iso"), ISO, "USD").unwrap();
    let div = Unit {
        id: "u".into(),
        measures: UnitMeasures::Divide {
            numerator: vec![iso.clone(), iso.clone()],
            denominator: vec![shares],
        },
        span: span(),
    };
    assert_eq!(format_unit(&div), "iso:USD*iso:USD / x:shares");
}

#[test]
fn accuracy_display() {
    assert_eq!(format_accuracy(Accuracy::Decimals(-3)), "decimals=-3");
    assert_eq!(format_accuracy(Accuracy::Precision(4)), "precision=4");
    assert_eq!(format_accuracy(Accuracy::Infinite), "accuracy=INF");
}

#[test]
fn qname_display_uses_prefix_else_clark() {
    assert_eq!(qname_display(&q("A")), "ex:A");
    let bare = QName::new(None, NS, "A").unwrap();
    assert_eq!(qname_display(&bare), format!("{{{NS}}}A"));
}

// ---- labels ----

#[test]
fn label_role_language_and_fallback_rules() {
    let labels = vec![
        label("A", STANDARD_LABEL_ROLE, Some("de"), "Umsatz"),
        label("A", STANDARD_LABEL_ROLE, Some("en-US"), "Revenue US"),
        label("A", STANDARD_LABEL_ROLE, None, "Untagged"),
        label("A", TOTAL, Some("en"), "Total revenue"),
        label("A", STANDARD_LABEL_ROLE, Some("en-US"), "Second duplicate"),
    ];
    let en = vec!["en".to_string()];
    let pick = |role: &str, langs: &[String]| {
        select_label(&labels, &q("A"), role, langs).map(|l| l.text.as_str())
    };
    // "en" matches the en-US subtag; first in input order wins duplicates.
    assert_eq!(pick(STANDARD_LABEL_ROLE, &en), Some("Revenue US"));
    // Requested role is honoured.
    assert_eq!(pick(TOTAL, &en), Some("Total revenue"));
    // Language order is respected.
    let de_en = vec!["de".to_string(), "en".to_string()];
    assert_eq!(pick(STANDARD_LABEL_ROLE, &de_en), Some("Umsatz"));
    // Unavailable language falls to the language-less label, never another language.
    let fr = vec!["fr".to_string()];
    assert_eq!(pick(STANDARD_LABEL_ROLE, &fr), Some("Untagged"));
    // Unknown role falls back to the standard role.
    assert_eq!(pick("urn:other", &en), Some("Revenue US"));
    // en-US must not match plain "en" labels, and exact match is case-insensitive.
    let us = vec!["EN-us".to_string()];
    assert_eq!(pick(TOTAL, &us), Some("Revenue US"));
    // Empty language list accepts any language in input order.
    assert_eq!(pick(STANDARD_LABEL_ROLE, &[]), Some("Umsatz"));
    // No label at all: nothing is invented.
    assert!(select_label(&labels, &q("Z"), STANDARD_LABEL_ROLE, &en).is_none());
    // Only a foreign-language label exists and none is wanted: None.
    let only_de = vec![label("A", STANDARD_LABEL_ROLE, Some("de"), "Umsatz")];
    assert!(select_label(&only_de, &q("A"), STANDARD_LABEL_ROLE, &en).is_none());
}

// ---- tree ordering ----

#[test]
fn rows_follow_arc_order_ties_by_input_and_elrs_sorted() {
    let rels = vec![
        rel("urn:b", "Root", "X", 1.0, None),
        rel(PRES, "Root", "C", 3.0, None),
        rel(PRES, "Root", "A", 1.0, None),
        rel(PRES, "Root", "B", 1.0, None), // ties with A: input order keeps A first
        rel(PRES, "A", "A1", 2.0, None),
        rel(PRES, "A", "A0", 1.5, None),
    ];
    let inst = instance(vec![], vec![]);
    let reports = render(&inst, &rels, &[], &[], &RenderOptions::default());
    let elrs: Vec<_> = reports.iter().map(|r| r.elr.as_str()).collect();
    assert_eq!(elrs, vec![PRES, "urn:b"]); // sorted by ELR URI
    let rows: Vec<(usize, String)> = reports[0]
        .table
        .rows
        .iter()
        .map(|r| (r.depth, r.cells[1].text.clone()))
        .collect();
    let want: Vec<(usize, String)> = [
        (0, "Root"),
        (1, "A"),
        (2, "A0"),
        (2, "A1"),
        (1, "B"),
        (1, "C"),
    ]
    .iter()
    .map(|(d, n)| (*d, format!("ex:{n}")))
    .collect();
    assert_eq!(rows, want);
}

#[test]
fn elr_filter_selects_one_network_and_other_kinds_are_ignored() {
    let mut calc = rel(PRES, "Root", "Calc", 1.0, None);
    calc.kind = LinkbaseKind::Calculation;
    let rels = vec![
        rel("urn:a", "R1", "K", 1.0, None),
        rel("urn:b", "R2", "K", 1.0, None),
        calc,
    ];
    let inst = instance(vec![], vec![]);
    let opts = RenderOptions {
        elr: Some("urn:b".into()),
        ..RenderOptions::default()
    };
    let reports = render(&inst, &rels, &[], &[], &opts);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].elr, "urn:b");
    // Calculation arc produced no presentation network.
    let all = render(&inst, &rels, &[], &[], &RenderOptions::default());
    assert_eq!(all.len(), 2);
}

#[test]
fn cycle_terminates_and_is_marked() {
    let rels = vec![
        rel(PRES, "A", "B", 1.0, None),
        rel(PRES, "B", "A", 1.0, None),
    ];
    let inst = instance(vec![], vec![]);
    let r = render(&inst, &rels, &[], &[], &RenderOptions::default());
    // No root exists (both are targets): nothing is invented, but it terminates.
    assert!(r.is_empty() || r[0].table.rows.len() < 10);
    let rels = vec![
        rel(PRES, "R", "A", 1.0, None),
        rel(PRES, "A", "B", 1.0, None),
        rel(PRES, "B", "A", 1.0, None),
    ];
    let r = render(&inst, &rels, &[], &[], &RenderOptions::default());
    let rows = &r[0].table.rows;
    assert_eq!(rows.len(), 4); // R, A, B, A(cycle marker)
    assert!(rows[3].cells[0].text.contains("[cycle]"));
}

// ---- labels in rows ----

#[test]
fn row_label_uses_preferred_label_then_standard_then_qname() {
    let rels = vec![
        rel(PRES, "Root", "A", 1.0, Some(TOTAL)),
        rel(PRES, "Root", "B", 2.0, None),
        rel(PRES, "Root", "C", 3.0, Some("urn:nolabel")),
    ];
    let labels = vec![
        label("A", STANDARD_LABEL_ROLE, Some("en"), "Revenue"),
        label("A", TOTAL, Some("en"), "Total revenue"),
        label("B", STANDARD_LABEL_ROLE, Some("en"), "B label"),
        label("C", STANDARD_LABEL_ROLE, Some("en"), "C label"),
    ];
    let inst = instance(vec![], vec![]);
    let r = render(&inst, &rels, &labels, &[], &RenderOptions::default());
    let l: Vec<_> = r[0]
        .table
        .rows
        .iter()
        .map(|x| x.cells[0].text.as_str())
        .collect();
    assert_eq!(l, vec!["ex:Root", "Total revenue", "B label", "C label"]);
    assert_eq!(r[0].table.headers[..2], ["Label", "Concept"]);
}

// ---- cells ----

fn two_ctx_instance(facts: Vec<Fact>) -> Instance {
    instance(
        vec![
            ctx("c1", instant("2025-12-31"), false),
            ctx(
                "c2",
                Period::Duration {
                    start: "2025-01-01".into(),
                    end: "2025-12-31".into(),
                },
                false,
            ),
            ctx("cf", Period::Forever, false),
            ctx("cd", instant("2024-12-31"), true),
        ],
        facts,
    )
}

#[test]
fn cells_value_nil_missing_duplicate_unsupported_empty() {
    let rels = vec![
        rel(PRES, "Root", "Rev", 1.0, None),
        rel(PRES, "Root", "Nil", 2.0, None),
        rel(PRES, "Root", "Dup", 3.0, None),
        rel(PRES, "Root", "Txt", 4.0, None),
    ];
    let concepts = vec![concept("Root", true)];
    let facts = vec![
        fact(
            "Rev",
            "c2",
            Some("usd"),
            Some("123000"),
            Some(Accuracy::Decimals(-3)),
        ),
        fact("Nil", "c1", None, None, None),
        fact("Dup", "c1", Some("usd"), Some("1"), None),
        fact("Dup", "c1", Some("usd"), Some("2"), None),
        fact("Txt", "cf", None, Some("hi"), None),
        fact("Rev", "cd", Some("usd"), Some("999"), None),
        fact(
            "Rev",
            "gone",
            Some("nounit"),
            Some("5"),
            Some(Accuracy::Precision(2)),
        ),
        fact("Unrelated", "c1", None, Some("x"), None),
    ];
    let inst = two_ctx_instance(facts);
    let r = render(&inst, &rels, &[], &concepts, &RenderOptions::default());
    let t = &r[0].table;
    assert_eq!(
        t.headers,
        vec![
            "Label",
            "Concept",
            "c1 2025-12-31",
            "c2 2025-01-01 to 2025-12-31",
            "cf forever",
            "cd 2024-12-31 [unsupported: dimensional context]",
            "gone [unresolved context]",
        ]
    );
    let cell = |row: usize, col: usize| {
        (
            t.rows[row].cells[col].kind,
            t.rows[row].cells[col].text.as_str(),
        )
    };
    // Abstract root row is empty, not missing.
    assert_eq!(cell(0, 2), (CellKind::Empty, ""));
    assert_eq!(cell(1, 2), (CellKind::Missing, "[missing]"));
    assert_eq!(
        cell(1, 3),
        (CellKind::Value, "123000 iso:USD [decimals=-3]")
    );
    assert_eq!(
        cell(1, 5),
        (CellKind::Unsupported, "[unsupported: dimensional context]")
    );
    assert_eq!(
        cell(1, 6),
        (CellKind::Value, "5 [unit nounit unresolved] [precision=2]")
    );
    assert_eq!(cell(2, 2), (CellKind::Nil, "[nil]"));
    assert_eq!(cell(3, 2), (CellKind::Duplicate, "[duplicate: 2 facts]"));
    assert_eq!(cell(4, 4), (CellKind::Value, "hi"));
}

#[test]
fn contexts_without_tree_facts_get_no_column() {
    let rels = vec![rel(PRES, "Root", "A", 1.0, None)];
    let inst = two_ctx_instance(vec![fact("A", "c1", None, Some("v"), None)]);
    let r = render(&inst, &rels, &[], &[], &RenderOptions::default());
    assert_eq!(r[0].table.headers.len(), 3);
}

// ---- escaping and formats ----

fn hostile() -> (Instance, Vec<Relationship>, Vec<Label>) {
    let rels = vec![
        rel(PRES, "Root", "A", 1.0, None),
        rel(PRES, "Root", "B", 2.0, None),
    ];
    let labels = vec![
        label(
            "A",
            STANDARD_LABEL_ROLE,
            Some("en"),
            "R&D <b>\"x\"</b> 'y', z",
        ),
        label(
            "B",
            STANDARD_LABEL_ROLE,
            Some("en"),
            "=HYPERLINK(\"http://e\")",
        ),
    ];
    let inst = instance(
        vec![ctx("c1", instant("2025-12-31"), false)],
        vec![
            fact(
                "A",
                "c1",
                None,
                Some("<script>alert('1')</script> & \"q\"\nline2,\u{1}"),
                None,
            ),
            fact("B", "c1", Some("usd"), Some("-5"), None),
        ],
    );
    (inst, rels, labels)
}

#[test]
fn html_escapes_everything_and_injects_nothing() {
    let (inst, rels, labels) = hostile();
    let reports = render(&inst, &rels, &labels, &[], &RenderOptions::default());
    let html = render_html(&reports);
    assert!(!html.contains("<script"));
    assert!(!html.contains("<b>"));
    assert!(html.contains("R&amp;D &lt;b&gt;&quot;x&quot;&lt;/b&gt; &#39;y&#39;, z"));
    assert!(html.contains("&lt;script&gt;alert(&#39;1&#39;)&lt;/script&gt; &amp; &quot;q&quot;"));
    assert!(html.contains('\u{FFFD}')); // control char not valid in XML 1.0
    assert!(!html.contains('\u{1}'));
    // ELR heading is escaped too.
    let mut r2 = reports.clone();
    r2[0].elr = "urn:<x>&\"".into();
    assert!(render_html(&r2).contains("urn:&lt;x&gt;&amp;&quot;"));
}

#[test]
fn csv_quotes_and_defuses_formulas() {
    let (inst, rels, labels) = hostile();
    let reports = render(&inst, &rels, &labels, &[], &RenderOptions::default());
    let csv = render_csv(&reports[0]);
    let lines: Vec<&str> = csv.split("\r\n").collect();
    assert_eq!(lines[0], "Depth,Label,Concept,c1 2025-12-31");
    assert!(csv.ends_with("\r\n"));
    // Commas/quotes/newlines are quoted; quotes doubled.
    assert!(csv.contains("\"R&D <b>\"\"x\"\"</b> 'y', z\""));
    assert!(csv.contains("\"<script>alert('1')</script> & \"\"q\"\"\nline2,\u{FFFD}\""));
    // Formula injection: leading = and - are neutralised with a quote prefix.
    assert!(csv.contains(",\"'=HYPERLINK(\"\"http://e\"\")\","));
    assert!(csv.contains(",'-5 iso:USD\r\n"));
}

#[test]
fn text_table_aligns_and_flattens_control_characters() {
    let (inst, rels, labels) = hostile();
    let reports = render(&inst, &rels, &labels, &[], &RenderOptions::default());
    let text = render_text(&reports);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], format!("ELR: {PRES}"));
    // All table lines share the position of the second column separator.
    let table: Vec<&str> = lines[1..]
        .iter()
        .copied()
        .take_while(|l| !l.is_empty())
        .collect();
    let pos: Vec<_> = table
        .iter()
        .enumerate()
        .map(|(i, l)| l.find(if i == 1 { "-+-" } else { " | " }))
        .collect();
    assert!(
        pos.iter().all(|p| *p == pos[0] && p.is_some()),
        "{table:#?}"
    );
    assert!(text.contains("\\n") && !text.contains("line2,\u{1}"));
    assert!(text.contains("  R&D") || text.contains("  =HYPERLINK") || text.contains("ex:Root"));
    assert!(table[1].starts_with("---"));
}

#[test]
fn output_is_byte_identical_across_runs() {
    let (inst, rels, labels) = hostile();
    let a = render(&inst, &rels, &labels, &[], &RenderOptions::default());
    let b = render(&inst, &rels, &labels, &[], &RenderOptions::default());
    assert_eq!(render_text(&a), render_text(&b));
    assert_eq!(render_html(&a), render_html(&b));
    assert_eq!(render_csv(&a[0]), render_csv(&b[0]));
    assert_eq!(a, b);
}

// ---- golden: authored scenario covering every cell state and hostile text ----

#[test]
fn authored_scenario_matches_goldens() {
    let rels = vec![
        rel(PRES, "Root", "Rev", 1.0, Some(TOTAL)),
        rel(PRES, "Root", "Nil", 2.0, None),
        rel(PRES, "Root", "Dup", 3.0, None),
        rel(PRES, "Dup", "Txt", 1.0, None),
        rel(PRES, "Root", "Weird", 4.0, None),
    ];
    let labels = vec![
        label("Rev", STANDARD_LABEL_ROLE, Some("en"), "Revenue"),
        label("Rev", TOTAL, Some("en"), "Total revenue"),
        label("Rev", TOTAL, Some("de"), "Gesamtumsatz"),
        label("Dup", STANDARD_LABEL_ROLE, Some("en"), "Duplicated item"),
        label(
            "Weird",
            STANDARD_LABEL_ROLE,
            Some("en"),
            "R&D <b>\"x\"</b>, 'y'",
        ),
    ];
    let concepts = vec![concept("Root", true)];
    let inst = two_ctx_instance(vec![
        fact(
            "Rev",
            "c2",
            Some("usd"),
            Some("123000"),
            Some(Accuracy::Decimals(-3)),
        ),
        fact("Rev", "cd", Some("usd"), Some("999"), None),
        fact("Nil", "c1", None, None, None),
        fact("Dup", "c1", Some("usd"), Some("1"), None),
        fact("Dup", "c1", Some("usd"), Some("2"), None),
        fact("Txt", "cf", None, Some("line1\nline2, \"q\""), None),
        fact("Weird", "c1", None, Some("=1+1 <i>&</i>"), None),
    ]);
    let reports = render(&inst, &rels, &labels, &concepts, &RenderOptions::default());
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/render");
    let check = |name: &str, actual: String| {
        let path = dir.join(name);
        if std::env::var_os("XBRL_RENDER_BLESS").is_some() {
            std::fs::write(&path, &actual).unwrap();
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), actual, "{name}");
    };
    check("scenario.txt", render_text(&reports));
    check("scenario.html", render_html(&reports));
    check("scenario.csv", render_csv(&reports[0]));
}
