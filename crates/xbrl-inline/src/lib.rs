//! Inline XBRL (iXBRL) support: extracting the instance facts embedded in an
//! XHTML document.
//!
//! Status: **scaffold only** (PLAN.md later-milestone list, after the MVP). No
//! iXBRL code exists yet; this crate exposes no items.
//!
//! When it lands it will:
//!
//! * extract facts, contexts, units and continuations from an iXBRL document
//!   under the same size/depth limits as the XML layer;
//! * keep the XHTML security posture explicit (no script execution, no remote
//!   resource loading) and report unsupported constructs as unsupported;
//! * report through [`xbrl_core::diagnostic::Diagnostic`] so the CLI and
//!   validation pipelines see one diagnostic shape.
//!
//! Dependency boundary: `xbrl-core` and `xbrl-xml`. The taxonomy crate joins only
//! once inline taxonomy references are actually resolved.
