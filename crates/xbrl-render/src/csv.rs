//! CSV per RFC 4180 (CRLF records, `"` doubled, fields with `,` `"` CR LF
//! quoted), plus spreadsheet formula-injection protection: a field whose first
//! non-space character is `=`, `+`, `-`, `@`, TAB or CR gets a leading `'`.
//! This also prefixes legitimate negative numbers, an intentional trade-off.
//! XML-invalid control characters are replaced by U+FFFD.

use crate::Report;

fn field(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| {
            if c.is_control() && !matches!(c, '\n' | '\r' | '\t') {
                '\u{FFFD}'
            } else {
                c
            }
        })
        .collect();
    let risky = cleaned
        .trim_start_matches(' ')
        .starts_with(['=', '+', '-', '@', '\t', '\r']);
    let value = if risky {
        format!("'{cleaned}")
    } else {
        cleaned
    };
    if value.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value
    }
}

fn record(out: &mut String, fields: impl Iterator<Item = String>) {
    out.push_str(&fields.collect::<Vec<_>>().join(","));
    out.push_str("\r\n");
}

/// One report as CSV: a `Depth` column replaces visual indentation, then the
/// table columns. The ELR is not a column; render one report per file.
#[must_use]
pub fn render_csv(report: &Report) -> String {
    let mut out = String::new();
    record(
        &mut out,
        std::iter::once("Depth".to_owned()).chain(report.table.headers.iter().map(|h| field(h))),
    );
    for row in &report.table.rows {
        record(
            &mut out,
            std::iter::once(row.depth.to_string()).chain(row.cells.iter().map(|c| field(&c.text))),
        );
    }
    out
}
