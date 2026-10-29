//! Behavior tests for the local-only URI/catalog resolver.
//!
//! Specifications these expectations are derived from:
//!
//! * XML Catalogs, OASIS Standard V1.1 (2005-10-07): §6 (catalog syntax),
//!   §6.5.5 `nextCatalog`, §7.2.2 URI resolution (`uri` before `rewriteURI`,
//!   longest `uriStartString` wins, a mapping is applied once).
//! * RFC 3986 (STD 66, 2005-01) §5.2 reference resolution — covered in
//!   `src/uri.rs` unit tests with the §5.4 examples.
//! * XBRL Taxonomy Packages 1.0 (Final Recommendation, 2016-04-19) §3.2: a
//!   package's `META-INF/catalog.xml` is an OASIS catalog that uses `rewriteURI`.
//!
//! Every fixture is `authored` (see `fixtures/README.md`): the directory trees
//! are built in a temporary directory, so each test states its own premise.
//! Failure tests assert the stable rule id, not merely "an error occurred".

use std::fs;
use std::path::Path;

use tempfile::TempDir;
use xbrl_xml::resolver::{DocumentRoot, Resolver, ResolverLimits};
use xbrl_xml::rules;
use xbrl_xml::uri::Uri;

const CATALOG_NS: &str = "urn:oasis:names:tc:entity:xmlns:xml:catalog";

fn catalog(entries: &str) -> String {
    format!("<catalog xmlns=\"{CATALOG_NS}\">{entries}</catalog>")
}

