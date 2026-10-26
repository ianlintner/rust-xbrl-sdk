//! XBRL 2.1 XML syntax layer: bounded, namespace-aware reading of XBRL documents.
//!
//! Status: **scaffold only** (PLAN.md milestone 2). No parser is implemented, so
//! this crate deliberately exposes no items and cannot be mistaken for a working
//! reader or validator.
//!
//! When it lands it will:
//!
//! * stream from a reader with hard limits on document size, nesting depth and
//!   entity expansion, refusing remote fetches by default;
//! * resolve element and attribute names to [`xbrl_core::qname::QName`];
//! * report every problem as a [`xbrl_core::diagnostic::Diagnostic`] carrying a
//!   stable rule id, severity and source span, instead of aborting with a string.
//!
//! Dependency boundary: `xbrl-core` only. Taxonomy, validation and rendering
//! concerns belong in their own crates.
