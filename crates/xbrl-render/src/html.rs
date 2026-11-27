//! HTML output. Every interpolated value passes through [`escape`]; attributes
//! only ever carry constants and a depth number. XML 1.0 `Char` production:
//! characters outside it are replaced by U+FFFD.

use std::fmt::Write as _;

use crate::{CellKind, Report};

fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
}

/// Escapes `&`, `<`, `>`, `"`, `'` and replaces non-XML characters.
pub(crate) fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c if !is_xml_char(c) => out.push('\u{FFFD}'),
            c => out.push(c),
        }
    }
    out
}

fn class(kind: CellKind) -> &'static str {
    match kind {
        CellKind::Text => "text",
        CellKind::Empty => "empty",
        CellKind::Value => "value",
        CellKind::Nil => "nil",
        CellKind::Missing => "missing",
        CellKind::Duplicate => "duplicate",
        CellKind::Unsupported => "unsupported",
    }
}

/// A complete HTML document with one `<section>` and `<table>` per report.
#[must_use]
pub fn render_html(reports: &[Report]) -> String {
    let mut out = String::from(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>XBRL presentation report</title>\n</head>\n<body>\n",
    );
    for r in reports {
        let _ = writeln!(out, "<section>\n<h2>{}</h2>", escape(&r.elr));
        out.push_str("<table>\n<thead>\n<tr>");
        for h in &r.table.headers {
            let _ = write!(out, "<th scope=\"col\">{}</th>", escape(h));
        }
        out.push_str("</tr>\n</thead>\n<tbody>\n");
        for row in &r.table.rows {
            let _ = write!(out, "<tr class=\"depth-{}\">", row.depth);
            for (i, c) in row.cells.iter().enumerate() {
                let tag = if i == 0 { "th scope=\"row\"" } else { "td" };
                let close = if i == 0 { "th" } else { "td" };
                let _ = write!(
                    out,
                    "<{tag} class=\"{}\">{}</{close}>",
                    class(c.kind),
                    escape(&c.text)
                );
            }
            out.push_str("</tr>\n");
        }
        out.push_str("</tbody>\n</table>\n</section>\n");
    }
    out.push_str("</body>\n</html>\n");
    out
}
