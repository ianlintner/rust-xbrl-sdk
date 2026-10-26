//! RFC 3986 URI reference parsing and resolution, plus strict `file:` mapping.
//!
//! This is deliberately a small, dependency-free subset of RFC 3986:
//! parsing into the five components (Appendix B), reference resolution
//! (§5.2, strict mode) with dot-segment removal (§5.2.4), and conversion
//! to/from local filesystem paths. It does **not** perform network I/O and
//! does not interpret any scheme other than `file`.
//!
//! Specification: IETF RFC 3986 (STD 66, January 2005) —
//! <https://www.rfc-editor.org/rfc/rfc3986>. The unit tests reproduce the
//! normal and abnormal examples of RFC 3986 §5.4 verbatim (base
//! `http://a/b/c/d;p?q`), which is the provenance of those expectations.
//!
//! Only POSIX-style absolute paths are supported for `file:` URIs; Windows
//! drive letters and UNC paths are rejected, not guessed at.

use std::fmt;
use std::path::{Path, PathBuf};

use xbrl_core::diagnostic::{Diagnostic, RuleId, Severity};

use crate::rules;

/// A parsed URI reference (absolute URI or relative reference).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Uri {
    scheme: Option<String>,
    authority: Option<String>,
    path: String,
    query: Option<String>,
    fragment: Option<String>,
}

pub(crate) fn error(rule: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(
        RuleId::new(rule).expect("rule constants are valid rule ids"),
        Severity::Error,
        message,
        None,
    )
}

fn invalid(message: impl Into<String>) -> Diagnostic {
    error(rules::RESOLVER_INVALID_URI, message)
}

impl Uri {
    /// Parses a URI or relative reference.
    ///
    /// Scheme names are lowercased (RFC 3986 §6.2.2.1); everything else is kept
    /// verbatim. Controls, spaces and the characters `<>"\^`{|}` are rejected
    /// rather than silently escaped.
    ///
    /// # Errors
    ///
    /// [`rules::RESOLVER_INVALID_URI`] when the text is not a valid reference.
    pub fn parse(input: &str) -> Result<Self, Diagnostic> {
        if let Some(bad) = input.chars().find(|c| {
            c.is_control()
                || matches!(
                    c,
                    ' ' | '<' | '>' | '"' | '\\' | '^' | '`' | '{' | '|' | '}'
                )
        }) {
            return Err(invalid(format!(
                "character {bad:?} is not allowed in a URI reference"
            )));
        }
        let (rest, fragment) = match input.split_once('#') {
            Some((rest, fragment)) => (rest, Some(fragment.to_owned())),
            None => (input, None),
        };
        let (rest, query) = match rest.split_once('?') {
            Some((rest, query)) => (rest, Some(query.to_owned())),
            None => (rest, None),
        };
        // After removing '?' and '#', a ':' before any '/' ends the scheme.
        let (scheme, rest) = match rest.find([':', '/']) {
            Some(i) if rest.as_bytes()[i] == b':' => {
                let scheme = &rest[..i];
                let mut chars = scheme.chars();
                let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
                    && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
                if !valid {
                    return Err(invalid(format!("{scheme:?} is not a valid URI scheme")));
                }
                (Some(scheme.to_ascii_lowercase()), &rest[i + 1..])
            }
            _ => (None, rest),
        };
        let (authority, path) = match rest.strip_prefix("//") {
            Some(after) => match after.find('/') {
                Some(i) => (Some(after[..i].to_owned()), after[i..].to_owned()),
                None => (Some(after.to_owned()), String::new()),
            },
            None => (None, rest.to_owned()),
        };
        Ok(Self {
            scheme,
            authority,
            path,
            query,
            fragment,
        })
    }

    /// The (lowercase) scheme, if present.
    #[must_use]
    pub fn scheme(&self) -> Option<&str> {
        self.scheme.as_deref()
    }

    /// The authority (host part), if present. `file:///x` has `Some("")`.
    #[must_use]
    pub fn authority(&self) -> Option<&str> {
        self.authority.as_deref()
    }

