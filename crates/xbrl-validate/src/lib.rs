//! XBRL validation: core rule engine plus profile rule sets.
//!
//! Status: **scaffold only** (PLAN.md milestone 5). No rule engine exists yet;
//! this crate exposes no items, and nothing here may claim a conformance result.
//!
//! Design constraints this crate must keep when implemented:
//!
//! * every finding is a [`xbrl_core::diagnostic::Diagnostic`] with a stable rule
//!   id, severity and source span — no bare booleans and no "valid" verdict
//!   without the diagnostics that justify it;
//! * unsupported constructs fail closed: report `unsupported` rather than
//!   treating an unhandled rule as passed;
//! * specification profiles (SEC EFM, ESMA ESEF, DQC) are separate rule sets over
//!   the same engine, versioned independently of the core spec.
//!
//! Dependency boundary: `xbrl-core`, `xbrl-xml` and `xbrl-taxonomy`. This crate
//! never depends on `xbrl-render` or `xbrl-cli`.
