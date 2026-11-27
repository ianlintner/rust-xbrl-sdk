//! Label selection (XBRL 2.1 §5.2.2: label role and `xml:lang`).
//!
//! Rules, in order, for requested role `R` and ordered languages `L`:
//!
//! 1. For role `R`, then (if `R` is not the standard role) the standard role:
//!    for each language in `L`: an exact case-insensitive `xml:lang` match, then
//!    a regional variant (`en` matches `en-US`).
//! 2. Still within that role: a label with no `xml:lang`.
//! 3. If `L` is empty, any label of that role in input order.
//!
//! Ties go to the first label in input order. A label in a language that was not
//! requested is never used and no text is invented; the caller falls back to the
//! concept name, which is a name and not a label.

use xbrl_core::qname::QName;
use xbrl_taxonomy::model::Label;

/// The standard label role (XBRL 2.1 §5.2.2.2).
pub const STANDARD_LABEL_ROLE: &str = "http://www.xbrl.org/2003/role/label";

fn lang_matches(lang: &str, wanted: &str) -> bool {
    let (lang, wanted) = (lang.to_ascii_lowercase(), wanted.to_ascii_lowercase());
    lang == wanted
        || lang
            .strip_prefix(&wanted)
            .is_some_and(|rest| rest.starts_with('-'))
}

fn in_role<'a>(
    labels: &'a [Label],
    concept: &QName,
    role: &str,
    languages: &[String],
) -> Option<&'a Label> {
    let mut candidates = labels
        .iter()
        .filter(|l| l.concept == *concept && l.role == role);
    if languages.is_empty() {
        return candidates.next();
    }
    let candidates: Vec<&Label> = candidates.collect();
    for wanted in languages {
        // Exact before regional variant, so `en` prefers a plain `en` label.
        let exact = candidates.iter().find(|l| {
            l.lang
                .as_deref()
                .is_some_and(|x| x.eq_ignore_ascii_case(wanted))
        });
        if let Some(l) = exact {
            return Some(l);
        }
        let variant = candidates
            .iter()
            .find(|l| l.lang.as_deref().is_some_and(|x| lang_matches(x, wanted)));
        if let Some(l) = variant {
            return Some(l);
        }
    }
    candidates.iter().find(|l| l.lang.is_none()).copied()
}

/// Selects the label for `concept` following the module rules, or `None`.
#[must_use]
pub fn select_label<'a>(
    labels: &'a [Label],
    concept: &QName,
    role: &str,
    languages: &[String],
) -> Option<&'a Label> {
    in_role(labels, concept, role, languages).or_else(|| {
        if role == STANDARD_LABEL_ROLE {
            None
        } else {
            in_role(labels, concept, STANDARD_LABEL_ROLE, languages)
        }
    })
}
