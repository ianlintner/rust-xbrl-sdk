//! Safe, local-only URI resolution with OASIS XML Catalogs.
//!
//! Specification versions this module implements (a deliberately small subset):
//!
//! * XML Catalogs, OASIS Standard V1.1 (2005-10-07): §6 catalog syntax —
//!   `uri`, `rewriteURI`, `nextCatalog`, `xml:base`; §7.2.2 URI resolution
//!   (`uri` entries before `rewriteURI`, longest `uriStartString` wins, the
//!   first matching catalog wins, and a mapping is applied **once**).
//!   `uriSuffix`, `delegateURI` and `group` change URI mapping but are not
//!   implemented, so a catalog containing them is rejected (fail closed)
//!   instead of being silently mis-resolved. System/public identifier entries
//!   do not affect URI mapping and are ignored.
//! * RFC 3986 (STD 66, 2005-01) reference resolution, see [`crate::uri`].
//!
//! # Security model
//!
//! * **No network I/O.** Only `file:` URIs (and relative references, which are
//!   resolved against a `file:` base) can be read. Every other scheme, and a
//!   `file:` URI naming a non-local host, fails with
//!   [`rules::RESOLVER_REMOTE_BLOCKED`]. A catalog is the only way to map a
//!   remote name (such as a taxonomy namespace URL) onto a local file.
//! * **Sandbox.** Every read must stay inside a [`DocumentRoot`]. The check is
//!   done twice: lexically on the percent-decoded, dot-segment-normalized path
//!   (so a missing file outside the root cannot be told apart from an existing
//!   one), then again on the canonical path after symlinks are followed.
//! * **Bounded.** File size, catalog entries and `nextCatalog` fan-out have
//!   hard limits, and `nextCatalog` cycles are detected. Catalogs are parsed
//!   with [`XmlReader`], which refuses DOCTYPE and therefore any entity.
//!
//! Known limit: a TOCTOU window exists between canonicalization and open; the
//! document root is assumed not to be concurrently modified by an attacker.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use xbrl_core::diagnostic::Diagnostic;

use crate::limits::Limits;
use crate::reader::{StartElement, XML_NAMESPACE, XmlEvent, XmlReader};
use crate::rules;
use crate::uri::{Uri, error};

/// Namespace of OASIS XML Catalogs elements (XML Catalogs 1.1 §6.1).
const CATALOG_NAMESPACE: &str = "urn:oasis:names:tc:entity:xmlns:xml:catalog";

/// Resource bounds for the resolver and catalog loader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolverLimits {
    /// Maximum size of one file read through the resolver, catalogs included.
    /// A file of exactly this size is accepted. Default: 64 MiB.
    pub max_file_bytes: u64,
    /// Maximum `uri`/`rewriteURI`/`nextCatalog` entries across all catalogs
    /// loaded by one resolver. Default: 4096.
    pub max_catalog_entries: usize,
    /// Maximum catalog files loaded by one resolver, counting a file once per
    /// time it is reached (so a `nextCatalog` diamond cannot fan out). Default: 32.
    pub max_catalog_files: usize,
}

impl Default for ResolverLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 64 * 1024 * 1024,
            max_catalog_entries: 4096,
            max_catalog_files: 32,
        }
    }
}

/// The directory tree a [`Resolver`] may read from.
#[derive(Debug, Clone)]
pub struct DocumentRoot {
    path: PathBuf,
    uri: Uri,
}

impl DocumentRoot {
    /// Opens `path` as a sandbox root. The path is canonicalized.
    ///
    /// # Errors
    ///
    /// [`rules::RESOLVER_ROOT_NOT_ACCESSIBLE`] if the path does not exist or is
    /// not a directory.
    pub fn new(path: impl AsRef<Path>) -> Result<Self, Diagnostic> {
        let path = path.as_ref();
        let canonical = fs::canonicalize(path).map_err(|e| {
            error(
                rules::RESOLVER_ROOT_NOT_ACCESSIBLE,
                format!("document root {path:?} is not accessible: {e}"),
            )
        })?;
        if !canonical.is_dir() {
            return Err(error(
                rules::RESOLVER_ROOT_NOT_ACCESSIBLE,
                format!("document root {path:?} is not a directory"),
            ));
        }
        let mut text = Uri::from_file_path(&canonical)?.to_string();
        if !text.ends_with('/') {
            text.push('/');
        }
        Ok(Self {
            uri: Uri::parse(&text)?,
            path: canonical,
        })
    }

