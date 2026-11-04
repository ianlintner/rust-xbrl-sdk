//! Core XBRL data model shared by every other crate in this workspace.
//!
//! Status: **milestone 3** (PLAN.md). This crate now provides a syntax-level
//! XBRL 2.1 instance model (facts, contexts, units, accuracy, schema refs) in
//! addition to qualified names and diagnostics. It does not resolve a DTS or
//! validate concept types, context/unit references, or conformance.
//!
//! Dependency boundary: this crate is the root of the workspace graph and
//! depends on no other crate in the workspace.
//!
//! # Specification provenance
//!
//! * XBRL 2.1 (2003-12-31) with errata — <https://specifications.xbrl.org/>
//! * XML Namespaces 1.0 (3rd ed.) and XML Schema Part 2 (NCName lexical space)

pub mod diagnostic;
pub mod instance;
pub mod namespaces;
pub mod qname;