fn put(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// A temp dir whose `pkg/` subdirectory is the document root; `outside.xsd`
/// sits next to it and must never be readable through the resolver.
fn sandbox() -> (TempDir, Resolver) {
    let dir = TempDir::new().unwrap();
    put(dir.path(), "outside.xsd", "<secret/>");
    put(dir.path(), "pkg/a.xsd", "<a/>");
    put(dir.path(), "pkg/sub/b.xsd", "<b/>");
    let root = DocumentRoot::new(dir.path().join("pkg")).unwrap();
    (dir, Resolver::new(root))
}

fn rule_of(result: Result<impl std::fmt::Debug, xbrl_core::diagnostic::Diagnostic>) -> String {
    result.unwrap_err().rule().as_str().to_owned()
}

/// The `file:` URI of `relative` under the temp dir; the file itself need not
/// exist (a catalog's base is only a name), but its directory must.
fn base_of(dir: &TempDir, relative: &str) -> Uri {
    let path = dir.path().join(relative);
    let parent = path.parent().unwrap().canonicalize().unwrap();
    Uri::from_file_path(&parent.join(path.file_name().unwrap())).unwrap()
}

// --- DocumentRoot -----------------------------------------------------------

#[test]
fn a_missing_or_non_directory_root_is_rejected() {
    let dir = TempDir::new().unwrap();
    put(dir.path(), "file.txt", "x");
    assert_eq!(
        rule_of(DocumentRoot::new(dir.path().join("absent"))),
        rules::RESOLVER_ROOT_NOT_ACCESSIBLE
    );
    assert_eq!(
        rule_of(DocumentRoot::new(dir.path().join("file.txt"))),
        rules::RESOLVER_ROOT_NOT_ACCESSIBLE
    );
}

// --- plain local resolution -------------------------------------------------

#[test]
fn relative_references_resolve_against_the_root_when_no_base_is_given() {
    let (_dir, resolver) = sandbox();
    let resolved = resolver.resolve("sub/b.xsd", None).unwrap();
    assert_eq!(resolved.bytes, b"<b/>");
    assert!(resolved.path.ends_with("pkg/sub/b.xsd"));
}

#[test]
fn relative_references_resolve_against_an_explicit_base_document() {
    let (dir, resolver) = sandbox();
    let base = base_of(&dir, "pkg/sub/b.xsd");
    let resolved = resolver.resolve("../a.xsd", Some(&base)).unwrap();
    assert_eq!(resolved.bytes, b"<a/>");
}

#[test]
fn the_fragment_is_dropped_before_reading() {
    let (_dir, resolver) = sandbox();
    let resolved = resolver.resolve("a.xsd#element(/1/2)", None).unwrap();
    assert_eq!(resolved.bytes, b"<a/>");
    assert_eq!(resolved.requested.fragment(), Some("element(/1/2)"));
    assert_eq!(resolved.mapped.fragment(), None);
}

#[test]
fn absolute_file_uris_inside_the_root_resolve() {
    let (dir, resolver) = sandbox();
    let uri = base_of(&dir, "pkg/a.xsd").to_string();
    assert_eq!(resolver.resolve(&uri, None).unwrap().bytes, b"<a/>");
}

#[test]
fn a_missing_file_inside_the_root_is_not_found() {
    let (_dir, resolver) = sandbox();
    assert_eq!(
        rule_of(resolver.resolve("nope.xsd", None)),
        rules::RESOLVER_NOT_FOUND
    );
}

#[test]
fn a_directory_is_not_a_document() {
    let (_dir, resolver) = sandbox();
    assert_eq!(
        rule_of(resolver.resolve("sub", None)),
        rules::RESOLVER_IO_ERROR
    );
}

#[test]
fn malformed_references_report_the_invalid_uri_rule() {
    let (_dir, resolver) = sandbox();
    assert_eq!(
        rule_of(resolver.resolve("a b.xsd", None)),
        rules::RESOLVER_INVALID_URI
    );
}

// --- no remote fetch --------------------------------------------------------

#[test]
fn remote_and_non_file_schemes_are_blocked_by_default() {
    let (_dir, resolver) = sandbox();
    for remote in [
        "http://www.xbrl.org/2003/xbrl-instance-2003-12-31.xsd",
        "https://www.sec.gov/a.xsd",
        "ftp://example.com/a.xsd",
        "urn:x:y",
        "jar:file:///a.zip!/a.xsd",
        "file://evil.example/share/a.xsd",
    ] {
        assert_eq!(
            rule_of(resolver.resolve(remote, None)),
            rules::RESOLVER_REMOTE_BLOCKED,
            "{remote}"
        );
    }
}

#[test]
fn a_remote_base_is_blocked_too() {
    let (_dir, resolver) = sandbox();
    let base = Uri::parse("http://example.com/dir/x.xsd").unwrap();
    assert_eq!(
        rule_of(resolver.resolve("a.xsd", Some(&base))),
        rules::RESOLVER_REMOTE_BLOCKED
    );
}

// --- traversal --------------------------------------------------------------

#[test]
fn dot_dot_traversal_out_of_the_root_is_refused() {
    let (_dir, resolver) = sandbox();
    // RFC 3986 §5.2.4 clamps "../" at the URI root, so this lands on /outside.xsd.
    for escape in [
        "../outside.xsd",
        "../../../../../../etc/passwd",
        "sub/../../outside.xsd",
    ] {
        assert_eq!(
            rule_of(resolver.resolve(escape, None)),
            rules::RESOLVER_OUTSIDE_ROOT,
            "{escape}"
        );
    }
}

#[test]
fn percent_encoded_traversal_is_refused_without_touching_the_filesystem() {
    let (dir, resolver) = sandbox();
    let root = dir.path().join("pkg").canonicalize().unwrap();
    for encoded in [
        "%2e%2e/outside.xsd",
        "sub/%2E%2E/%2E%2E/outside.xsd",
        "%2e%2e%2foutside.xsd",
    ] {
        let uri = format!("file://{}/{encoded}", root.display());
        assert_eq!(
            rule_of(resolver.resolve(&uri, None)),
            rules::RESOLVER_OUTSIDE_ROOT,
            "{encoded}"
        );
    }
}

#[test]
fn an_absolute_path_outside_the_root_is_refused_even_when_it_does_not_exist() {
    // No existence oracle: a nonexistent outside path must fail the same way as
    // an existing one.
    let (_dir, resolver) = sandbox();
    for outside in [
        "file:///etc/passwd",
        "file:///definitely/not/there.xsd",
        "/etc/passwd",
    ] {
        assert_eq!(
            rule_of(resolver.resolve(outside, None)),
            rules::RESOLVER_OUTSIDE_ROOT,
            "{outside}"
        );
    }
}

#[test]
fn a_sibling_directory_sharing_the_root_name_prefix_is_outside() {
    let dir = TempDir::new().unwrap();
    put(dir.path(), "pkg/a.xsd", "<a/>");
    put(dir.path(), "pkg-evil/x.xsd", "<x/>");
    let resolver = Resolver::new(DocumentRoot::new(dir.path().join("pkg")).unwrap());
    let evil = base_of(&dir, "pkg-evil/x.xsd").to_string();
    assert_eq!(
        rule_of(resolver.resolve(&evil, None)),
        rules::RESOLVER_OUTSIDE_ROOT
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_that_escapes_the_root_is_refused() {
    use std::os::unix::fs::symlink;
    let (dir, resolver) = sandbox();
    symlink(
        dir.path().join("outside.xsd"),
        dir.path().join("pkg/link.xsd"),
    )
    .unwrap();
    symlink(dir.path(), dir.path().join("pkg/dirlink")).unwrap();
    assert_eq!(
        rule_of(resolver.resolve("link.xsd", None)),
        rules::RESOLVER_OUTSIDE_ROOT
    );
    assert_eq!(
        rule_of(resolver.resolve("dirlink/outside.xsd", None)),
        rules::RESOLVER_OUTSIDE_ROOT
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_that_stays_inside_the_root_is_allowed() {
    use std::os::unix::fs::symlink;
    let (dir, resolver) = sandbox();
    symlink(
        dir.path().join("pkg/a.xsd"),
        dir.path().join("pkg/alias.xsd"),
    )
    .unwrap();
    assert_eq!(resolver.resolve("alias.xsd", None).unwrap().bytes, b"<a/>");
}

#[cfg(unix)]
#[test]
fn a_symlink_cycle_is_an_io_error_not_a_hang() {
    use std::os::unix::fs::symlink;
    let (dir, resolver) = sandbox();
    symlink(dir.path().join("pkg/y"), dir.path().join("pkg/x")).unwrap();
    symlink(dir.path().join("pkg/x"), dir.path().join("pkg/y")).unwrap();
    assert_eq!(
        rule_of(resolver.resolve("x", None)),
        rules::RESOLVER_IO_ERROR
    );
}

// --- size limits ------------------------------------------------------------

#[test]
fn a_file_over_the_byte_limit_is_refused_and_exactly_at_the_limit_is_accepted() {
    let (dir, _) = sandbox();
    put(dir.path(), "pkg/ten.xsd", "0123456789");
    let root = DocumentRoot::new(dir.path().join("pkg")).unwrap();
    let limits = |max_file_bytes| ResolverLimits {
        max_file_bytes,
        ..ResolverLimits::default()
    };
    let at_limit = Resolver::new(root.clone()).with_limits(limits(10));
    assert_eq!(at_limit.resolve("ten.xsd", None).unwrap().bytes.len(), 10);
    let over = Resolver::new(root).with_limits(limits(9));
    assert_eq!(
        rule_of(over.resolve("ten.xsd", None)),
        rules::RESOLVER_FILE_TOO_LARGE
    );
}

// --- catalogs: URI mapping --------------------------------------------------

#[test]
fn a_uri_entry_maps_an_exact_remote_name_to_a_local_file() {
    let (dir, mut resolver) = sandbox();
    let cat = catalog(r#"<uri name="http://example.com/a.xsd" uri="a.xsd"/>"#);
    resolver
        .add_catalog_bytes(cat.as_bytes(), &base_of(&dir, "pkg/catalog.xml"))
        .unwrap();
    let resolved = resolver.resolve("http://example.com/a.xsd", None).unwrap();
    assert_eq!(resolved.bytes, b"<a/>");
    assert_eq!(resolved.requested.to_string(), "http://example.com/a.xsd");
    assert!(resolved.mapped.to_string().starts_with("file:///"));
}

#[test]
fn rewrite_uri_replaces_the_prefix_and_the_longest_start_string_wins() {
    let (dir, mut resolver) = sandbox();
    put(dir.path(), "pkg/other/b.xsd", "<other/>");
    let cat = catalog(
        r#"<rewriteURI uriStartString="http://example.com/" rewritePrefix="./"/>
           <rewriteURI uriStartString="http://example.com/sub/" rewritePrefix="other/"/>"#,
    );
    resolver
        .add_catalog_bytes(cat.as_bytes(), &base_of(&dir, "pkg/catalog.xml"))
        .unwrap();
    assert_eq!(
        resolver
            .resolve("http://example.com/a.xsd", None)
            .unwrap()
            .bytes,
        b"<a/>"
    );
    assert_eq!(
        resolver
            .resolve("http://example.com/sub/b.xsd", None)
            .unwrap()
            .bytes,
        b"<other/>"
    );
}

#[test]
fn an_exact_uri_entry_beats_rewrite_uri_regardless_of_document_order() {
    let (dir, mut resolver) = sandbox();
    put(dir.path(), "pkg/special.xsd", "<special/>");
    let cat = catalog(
        r#"<rewriteURI uriStartString="http://example.com/" rewritePrefix="./"/>
           <uri name="http://example.com/a.xsd" uri="special.xsd"/>"#,
    );
    resolver
        .add_catalog_bytes(cat.as_bytes(), &base_of(&dir, "pkg/catalog.xml"))
        .unwrap();
    assert_eq!(
        resolver
            .resolve("http://example.com/a.xsd", None)
            .unwrap()
            .bytes,
        b"<special/>"
    );
}

#[test]
fn the_first_matching_entry_in_document_order_wins() {
    let (dir, mut resolver) = sandbox();
    put(dir.path(), "pkg/one.xsd", "<one/>");
    put(dir.path(), "pkg/two.xsd", "<two/>");
    let cat = catalog(
        r#"<uri name="http://example.com/x.xsd" uri="one.xsd"/>
           <uri name="http://example.com/x.xsd" uri="two.xsd"/>"#,
    );
    resolver
        .add_catalog_bytes(cat.as_bytes(), &base_of(&dir, "pkg/catalog.xml"))
        .unwrap();
    assert_eq!(
        resolver
            .resolve("http://example.com/x.xsd", None)
            .unwrap()
            .bytes,
        b"<one/>"
    );
}

#[test]
fn a_catalog_mapping_is_applied_once_not_iterated() {
    // XML Catalogs 1.1 §7.2.2: the mapped URI is returned as-is. A chain
    // http://a -> http://b -> a.xsd must therefore stop at http://b, which is a
    // blocked remote URI, rather than silently following the second hop.
    let (dir, mut resolver) = sandbox();
    let cat = catalog(
        r#"<uri name="http://a.example/x.xsd" uri="http://b.example/x.xsd"/>
           <uri name="http://b.example/x.xsd" uri="a.xsd"/>"#,
    );
    resolver
        .add_catalog_bytes(cat.as_bytes(), &base_of(&dir, "pkg/catalog.xml"))
        .unwrap();
    assert_eq!(
        rule_of(resolver.resolve("http://a.example/x.xsd", None)),
        rules::RESOLVER_REMOTE_BLOCKED
    );
    assert_eq!(
        resolver
            .resolve("http://b.example/x.xsd", None)
            .unwrap()
            .bytes,
        b"<a/>"
    );
}

#[test]
fn a_self_mapping_catalog_entry_cannot_loop_or_enable_remote_access() {
    let (dir, mut resolver) = sandbox();
    let cat = catalog(r#"<uri name="http://loop.example/x" uri="http://loop.example/x"/>"#);
    resolver
        .add_catalog_bytes(cat.as_bytes(), &base_of(&dir, "pkg/catalog.xml"))
        .unwrap();
    assert_eq!(
        rule_of(resolver.resolve("http://loop.example/x", None)),
        rules::RESOLVER_REMOTE_BLOCKED
    );
}

#[test]
fn a_catalog_cannot_map_to_a_path_outside_the_root() {
    let (dir, mut resolver) = sandbox();
    let cat = catalog(
        r#"<uri name="http://example.com/evil.xsd" uri="../outside.xsd"/>
           <rewriteURI uriStartString="http://example.com/up/" rewritePrefix="../"/>"#,
    );
    resolver
        .add_catalog_bytes(cat.as_bytes(), &base_of(&dir, "pkg/catalog.xml"))
        .unwrap();
    for uri in [
        "http://example.com/evil.xsd",
        "http://example.com/up/outside.xsd",
    ] {
        assert_eq!(
            rule_of(resolver.resolve(uri, None)),
            rules::RESOLVER_OUTSIDE_ROOT,
            "{uri}"
        );
    }
}

#[test]
fn xml_base_on_the_catalog_and_on_an_entry_changes_how_targets_resolve() {
    let (dir, mut resolver) = sandbox();
    let cat = format!(
        r#"<catalog xmlns="{CATALOG_NS}" xml:base="sub/">
             <uri name="http://example.com/b.xsd" uri="b.xsd"/>
             <uri name="http://example.com/a.xsd" uri="a.xsd" xml:base="../"/>
           </catalog>"#
    );
    resolver
        .add_catalog_bytes(cat.as_bytes(), &base_of(&dir, "pkg/catalog.xml"))
        .unwrap();
    assert_eq!(
        resolver
            .resolve("http://example.com/b.xsd", None)
            .unwrap()
            .bytes,
        b"<b/>"
    );
    assert_eq!(
        resolver
            .resolve("http://example.com/a.xsd", None)
            .unwrap()
            .bytes,
        b"<a/>"
    );
}

#[test]
fn entries_in_other_namespaces_and_system_public_entries_are_ignored() {
    let (dir, mut resolver) = sandbox();
    let cat = format!(
        r#"<catalog xmlns="{CATALOG_NS}" xmlns:x="urn:other">
             <x:uri name="http://example.com/a.xsd" uri="nope.xsd"/>
             <system systemId="http://example.com/s" uri="nope.xsd"/>
             <public publicId="p" uri="nope.xsd"/>
             <uri name="http://example.com/a.xsd" uri="a.xsd"/>
           </catalog>"#
    );
    resolver
        .add_catalog_bytes(cat.as_bytes(), &base_of(&dir, "pkg/catalog.xml"))
        .unwrap();
    assert_eq!(
        resolver
            .resolve("http://example.com/a.xsd", None)
            .unwrap()
            .bytes,
        b"<a/>"
    );
}

#[test]
fn unmatched_uris_fall_through_to_plain_local_resolution() {
    let (dir, mut resolver) = sandbox();
    let cat = catalog(r#"<uri name="http://example.com/other.xsd" uri="a.xsd"/>"#);
    resolver
        .add_catalog_bytes(cat.as_bytes(), &base_of(&dir, "pkg/catalog.xml"))
        .unwrap();
    assert_eq!(resolver.resolve("sub/b.xsd", None).unwrap().bytes, b"<b/>");
}

// --- catalogs: malformed / unsupported / hostile -----------------------------

#[test]
fn a_catalog_with_the_wrong_root_or_namespace_is_rejected() {
    let (dir, mut resolver) = sandbox();
    let base = base_of(&dir, "pkg/catalog.xml");
    for bad in [
        format!(r#"<notcatalog xmlns="{CATALOG_NS}"/>"#),
        r#"<catalog xmlns="http://example.com/other"/>"#.to_owned(),
        "<catalog/>".to_owned(),
    ] {
        assert_eq!(
            rule_of(resolver.add_catalog_bytes(bad.as_bytes(), &base)),
            rules::CATALOG_INVALID_ROOT,
            "{bad}"
        );
    }
}

#[test]
fn entries_missing_required_attributes_are_rejected() {
    let (dir, mut resolver) = sandbox();
    let base = base_of(&dir, "pkg/catalog.xml");
    for bad in [
        r#"<uri name="http://example.com/a"/>"#,
        r#"<uri uri="a.xsd"/>"#,
        r#"<uri name="" uri="a.xsd"/>"#,
        r#"<rewriteURI uriStartString="http://example.com/"/>"#,
        r#"<rewriteURI rewritePrefix="x/"/>"#,
        r#"<rewriteURI uriStartString="" rewritePrefix="x/"/>"#,
        r#"<nextCatalog/>"#,
    ] {
        assert_eq!(
            rule_of(resolver.add_catalog_bytes(catalog(bad).as_bytes(), &base)),
            rules::CATALOG_INVALID_ENTRY,
            "{bad}"
        );
    }
}

#[test]
fn catalog_features_that_affect_uri_mapping_but_are_unimplemented_fail_closed() {
    let (dir, mut resolver) = sandbox();
    let base = base_of(&dir, "pkg/catalog.xml");
    for unsupported in [
        r#"<uriSuffix uriSuffix="a.xsd" uri="a.xsd"/>"#,
        r#"<delegateURI uriStartString="http://e/" catalog="c.xml"/>"#,
        r#"<group><uri name="http://e/a" uri="a.xsd"/></group>"#,
    ] {
        assert_eq!(
            rule_of(resolver.add_catalog_bytes(catalog(unsupported).as_bytes(), &base)),
            rules::CATALOG_UNSUPPORTED,
            "{unsupported}"
        );
    }
}

#[test]
fn a_catalog_with_a_doctype_is_refused_so_no_entity_can_be_declared() {
    let (dir, mut resolver) = sandbox();
    let hostile = format!(
        r#"<?xml version="1.0"?><!DOCTYPE catalog [<!ENTITY x SYSTEM "file:///etc/passwd">]><catalog xmlns="{CATALOG_NS}">&x;</catalog>"#
    );
    assert_eq!(
        rule_of(resolver.add_catalog_bytes(hostile.as_bytes(), &base_of(&dir, "pkg/catalog.xml"))),
        rules::DOCTYPE_FORBIDDEN
    );
}

#[test]
fn a_malformed_catalog_reports_the_reader_rule() {
    let (dir, mut resolver) = sandbox();
    assert_eq!(
        rule_of(resolver.add_catalog_bytes(b"<catalog", &base_of(&dir, "pkg/catalog.xml"))),
        rules::MALFORMED
    );
}

#[test]
fn the_catalog_entry_budget_is_enforced() {
    let (dir, mut resolver) = sandbox();
    resolver = resolver.with_limits(ResolverLimits {
        max_catalog_entries: 2,
        ..ResolverLimits::default()
    });
    let base = base_of(&dir, "pkg/catalog.xml");
    let two =
        catalog(r#"<uri name="http://e/1" uri="a.xsd"/><uri name="http://e/2" uri="a.xsd"/>"#);
    resolver.add_catalog_bytes(two.as_bytes(), &base).unwrap();
    let one_more = catalog(r#"<uri name="http://e/3" uri="a.xsd"/>"#);
    assert_eq!(
        rule_of(resolver.add_catalog_bytes(one_more.as_bytes(), &base)),
        rules::RESOLVER_LIMIT_CATALOG
    );
}

#[test]
fn a_hostile_catalog_is_bounded_by_the_xml_reader_limits() {
    let (dir, mut resolver) = sandbox();
    resolver = resolver.with_limits(ResolverLimits {
        max_file_bytes: 256,
        ..ResolverLimits::default()
    });
    let big = catalog(&"<uri name=\"http://e/x\" uri=\"a.xsd\"/>".repeat(100));
    assert_eq!(
        rule_of(resolver.add_catalog_bytes(big.as_bytes(), &base_of(&dir, "pkg/catalog.xml"))),
        rules::LIMIT_DOCUMENT_SIZE
    );
}

// --- catalogs: nextCatalog chains and cycles ---------------------------------

#[test]
fn next_catalog_files_are_loaded_in_order_after_the_current_catalog() {
    let (dir, mut resolver) = sandbox();
    put(dir.path(), "pkg/special.xsd", "<special/>");
    put(
        dir.path(),
        "pkg/catalog.xml",
        &catalog(
            r#"<nextCatalog catalog="second.xml"/>
               <uri name="http://example.com/first.xsd" uri="a.xsd"/>"#,
        ),
    );
    put(
        dir.path(),
        "pkg/second.xml",
        &catalog(
            r#"<uri name="http://example.com/first.xsd" uri="special.xsd"/>
               <uri name="http://example.com/second.xsd" uri="sub/b.xsd"/>"#,
        ),
    );
    resolver.add_catalog_file("catalog.xml", None).unwrap();
    // The first catalog's own entries are consulted before its nextCatalog.
    assert_eq!(
        resolver
            .resolve("http://example.com/first.xsd", None)
            .unwrap()
            .bytes,
        b"<a/>"
    );
    assert_eq!(
        resolver
            .resolve("http://example.com/second.xsd", None)
            .unwrap()
            .bytes,
        b"<b/>"
    );
}

#[test]
fn next_catalog_resolves_relative_to_the_catalog_that_names_it() {
    let (dir, mut resolver) = sandbox();
    put(
        dir.path(),
        "pkg/sub/catalog.xml",
        &catalog(r#"<nextCatalog catalog="deeper/next.xml"/>"#),
    );
    put(
        dir.path(),
        "pkg/sub/deeper/next.xml",
        &catalog(r#"<uri name="http://example.com/n.xsd" uri="../../a.xsd"/>"#),
    );
    resolver.add_catalog_file("sub/catalog.xml", None).unwrap();
    assert_eq!(
        resolver
            .resolve("http://example.com/n.xsd", None)
            .unwrap()
            .bytes,
        b"<a/>"
    );
}

#[test]
fn a_next_catalog_cycle_is_detected() {
    let (dir, mut resolver) = sandbox();
    put(
        dir.path(),
        "pkg/one.xml",
        &catalog(r#"<nextCatalog catalog="two.xml"/>"#),
    );
    put(
        dir.path(),
        "pkg/two.xml",
        &catalog(r#"<nextCatalog catalog="one.xml"/>"#),
    );
    put(
        dir.path(),
        "pkg/self.xml",
        &catalog(r#"<nextCatalog catalog="self.xml"/>"#),
    );
    assert_eq!(
        rule_of(resolver.add_catalog_file("one.xml", None)),
        rules::CATALOG_CYCLE
    );
    assert_eq!(
        rule_of(resolver.add_catalog_file("self.xml", None)),
        rules::CATALOG_CYCLE
    );
}

#[test]
fn a_diamond_of_next_catalogs_is_not_a_cycle() {
    let (dir, mut resolver) = sandbox();
    put(
        dir.path(),
        "pkg/top.xml",
        &catalog(r#"<nextCatalog catalog="left.xml"/><nextCatalog catalog="right.xml"/>"#),
    );
    put(
        dir.path(),
        "pkg/left.xml",
        &catalog(r#"<nextCatalog catalog="leaf.xml"/>"#),
    );
    put(
        dir.path(),
        "pkg/right.xml",
        &catalog(r#"<nextCatalog catalog="leaf.xml"/>"#),
    );
    put(
        dir.path(),
        "pkg/leaf.xml",
        &catalog(r#"<uri name="http://example.com/a.xsd" uri="a.xsd"/>"#),
    );
    resolver.add_catalog_file("top.xml", None).unwrap();
    assert_eq!(
        resolver
            .resolve("http://example.com/a.xsd", None)
            .unwrap()
            .bytes,
        b"<a/>"
    );
}

#[test]
fn a_long_next_catalog_chain_hits_the_file_budget() {
    let (dir, mut resolver) = sandbox();
    resolver = resolver.with_limits(ResolverLimits {
        max_catalog_files: 3,
        ..ResolverLimits::default()
    });
    for i in 0..6 {
        put(
            dir.path(),
            &format!("pkg/c{i}.xml"),
            &catalog(&format!(r#"<nextCatalog catalog="c{}.xml"/>"#, i + 1)),
        );
    }
    assert_eq!(
        rule_of(resolver.add_catalog_file("c0.xml", None)),
        rules::RESOLVER_LIMIT_CATALOG
    );
}

#[test]
fn a_next_catalog_cannot_reach_a_remote_or_outside_file() {
    let (dir, mut resolver) = sandbox();
    put(
        dir.path(),
        "pkg/remote.xml",
        &catalog(r#"<nextCatalog catalog="http://example.com/catalog.xml"/>"#),
    );
    put(
        dir.path(),
        "pkg/escape.xml",
        &catalog(r#"<nextCatalog catalog="../outside.xsd"/>"#),
    );
    assert_eq!(
        rule_of(resolver.add_catalog_file("remote.xml", None)),
        rules::RESOLVER_REMOTE_BLOCKED
    );
    assert_eq!(
        rule_of(resolver.add_catalog_file("escape.xml", None)),
        rules::RESOLVER_OUTSIDE_ROOT
    );
}

#[test]
fn a_failed_catalog_load_leaves_no_partial_state_behind() {
    let (dir, mut resolver) = sandbox();
    put(
        dir.path(),
        "pkg/bad.xml",
        &catalog(
            r#"<uri name="http://example.com/a.xsd" uri="a.xsd"/><nextCatalog catalog="missing.xml"/>"#,
        ),
    );
    assert!(resolver.add_catalog_file("bad.xml", None).is_err());
    // The half-loaded catalog's entry must not be visible.
    assert_eq!(
        rule_of(resolver.resolve("http://example.com/a.xsd", None)),
        rules::RESOLVER_REMOTE_BLOCKED
    );
}