    /// The canonical root directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// A successfully read document.
#[derive(Debug, Clone)]
pub struct ResolvedFile {
    /// The reference as requested, made absolute against the base (fragment kept).
    pub requested: Uri,
    /// The URI actually read, after catalog mapping, without fragment.
    pub mapped: Uri,
    /// The canonical path that was read.
    pub path: PathBuf,
    /// The file contents (at most [`ResolverLimits::max_file_bytes`]).
    pub bytes: Vec<u8>,
}

#[derive(Debug, Default)]
struct Catalog {
    /// `(name, absolute target)` in document order.
    exact: Vec<(String, String)>,
    /// `(uriStartString, absolute rewrite prefix)` in document order.
    rewrites: Vec<(String, String)>,
}

impl Catalog {
    /// XML Catalogs 1.1 §7.2.2: `uri` entries first, then the longest matching
    /// `rewriteURI`. Returns the (still textual) mapped URI.
    fn lookup(&self, name: &str) -> Option<String> {
        if let Some((_, target)) = self.exact.iter().find(|(n, _)| n == name) {
            return Some(target.clone());
        }
        let mut best: Option<&(String, String)> = None;
        for rule in &self.rewrites {
            if name.starts_with(&rule.0) && best.is_none_or(|b| rule.0.len() > b.0.len()) {
                best = Some(rule);
            }
        }
        best.map(|(start, prefix)| format!("{prefix}{}", &name[start.len()..]))
    }
}

#[derive(Default)]
struct Parsed {
    catalog: Catalog,
    next: Vec<Uri>,
    entries: usize,
}

/// Loader state, committed to the resolver only if the whole load succeeds.
struct Staged {
    catalogs: Vec<Catalog>,
    entries: usize,
    files: usize,
    stack: Vec<PathBuf>,
}

/// Local-only resolver. See the module documentation for the security model.
#[derive(Debug)]
pub struct Resolver {
    root: DocumentRoot,
    limits: ResolverLimits,
    catalogs: Vec<Catalog>,
    entries: usize,
    files: usize,
}

impl Resolver {
    /// A resolver with default limits and no catalogs.
    #[must_use]
    pub fn new(root: DocumentRoot) -> Self {
        Self {
            root,
            limits: ResolverLimits::default(),
            catalogs: Vec::new(),
            entries: 0,
            files: 0,
        }
    }

    /// Replaces the resource limits.
    #[must_use]
    pub fn with_limits(mut self, limits: ResolverLimits) -> Self {
        self.limits = limits;
        self
    }

    /// The sandbox root.
    #[must_use]
    pub fn root(&self) -> &DocumentRoot {
        &self.root
    }

    /// Loads a catalog from memory. `base` is the URI the catalog would have as
    /// a file; relative `uri`, `rewritePrefix` and `nextCatalog` values resolve
    /// against it. `nextCatalog` files are loaded after this catalog's entries.
    ///
    /// All-or-nothing: on error no entry of this load becomes visible.
    ///
    /// # Errors
    ///
    /// Reader diagnostics for malformed or hostile XML; [`rules::CATALOG_INVALID_ROOT`],
    /// [`rules::CATALOG_INVALID_ENTRY`], [`rules::CATALOG_UNSUPPORTED`],
    /// [`rules::CATALOG_CYCLE`], [`rules::RESOLVER_LIMIT_CATALOG`], and the
    /// resolver rules for any `nextCatalog` file that cannot be read.
    pub fn add_catalog_bytes(&mut self, bytes: &[u8], base: &Uri) -> Result<(), Diagnostic> {
        let mut staged = self.stage();
        self.load_catalog(bytes, base, &mut staged)?;
        self.commit(staged);
        Ok(())
    }

    /// Loads a catalog file from inside the document root. `reference` resolves
    /// against `base`, or against the root when `base` is `None`.
    ///
    /// # Errors
    ///
    /// As [`Resolver::add_catalog_bytes`], plus the errors of [`Resolver::resolve`]
    /// for reading the file itself. Catalog mapping is **not** applied to the
    /// catalog path.
    pub fn add_catalog_file(
        &mut self,
        reference: &str,
        base: Option<&Uri>,
    ) -> Result<(), Diagnostic> {
        let uri = base
            .unwrap_or(&self.root.uri)
            .resolve(&Uri::parse(reference)?)?
            .without_fragment();
        let (path, bytes) = self.read_local(&uri)?;
        let mut staged = self.stage();
        staged.stack.push(path);
        self.load_catalog(&bytes, &uri, &mut staged)?;
        self.commit(staged);
        Ok(())
    }

