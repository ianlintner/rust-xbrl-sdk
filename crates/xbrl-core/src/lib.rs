//! Core XBRL data model shared by every other crate in this workspace.
//!
//! Status: **scaffold baseline** (PLAN.md milestone 1). This crate currently
//! provides only the small, spec-derived primitives that later milestones build
//! on: XML qualified names, XBRL namespace constants and the typed diagnostic
//! envelope. It does not parse XBRL instances, resolve DTSs or validate
//! anything, and it makes no conformance claim.
//!
//! Dependency boundary: this crate is the root of the workspace graph and
//! depends on no other crate in the workspace.
//!
//! # Specification provenance
//!
//! * XBRL 2.1 (2003-12-31) with errata — <https://specifications.xbrl.org/>
//! * XML Namespaces 1.0 (3rd ed.) and XML Schema Part 2 (NCName lexical space)

pub mod diagnostic;
pub mod namespaces;
pub mod qname;
