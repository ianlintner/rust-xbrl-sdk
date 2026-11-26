//! Pure-Rust, bounded discovery of a local DTS and its common linkbases.
//!
//! This crate reads XML Schema documents and XBRL linkbases from the local
//! filesystem only. It produces typed concepts, types, labels and acyclic
//! presentation/calculation graphs. All failures are
//! [`xbrl_core::diagnostic::Diagnostic`] values with stable rule ids from
//! [`rules`]. Remote references are blocked unless an OASIS catalog maps them
//! to local files; there is always a document limit, an item limit and cycle
//! detection. This crate does **not** validate instances or file profiles.
//!
//! Specification versions (no conformance claim is implied):
//!
//! * XBRL 2.1 Recommendation 2003-12-31 (errata 2013-02-20), §§3–7.
//! * XML Schema 1.0 Part 1, 2nd ed., 2004-10-28.
//! * XLink 1.0, 2001-06-27; XML Base, 2001-06-27; RFC 3986, 2005-01.
//! * XML Catalogs OASIS Standard 1.1, 2005-10-07.
//!
//! Fixture provenance and official conformance references are documented in
//! `fixtures/taxonomy/README.md`. All taxonomy fixtures are `authored`; no
//! third-party corpus is vendored or claimed as passed.

pub mod dts;
pub mod model;
pub mod rules;