    /// Resolves `reference` (against `base`, or the root) and reads the file.
    ///
    /// # Errors
    ///
    /// [`rules::RESOLVER_INVALID_URI`], [`rules::RESOLVER_REMOTE_BLOCKED`],
    /// [`rules::RESOLVER_OUTSIDE_ROOT`], [`rules::RESOLVER_NOT_FOUND`],
    /// [`rules::RESOLVER_FILE_TOO_LARGE`] or [`rules::RESOLVER_IO_ERROR`].
    pub fn resolve(&self, reference: &str, base: Option<&Uri>) -> Result<ResolvedFile, Diagnostic> {
        let requested = base
            .unwrap_or(&self.root.uri)
            .resolve(&Uri::parse(reference)?)?;
        let lookup = requested.without_fragment();
        // Applied once: the result of a mapping is never mapped again.
        let mapped = match self.map_through_catalogs(&lookup.to_string()) {
            Some(text) => Uri::parse(&text)?,
            None => lookup,
        };
        let (path, bytes) = self.read_local(&mapped)?;
        Ok(ResolvedFile {
            requested,
            mapped,
            path,
            bytes,
        })
    }

    fn map_through_catalogs(&self, name: &str) -> Option<String> {
        self.catalogs.iter().find_map(|c| c.lookup(name))
    }

    fn stage(&self) -> Staged {
        Staged {
            catalogs: Vec::new(),
            entries: self.entries,
            files: self.files,
            stack: Vec::new(),
        }
    }

    fn commit(&mut self, staged: Staged) {
        self.entries = staged.entries;
        self.files = staged.files;
        self.catalogs.extend(staged.catalogs);
    }

    fn load_catalog(
        &self,
        bytes: &[u8],
        base: &Uri,
        staged: &mut Staged,
    ) -> Result<(), Diagnostic> {
        staged.files += 1;
        if staged.files > self.limits.max_catalog_files {
            return Err(error(
                rules::RESOLVER_LIMIT_CATALOG,
                format!(
                    "more than {} catalog files loaded",
                    self.limits.max_catalog_files
                ),
            ));
        }
        let reader_limits = Limits {
            max_document_bytes: self.limits.max_file_bytes,
            ..Limits::default()
        };
        let budget = self
            .limits
            .max_catalog_entries
            .saturating_sub(staged.entries);
        let parsed = parse_catalog(bytes, base, &reader_limits, budget)?;
        staged.entries += parsed.entries;
        staged.catalogs.push(parsed.catalog);
        for next in parsed.next {
            let (path, next_bytes) = self.read_local(&next)?;
            if staged.stack.contains(&path) {
                return Err(error(
                    rules::CATALOG_CYCLE,
                    format!("nextCatalog cycle through {path:?}"),
                ));
            }
            staged.stack.push(path);
            self.load_catalog(&next_bytes, &next, staged)?;
            staged.stack.pop();
        }
        Ok(())
    }

    /// Reads a `file:` URI inside the sandbox. Never touches the network.
    fn read_local(&self, uri: &Uri) -> Result<(PathBuf, Vec<u8>), Diagnostic> {
        let decoded = uri.to_file_path()?;
        let lexical = normalize(&decoded);
        if !lexical.starts_with(&self.root.path) {
            return Err(outside(&lexical));
        }
        let canonical = fs::canonicalize(&lexical).map_err(|e| io_error(&lexical, &e))?;
        if !canonical.starts_with(&self.root.path) {
            return Err(outside(&canonical));
        }
        let io = |e: &std::io::Error| io_error(&canonical, e);
        let file = File::open(&canonical).map_err(|e| io(&e))?;
        let meta = file.metadata().map_err(|e| io(&e))?;
        if !meta.is_file() {
            return Err(error(
                rules::RESOLVER_IO_ERROR,
                format!("{canonical:?} is not a regular file"),
            ));
        }
        let max = self.limits.max_file_bytes;
        let too_large = || {
            error(
                rules::RESOLVER_FILE_TOO_LARGE,
                format!("{canonical:?} exceeds the {max}-byte file limit"),
            )
        };
        if meta.len() > max {
            return Err(too_large());
        }
        let mut bytes = Vec::new();
        file.take(max.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|e| io(&e))?;
        if bytes.len() as u64 > max {
            return Err(too_large());
        }
        Ok((canonical, bytes))
    }
}

fn outside(path: &Path) -> Diagnostic {
    error(
        rules::RESOLVER_OUTSIDE_ROOT,
        format!("{path:?} is outside the document root"),
    )
}

fn io_error(path: &Path, e: &std::io::Error) -> Diagnostic {
    if e.kind() == std::io::ErrorKind::NotFound {
        error(
            rules::RESOLVER_NOT_FOUND,
            format!("{path:?} does not exist"),
        )
    } else {
        error(
            rules::RESOLVER_IO_ERROR,
            format!("cannot read {path:?}: {e}"),
        )
    }
}

/// Lexically removes `.` and `..` components (a `..` at the root is dropped,
/// as in RFC 3986 §5.2.4). Needed after percent-decoding, which can reveal
/// `..` segments that URI dot-segment removal could not see.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if out.parent().is_some() {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn attribute<'a>(element: &'a StartElement, local: &str) -> Option<&'a str> {
    element
        .attributes
        .iter()
        .find(|a| a.name.namespace().is_none() && a.name.local_name() == local)
        .map(|a| a.value.as_str())
}