    /// The path component (possibly empty), still percent-encoded.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The query component without the leading `?`.
    #[must_use]
    pub fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }

    /// The fragment component without the leading `#`.
    #[must_use]
    pub fn fragment(&self) -> Option<&str> {
        self.fragment.as_deref()
    }

    /// A copy of this URI with the fragment removed.
    #[must_use]
    pub fn without_fragment(&self) -> Self {
        Self {
            fragment: None,
            ..self.clone()
        }
    }

    /// Resolves `reference` against `self` as the base (RFC 3986 §5.2.2, strict).
    ///
    /// # Errors
    ///
    /// [`rules::RESOLVER_INVALID_URI`] when `self` has no scheme (a base must be
    /// absolute).
    pub fn resolve(&self, reference: &Self) -> Result<Self, Diagnostic> {
        if self.scheme.is_none() {
            return Err(invalid("the base URI must be absolute (it has no scheme)"));
        }
        let target = if reference.scheme.is_some() {
            Self {
                path: remove_dot_segments(&reference.path),
                ..reference.clone()
            }
        } else if reference.authority.is_some() {
            Self {
                scheme: self.scheme.clone(),
                path: remove_dot_segments(&reference.path),
                ..reference.clone()
            }
        } else {
            let (path, query) = if reference.path.is_empty() {
                (
                    self.path.clone(),
                    reference.query.clone().or_else(|| self.query.clone()),
                )
            } else if reference.path.starts_with('/') {
                (
                    remove_dot_segments(&reference.path),
                    reference.query.clone(),
                )
            } else {
                (
                    remove_dot_segments(&self.merge_path(&reference.path)),
                    reference.query.clone(),
                )
            };
            Self {
                scheme: self.scheme.clone(),
                authority: self.authority.clone(),
                path,
                query,
                fragment: reference.fragment.clone(),
            }
        };
        Ok(target)
    }

    /// RFC 3986 §5.2.3.
    fn merge_path(&self, relative: &str) -> String {
        if self.authority.is_some() && self.path.is_empty() {
            return format!("/{relative}");
        }
        match self.path.rfind('/') {
            Some(i) => format!("{}{relative}", &self.path[..=i]),
            None => relative.to_owned(),
        }
    }

    /// Converts a `file:` URI to a local path.
    ///
    /// Accepted: scheme `file`, an empty authority or `localhost`, no query, an
    /// absolute path. The path is percent-decoded as UTF-8; a decoded NUL or a
    /// malformed escape is rejected. The fragment is ignored. The result is not
    /// yet normalized or sandboxed — callers must canonicalize and confine it.
    ///
    /// # Errors
    ///
    /// [`rules::RESOLVER_REMOTE_BLOCKED`] for any other scheme or a non-local
    /// host; [`rules::RESOLVER_INVALID_URI`] for a malformed `file:` URI.
    pub fn to_file_path(&self) -> Result<PathBuf, Diagnostic> {
        match self.scheme.as_deref() {
            Some("file") => {}
            Some(other) => {
                return Err(error(
                    rules::RESOLVER_REMOTE_BLOCKED,
                    format!("scheme {other:?} is not permitted: only local files are read"),
                ));
            }
            None => return Err(invalid("a relative reference cannot be mapped to a file")),
        }
        match self.authority.as_deref() {
            None | Some("") => {}
            Some(host) if host.eq_ignore_ascii_case("localhost") => {}
            Some(host) => {
                return Err(error(
                    rules::RESOLVER_REMOTE_BLOCKED,
                    format!("file URI host {host:?} is not local"),
                ));
            }
        }
        if self.query.is_some() {
            return Err(invalid("a file URI must not carry a query"));
        }
        if !self.path.starts_with('/') {
            return Err(invalid("a file URI must have an absolute path"));
        }
        let decoded = percent_decode(&self.path)?;
        Ok(PathBuf::from(decoded))
    }

    /// Builds a `file:` URI from an absolute path, percent-encoding anything
    /// outside the RFC 3986 unreserved set and `/`.
    ///
    /// # Errors
    ///
    /// [`rules::RESOLVER_INVALID_URI`] when the path is relative or not UTF-8.
    pub fn from_file_path(path: &Path) -> Result<Self, Diagnostic> {
        if !path.is_absolute() {
            return Err(invalid(format!("{path:?} is not an absolute path")));
        }
        let text = path
            .to_str()
            .ok_or_else(|| invalid(format!("{path:?} is not valid UTF-8")))?;
        let mut encoded = String::with_capacity(text.len());
        for byte in text.bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/') {
                encoded.push(char::from(byte));
            } else {
                encoded.push_str(&format!("%{byte:02X}"));
            }
        }
        Ok(Self {
            scheme: Some("file".to_owned()),
            authority: Some(String::new()),
            path: encoded,
            query: None,
            fragment: None,
        })
    }
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(scheme) = &self.scheme {
            write!(f, "{scheme}:")?;
        }
        if let Some(authority) = &self.authority {
            write!(f, "//{authority}")?;
        }
        f.write_str(&self.path)?;
        if let Some(query) = &self.query {
            write!(f, "?{query}")?;
        }
        if let Some(fragment) = &self.fragment {
            write!(f, "#{fragment}")?;
        }
        Ok(())
    }
}

