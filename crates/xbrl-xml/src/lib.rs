//! Bounded, namespace-aware streaming XML reader for XBRL 2.1 documents.
//!
//! Status: **milestone 3** (PLAN.md). This crate wraps `quick_xml` with
//! namespace resolution, hard limits on resource use, typed diagnostics and
//! syntax-level instance extraction ([`instance::parse_instance`]). It does not
//! validate a DTS or claim XBRL conformance.
//! It deliberately refuses DOCTYPE declarations, external entity references,
//! and remote fetches. Every finding is reported as a [`xbrl_core::diagnostic::Diagnostic`]
//! with a stable rule id, severity and source span.
//!
//! # Specification provenance (XML and Namespaces bases)
//!
//! * XML 1.0 (5th ed., 2008-11-26) — <https://www.w3.org/TR/xml/>
//! * Namespaces in XML 1.0 (3rd ed., 2009-12-08) — <https://www.w3.org/TR/REC-xml-names/>
//!
//! # Safety
//!
//! This crate requires no `unsafe` and the workspace forbids it. All limits
//! are explicit fields on [`limits::Limits`] with non-zero defaults checked before
//! every buffer allocation and every recursion depth increment.
//!
//! Dependency boundary: `xbrl-core` only (see `docs/architecture.md`).

pub mod instance;
pub mod limits;
pub mod reader;
pub mod resolver;
pub mod rules;
pub mod uri;
