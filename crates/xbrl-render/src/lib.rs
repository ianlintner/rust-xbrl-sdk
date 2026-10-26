//! Headless rendering of report content: text tables, HTML tables and CSV.
//!
//! Status: **scaffold only** (PLAN.md milestone 6). No renderer exists yet; this
//! crate exposes no items.
//!
//! When it lands it will:
//!
//! * render from the typed model, never from re-parsed source text;
//! * escape every interpolated value (no HTML injection through labels or facts)
//!   and produce byte-identical output for identical input;
//! * keep rendering separate from validation: a renderer never decides whether a
//!   report is valid, it only formats what the model already holds, and it prints
//!   diagnostics verbatim as [`xbrl_core::diagnostic::Diagnostic`] text.
//!
//! Dependency boundary: `xbrl-core` and `xbrl-taxonomy` (for presentation trees
//! and labels). Rendering must not depend on `xbrl-validate` or `xbrl-cli`.