/// RFC 3986 §5.2.4.
fn remove_dot_segments(path: &str) -> String {
    let mut input = path;
    let mut output = String::new();
    let pop = |out: &mut String| match out.rfind('/') {
        Some(i) => out.truncate(i),
        None => out.clear(),
    };
    while !input.is_empty() {
        if let Some(rest) = input.strip_prefix("../") {
            input = rest;
        } else if let Some(rest) = input.strip_prefix("./") {
            input = rest;
        } else if input.starts_with("/./") {
            input = &input[2..];
        } else if input == "/." {
            input = "/";
        } else if input.starts_with("/../") {
            input = &input[3..];
            pop(&mut output);
        } else if input == "/.." {
            input = "/";
            pop(&mut output);
        } else if input == "." || input == ".." {
            input = "";
        } else {
            let start = usize::from(input.starts_with('/'));
            let end = input[start..].find('/').map_or(input.len(), |i| i + start);
            output.push_str(&input[..end]);
            input = &input[end..];
        }
    }
    output
}

fn percent_decode(text: &str) -> Result<String, Diagnostic> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes
                .get(i + 1..i + 3)
                .and_then(|h| std::str::from_utf8(h).ok())
                .and_then(|h| u8::from_str_radix(h, 16).ok())
                .ok_or_else(|| invalid("malformed percent-escape in path"))?;
            if hex == 0 {
                return Err(invalid("an encoded NUL byte is not allowed in a path"));
            }
            out.push(hex);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| invalid("decoded path is not valid UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(base: &str, reference: &str) -> String {
        Uri::parse(base)
            .unwrap()
            .resolve(&Uri::parse(reference).unwrap())
            .unwrap()
            .to_string()
    }

    const BASE: &str = "http://a/b/c/d;p?q";

    #[test]
    fn rfc3986_section_5_4_1_normal_examples() {
        let cases = [
            ("g:h", "g:h"),
            ("g", "http://a/b/c/g"),
            ("./g", "http://a/b/c/g"),
            ("g/", "http://a/b/c/g/"),
            ("/g", "http://a/g"),
            ("//g", "http://g"),
            ("?y", "http://a/b/c/d;p?y"),
            ("g?y", "http://a/b/c/g?y"),
            ("#s", "http://a/b/c/d;p?q#s"),
            ("g#s", "http://a/b/c/g#s"),
            ("g?y#s", "http://a/b/c/g?y#s"),
            (";x", "http://a/b/c/;x"),
            ("g;x", "http://a/b/c/g;x"),
            ("g;x?y#s", "http://a/b/c/g;x?y#s"),
            ("", "http://a/b/c/d;p?q"),
            (".", "http://a/b/c/"),
            ("./", "http://a/b/c/"),
            ("..", "http://a/b/"),
            ("../", "http://a/b/"),
            ("../g", "http://a/b/g"),
            ("../..", "http://a/"),
            ("../../", "http://a/"),
            ("../../g", "http://a/g"),
        ];
        for (reference, expected) in cases {
            assert_eq!(
                resolve(BASE, reference),
                expected,
                "reference {reference:?}"
            );
        }
    }

    #[test]
    fn rfc3986_section_5_4_2_abnormal_examples() {
        let cases = [
            ("../../../g", "http://a/g"),
            ("../../../../g", "http://a/g"),
            ("/./g", "http://a/g"),
            ("/../g", "http://a/g"),
            ("g.", "http://a/b/c/g."),
            (".g", "http://a/b/c/.g"),
            ("g..", "http://a/b/c/g.."),
            ("..g", "http://a/b/c/..g"),
            ("./../g", "http://a/b/g"),
            ("./g/.", "http://a/b/c/g/"),
            ("g/./h", "http://a/b/c/g/h"),
            ("g/../h", "http://a/b/c/h"),
            ("g;x=1/./y", "http://a/b/c/g;x=1/y"),
            ("g;x=1/../y", "http://a/b/c/y"),
            ("g?y/./x", "http://a/b/c/g?y/./x"),
            ("g?y/../x", "http://a/b/c/g?y/../x"),
            ("g#s/./x", "http://a/b/c/g#s/./x"),
            ("g#s/../x", "http://a/b/c/g#s/../x"),
            ("http:g", "http:g"),
        ];
        for (reference, expected) in cases {
            assert_eq!(
                resolve(BASE, reference),
                expected,
                "reference {reference:?}"
            );
        }
    }

    #[test]
    fn components_are_split_per_appendix_b() {
        let uri = Uri::parse("HTTP://user@Example.com:8080/a/b?x=1#frag").unwrap();
        assert_eq!(uri.scheme(), Some("http"));
        assert_eq!(uri.authority(), Some("user@Example.com:8080"));
        assert_eq!(uri.path(), "/a/b");
        assert_eq!(uri.query(), Some("x=1"));
        assert_eq!(uri.fragment(), Some("frag"));
        assert_eq!(uri.to_string(), "http://user@Example.com:8080/a/b?x=1#frag");
    }

    #[test]
    fn invalid_references_are_rejected_with_the_invalid_uri_rule() {
        for bad in ["a b", "http://x/\n", "1http://x", "a<b", "x\\y", "a:b c"] {
            let err = Uri::parse(bad).unwrap_err();
            assert_eq!(err.rule().as_str(), rules::RESOLVER_INVALID_URI, "{bad:?}");
        }
    }

    #[test]
    fn a_relative_base_is_rejected() {
        let err = Uri::parse("a/b")
            .unwrap()
            .resolve(&Uri::parse("c").unwrap())
            .unwrap_err();
        assert_eq!(err.rule().as_str(), rules::RESOLVER_INVALID_URI);
    }

    #[test]
    fn file_uris_map_to_paths_and_other_schemes_are_blocked() {
        let path = |s: &str| Uri::parse(s).unwrap().to_file_path();
        assert_eq!(
            path("file:///tmp/a%20b.xsd#x").unwrap(),
            PathBuf::from("/tmp/a b.xsd")
        );
        assert_eq!(
            path("file://localhost/tmp/a").unwrap(),
            PathBuf::from("/tmp/a")
        );
        for blocked in [
            "http://x/a",
            "https://x/a",
            "ftp://x/a",
            "urn:x:y",
            "jar:file:///a",
        ] {
            assert_eq!(
                path(blocked).unwrap_err().rule().as_str(),
                rules::RESOLVER_REMOTE_BLOCKED,
                "{blocked}"
            );
        }
        // A non-local host would be a network share, not a local file.
        assert_eq!(
            path("file://evil.example/etc/passwd")
                .unwrap_err()
                .rule()
                .as_str(),
            rules::RESOLVER_REMOTE_BLOCKED
        );
        for malformed in [
            "file:relative",
            "file:///a%zz",
            "file:///a%00b",
            "file:///a?q=1",
            "file:///a%ff",
        ] {
            assert_eq!(
                path(malformed).unwrap_err().rule().as_str(),
                rules::RESOLVER_INVALID_URI,
                "{malformed}"
            );
        }
    }

    #[test]
    fn from_file_path_round_trips_awkward_names() {
        let original = Path::new("/tmp/with space/ünï#?%.xsd");
        let uri = Uri::from_file_path(original).unwrap();
        assert!(uri.to_string().starts_with("file:///tmp/with%20space/"));
        assert_eq!(uri.to_file_path().unwrap(), original);
        assert_eq!(
            Uri::from_file_path(Path::new("rel"))
                .unwrap_err()
                .rule()
                .as_str(),
            rules::RESOLVER_INVALID_URI
        );
    }
}
