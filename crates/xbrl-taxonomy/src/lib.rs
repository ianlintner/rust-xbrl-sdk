//! Taxonomy discovery and reading: DTS resolution, schemas, concepts, role types
//! and the presentation / calculation / label linkbases.
//!
//! Status: **scaffold only** (PLAN.md milestone 4). No discovery, schema or
//! linkbase code exists yet; this crate exposes no items.
//!
//! When it lands it will:
//!
//! * resolve a DTS from local files only, with no remote fetch by default and
//!   explicit limits on resolver IO and cycle depth;
//! * key concepts by [`xbrl_core::qname::QName`] and keep unknown extension
//!   content addressable instead of silently dropping it;
//! * surface every resolution failure as a [`xbrl_core::diagnostic::Diagnostic`]
//!   with rule id and source span, never as a silent empty result.
//!
//! Dependency boundary: `xbrl-core` and `xbrl-xml`. Validation and rendering
//! belong to their own crates; nothing here may depend on them.
