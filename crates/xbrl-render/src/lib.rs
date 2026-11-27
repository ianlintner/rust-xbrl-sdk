//! Headless rendering of presentation-tree reports as text, HTML and CSV.
//!
//! Specification basis (revisions pinned):
//!
//! * XBRL 2.1 Recommendation 2003-12-31 with errata to 2013-02-20: presentation
//!   links and `order`/`preferredLabel` (§5.2.4), label resources and roles
//!   (§5.2.2), contexts and periods (§4.7), units (§4.8), `decimals`/`precision`
//!   (§4.6.4–4.6.6).
//! * XML 1.0 (Fifth Edition) character data and the `Char` production, for HTML
//!   escaping of `&`, `<`, `>`, `"` and `'`.
//! * RFC 4180 for CSV quoting (CRLF records, doubled quotes).
//!
//! Boundaries:
//!
//! * Input is the typed model only ([`xbrl_core::instance::Instance`],
//!   [`xbrl_taxonomy::model`]); source text is never re-parsed.
//! * Rendering never decides validity. Missing, duplicate and nil facts are shown
//!   as explicit states; no value is manufactured or silently chosen.
//! * **Unsupported, not valid-by-inference:** XBRL Dimensions, hypercubes, typed
//!   dimensions, SEC/ESMA profiles and any certification. A context carrying
//!   segment or scenario content is rendered as an *unsupported* column whose
//!   values are withheld.
//! * Output is deterministic: it depends only on the input values and their
//!   order. Text width is measured in `char`s (no East-Asian width tables).

mod build;
mod csv;
mod display;
mod html;
mod label;
mod text;

pub use build::{Cell, CellKind, RenderInput, RenderOptions, Report, Row, Table, build_reports};
pub use csv::render_csv;
pub use display::{format_accuracy, format_period, format_unit, qname_display};
pub use html::render_html;
pub use label::{STANDARD_LABEL_ROLE, select_label};
pub use text::render_text;
