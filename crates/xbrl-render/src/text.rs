//! Fixed-width text tables. Width is the `char` count; control characters are
//! shown as escapes so one record is always one line.

use std::fmt::Write as _;

use crate::{Report, Table};

/// Escapes control characters (`\n`, `\r`, `\t`, `\u{..}`) so a value stays on
/// one line. Backslash is left as is.
pub(crate) fn flatten(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{{{:x}}}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

fn lines(table: &Table) -> Vec<Vec<String>> {
    let mut all = vec![table.headers.iter().map(|h| flatten(h)).collect::<Vec<_>>()];
    for row in &table.rows {
        let mut cells: Vec<String> = row.cells.iter().map(|c| flatten(&c.text)).collect();
        if let Some(first) = cells.first_mut() {
            *first = format!("{}{first}", "  ".repeat(row.depth));
        }
        all.push(cells);
    }
    all
}

fn emit(out: &mut String, widths: &[usize], cells: &[String]) {
    let mut line = String::new();
    for (i, w) in widths.iter().enumerate() {
        if i > 0 {
            line.push_str(" | ");
        }
        let text = cells.get(i).map_or("", String::as_str);
        line.push_str(text);
        line.push_str(&" ".repeat(w - text.chars().count()));
    }
    out.push_str(line.trim_end());
    out.push('\n');
}

fn render_table(out: &mut String, table: &Table) {
    let grid = lines(table);
    let cols = table.headers.len();
    let widths: Vec<usize> = (0..cols)
        .map(|i| {
            grid.iter()
                .map(|r| r.get(i).map_or(0, |c| c.chars().count()))
                .max()
                .unwrap_or(0)
        })
        .collect();
    let mut it = grid.into_iter();
    if let Some(head) = it.next() {
        emit(out, &widths, &head);
    }
    let rule: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    let rule_line = rule.join("-+-");
    out.push_str(&rule_line);
    out.push('\n');
    for r in it {
        emit(out, &widths, &r);
    }
}

/// Renders each report as `ELR: <uri>`, an aligned table, and a blank line
/// between reports. Rows are indented two spaces per presentation depth.
#[must_use]
pub fn render_text(reports: &[Report]) -> String {
    let mut out = String::new();
    for (i, r) in reports.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str("ELR: ");
        out.push_str(&flatten(&r.elr));
        out.push('\n');
        render_table(&mut out, &r.table);
    }
    out
}