fn required<'a>(element: &'a StartElement, local: &str) -> Result<&'a str, Diagnostic> {
    match attribute(element, local) {
        Some(value) if !value.is_empty() => Ok(value),
        _ => Err(error(
            rules::CATALOG_INVALID_ENTRY,
            format!(
                "<{}> requires a non-empty {local:?} attribute",
                element.name.local_name()
            ),
        )),
    }
}

/// `xml:base` handling (XML Catalogs 1.1 §6.2): the element's base is its
/// `xml:base` resolved against the inherited base.
fn effective_base(inherited: &Uri, element: &StartElement) -> Result<Uri, Diagnostic> {
    let declared = element
        .attributes
        .iter()
        .find(|a| a.name.namespace() == Some(XML_NAMESPACE) && a.name.local_name() == "base");
    match declared {
        Some(attr) => inherited.resolve(&Uri::parse(&attr.value)?),
        None => Ok(inherited.clone()),
    }
}

fn parse_catalog(
    source: &[u8],
    base: &Uri,
    limits: &Limits,
    budget: usize,
) -> Result<Parsed, Diagnostic> {
    let mut reader = XmlReader::new(source, limits.clone());
    let mut parsed = Parsed::default();
    let mut root_base: Option<Uri> = None;
    let mut depth = 0_usize;
    while let Some(event) = reader.next_event()? {
        match event {
            XmlEvent::Text(_) => {}
            XmlEvent::End(_) => depth = depth.saturating_sub(1),
            XmlEvent::Start(element) => {
                depth += 1;
                let in_catalog = element.name.namespace() == Some(CATALOG_NAMESPACE);
                if depth == 1 {
                    if !in_catalog || element.name.local_name() != "catalog" {
                        return Err(error(
                            rules::CATALOG_INVALID_ROOT,
                            "the root element must be {urn:oasis:names:tc:entity:xmlns:xml:catalog}catalog",
                        ));
                    }
                    root_base = Some(effective_base(base, &element)?);
                } else if depth == 2 && in_catalog {
                    let inherited = root_base.as_ref().expect("depth 2 follows a root");
                    parse_entry(&element, inherited, &mut parsed, budget)?;
                }
            }
        }
    }
    if root_base.is_none() {
        return Err(error(
            rules::CATALOG_INVALID_ROOT,
            "the catalog has no root element",
        ));
    }
    Ok(parsed)
}

fn parse_entry(
    element: &StartElement,
    inherited: &Uri,
    parsed: &mut Parsed,
    budget: usize,
) -> Result<(), Diagnostic> {
    let name = element.name.local_name();
    let counted = matches!(name, "uri" | "rewriteURI" | "nextCatalog");
    if counted {
        parsed.entries += 1;
        if parsed.entries > budget {
            return Err(error(
                rules::RESOLVER_LIMIT_CATALOG,
                "the catalog entry limit was exceeded",
            ));
        }
    }
    let base = effective_base(inherited, element)?;
    let absolute = |value: &str| -> Result<Uri, Diagnostic> { base.resolve(&Uri::parse(value)?) };
    match name {
        "uri" => {
            let key = required(element, "name")?;
            let target = absolute(required(element, "uri")?)?;
            parsed
                .catalog
                .exact
                .push((key.to_owned(), target.to_string()));
        }
        "rewriteURI" => {
            let start = required(element, "uriStartString")?;
            let prefix = absolute(required(element, "rewritePrefix")?)?;
            parsed
                .catalog
                .rewrites
                .push((start.to_owned(), prefix.to_string()));
        }
        "nextCatalog" => {
            parsed
                .next
                .push(absolute(required(element, "catalog")?)?.without_fragment());
        }
        // Change URI mapping but are not implemented: fail closed.
        "uriSuffix" | "delegateURI" | "group" => {
            return Err(error(
                rules::CATALOG_UNSUPPORTED,
                format!("<{name}> is not supported by this resolver"),
            ));
        }
        // System/public identifier entries (XML Catalogs 1.1 §6.5) and the
        // TR9401 extensions do not affect URI mapping.
        "system" | "public" | "rewriteSystem" | "systemSuffix" | "delegateSystem"
        | "delegatePublic" | "doctype" | "document" | "dtddecl" | "entity" | "linktype"
        | "notation" | "sgmldecl" => {}
        other => {
            return Err(error(
                rules::CATALOG_UNSUPPORTED,
                format!("unrecognized catalog element <{other}>"),
            ));
        }
    }
    Ok(())
}
