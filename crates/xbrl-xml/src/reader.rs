//! Streaming namespace-aware XML reader with hard resource limits.
//!
//! [`XmlReader`] drives `quick_xml`'s event reader and adds what XBRL needs on
//! top of a tokenizer:
//!
//! * **Namespaces** (Namespaces in XML 1.0 §3, §6): every element and attribute
//!   name is resolved to a namespace URI before it is returned; the `xmlns`
//!   rules are enforced; unprefixed attributes have no namespace.
//! * **Limits** ([`Limits`]): bytes read, nesting depth, attributes per element,
//!   token size and namespace bindings are checked eagerly, so hostile input
//!   fails with a typed [`Diagnostic`] instead of exhausting memory or stack.
//!   The reader is iterative: nesting never recurses.
//! * **Fail-closed entities**: DOCTYPE declarations (and therefore every
//!   internal/external entity) are rejected; only the five predefined entities
//!   and character references are expanded. Nothing is ever fetched.
//! * **Encoding**: UTF-8 only. A declared non-UTF-8 encoding or XML 1.1 is
//!   reported as unsupported rather than guessed at.
//!
//! Memory is bounded by `max_document_bytes` (the tokenizer buffers at most one
//! markup token, which can never exceed the document) plus the output of the
//! caller's own accumulation; text and attribute values are additionally
//! bounded by `max_token_bytes`.

use std::collections::{HashSet, VecDeque};
use std::io::{self, BufReader, Read};

use quick_xml::Error as XmlError;
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use xbrl_core::diagnostic::{Diagnostic, RuleId, Severity, SourceSpan};
use xbrl_core::qname::QName;

use crate::limits::Limits;
use crate::rules;

/// The namespace bound to the predefined `xml` prefix (Namespaces §3).
pub const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";
/// The namespace reserved for `xmlns` declarations (Namespaces §3).
pub const XMLNS_NAMESPACE: &str = "http://www.w3.org/2000/xmlns/";

/// An element or attribute name resolved against the in-scope namespaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedName {
    namespace: Option<String>,
    local_name: String,
    prefix: Option<String>,
}

impl ResolvedName {
    /// The resolved namespace URI, or `None` for "no namespace".
    #[must_use]
    pub fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref()
    }

    /// The local name, without any prefix.
    #[must_use]
    pub fn local_name(&self) -> &str {
        &self.local_name
    }

    /// The lexical prefix as written in the source, if any.
    #[must_use]
    pub fn prefix(&self) -> Option<&str> {
        self.prefix.as_deref()
    }

    /// Converts to an [`xbrl_core::qname::QName`]; `None` when the name is in no
    /// namespace (a `QName` always carries a namespace URI).
    #[must_use]
    pub fn to_qname(&self) -> Option<QName> {
        QName::new(self.prefix(), self.namespace()?, &self.local_name).ok()
    }
}

/// A namespace declaration (`xmlns` / `xmlns:p`) made on an element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceBinding {
    prefix: Option<String>,
    uri: String,
}

impl NamespaceBinding {
    /// The declared prefix, or `None` for a default-namespace declaration.
    #[must_use]
    pub fn prefix(&self) -> Option<&str> {
        self.prefix.as_deref()
    }

    /// The namespace URI. Empty only for `xmlns=""` (default namespace reset).
    #[must_use]
    pub fn uri(&self) -> &str {
        &self.uri
    }
}

/// A non-`xmlns` attribute with a resolved name and normalized value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    /// Resolved attribute name (unprefixed attributes are in no namespace).
    pub name: ResolvedName,
    /// Value after entity expansion and attribute-value normalization
    /// (XML 1.0 §3.3.3).
    pub value: String,
}

/// An element start tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartElement {
    /// Resolved element name.
    pub name: ResolvedName,
    /// Attributes in document order, excluding namespace declarations.
    pub attributes: Vec<Attribute>,
    /// Namespace declarations made on this element, in document order.
    pub namespaces: Vec<NamespaceBinding>,
    /// Byte span of the start tag in the source.
    pub span: SourceSpan,
}

/// An element end tag. A self-closing element yields a `Start` followed by an
/// `End` carrying the same span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndElement {
    /// Resolved element name.
    pub name: ResolvedName,
    /// Byte span of the end tag in the source.
    pub span: SourceSpan,
}

/// A run of character data. Text, CDATA sections and entity/character
/// references between two tags are coalesced into one event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextContent {
    /// Decoded text with line endings normalized to `\n` (XML 1.0 §2.11).
    pub text: String,
    /// Byte span covering the whole run in the source.
    pub span: SourceSpan,
}

/// One item of the event stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XmlEvent {
    /// An element start tag.
    Start(StartElement),
    /// An element end tag.
    End(EndElement),
    /// Character data inside an element.
    Text(TextContent),
}

/// Caps the number of bytes pulled from the source. Reads one byte past the
/// limit to distinguish "exactly at the limit" from "over it".
struct LimitedRead<R> {
    inner: R,
    limit: u64,
    read: u64,
    exceeded: bool,
}

impl<R: Read> Read for LimitedRead<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.exceeded {
            return Err(io::Error::other("document byte limit exceeded"));
        }
        let allowed = self.limit.saturating_sub(self.read).saturating_add(1);
        let max = usize::try_from(allowed)
            .unwrap_or(usize::MAX)
            .min(buf.len());
        let n = self.inner.read(&mut buf[..max])?;
        self.read += n as u64;
        if self.read > self.limit {
            self.exceeded = true;
            return Err(io::Error::other("document byte limit exceeded"));
        }
        Ok(n)
    }
}

/// Bounded, namespace-aware streaming XML reader. See the module docs.
pub struct XmlReader<R: Read> {
    inner: Reader<BufReader<LimitedRead<R>>>,
    limits: Limits,
    buf: Vec<u8>,
    /// In-scope namespace bindings, innermost last. `None` key = default namespace.
    bindings: Vec<(Option<String>, String)>,
    /// Open elements: resolved name plus the `bindings` length to restore on close.
    open: Vec<(ResolvedName, usize)>,
    queue: VecDeque<XmlEvent>,
    text: String,
    text_span: Option<(usize, usize)>,
    /// Span of the raw token being processed, used for error locations.
    current: (usize, usize),
    root_seen: bool,
    at_eof: bool,
    error: Option<Diagnostic>,
}

impl<R: Read> XmlReader<R> {
    /// Creates a reader over `source` that enforces `limits`.
    pub fn new(source: R, limits: Limits) -> Self {
        let limited = LimitedRead {
            inner: source,
            limit: limits.max_document_bytes,
            read: 0,
            exceeded: false,
        };
        let mut inner = Reader::from_reader(BufReader::new(limited));
        let config = inner.config_mut();
        config.trim_text(false);
        config.expand_empty_elements = false;
        config.check_end_names = true;
        config.check_comments = true;
        Self {
            inner,
            limits,
            buf: Vec::new(),
            bindings: Vec::new(),
            open: Vec::new(),
            queue: VecDeque::new(),
            text: String::new(),
            text_span: None,
            current: (0, 0),
            root_seen: false,
            at_eof: false,
            error: None,
        }
    }

    /// Returns the next event, `Ok(None)` once the document ended cleanly.
    ///
    /// After the first error every further call returns that same error.
    ///
    /// # Errors
    ///
    /// A [`Diagnostic`] with a stable rule id from [`crate::rules`] when a limit
    /// is exceeded, the document is not well-formed, a namespace constraint is
    /// violated, or the document uses a construct this reader refuses (DOCTYPE,
    /// non-UTF-8 encoding, XML 1.1).
    pub fn next_event(&mut self) -> Result<Option<XmlEvent>, Diagnostic> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        loop {
            if let Some(event) = self.queue.pop_front() {
                return Ok(Some(event));
            }
            if self.at_eof {
                return Ok(None);
            }
            if let Err(error) = self.pull() {
                self.error = Some(error.clone());
                return Err(error);
            }
        }
    }

    /// Reads and handles one raw token from the tokenizer.
    fn pull(&mut self) -> Result<(), Diagnostic> {
        let start = position(self.inner.buffer_position());
        self.current = (start, start);
        let mut buf = std::mem::take(&mut self.buf);
        buf.clear();
        let result = self.inner.read_event_into(&mut buf);
        let end = position(self.inner.buffer_position()).max(start);
        self.current = (start, end);
        let outcome = match result {
            Ok(event) => self.handle(&event),
            Err(error) => Err(self.convert(&error)),
        };
        self.buf = buf;
        outcome
    }

    fn handle(&mut self, event: &Event<'_>) -> Result<(), Diagnostic> {
        match event {
            Event::Decl(decl) => self.handle_decl(decl),
            Event::Start(tag) => {
                self.flush_text();
                self.start_element(tag, false)
            }
            Event::Empty(tag) => {
                self.flush_text();
                self.start_element(tag, true)
            }
            Event::End(_) => {
                self.flush_text();
                self.end_element()
            }
            Event::Text(raw) => self.handle_text(raw, false),
            Event::CData(raw) => self.handle_text(raw, true),
            Event::GeneralRef(reference) => {
                if self.open.is_empty() {
                    return Err(self.malformed("entity reference outside the root element"));
                }
                let name = self.utf8(reference)?;
                let ch = self.resolve_reference(name)?;
                let mut tmp = [0_u8; 4];
                self.append_text(ch.encode_utf8(&mut tmp))
            }
            Event::Comment(raw) => self.check_token(raw.len() as u64),
            Event::PI(raw) => self.check_token(raw.len() as u64),
            Event::DocType(_) => Err(self.error(
                rules::DOCTYPE_FORBIDDEN,
                "DOCTYPE declarations (and so all entity declarations) are not permitted",
            )),
            Event::Eof => {
                self.flush_text();
                if !self.open.is_empty() {
                    return Err(self.malformed("unexpected end of document: unclosed element"));
                }
                if !self.root_seen {
                    return Err(self.malformed("document has no root element"));
                }
                self.at_eof = true;
                Ok(())
            }
        }
    }

    fn handle_decl(&mut self, decl: &quick_xml::events::BytesDecl<'_>) -> Result<(), Diagnostic> {
        let version = decl.version().map_err(|e| self.convert(&e))?;
        let version = self.utf8(&version)?;
        if version != "1.0" {
            return Err(self.error(
                rules::UNSUPPORTED_VERSION,
                format!("XML version {version:?} is not supported; only 1.0 is"),
            ));
        }
        if let Some(encoding) = decl.encoding() {
            let encoding = encoding.map_err(|_| self.malformed("malformed XML declaration"))?;
            let encoding = self.utf8(&encoding)?;
            if !encoding.eq_ignore_ascii_case("utf-8") {
                return Err(self.error(
                    rules::UNSUPPORTED_ENCODING,
                    format!("encoding {encoding:?} is not supported; only UTF-8 is"),
                ));
            }
        }
        Ok(())
    }

    fn handle_text(&mut self, raw: &[u8], cdata: bool) -> Result<(), Diagnostic> {
        let text = self.utf8(raw)?;
        if self.open.is_empty() {
            // Prolog / epilog: only whitespace is well-formed (XML 1.0 [27] Misc).
            if cdata
                || !text
                    .bytes()
                    .all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
            {
                return Err(self.malformed("character data outside the root element"));
            }
            return Ok(());
        }
        if let Some(bad) = text.chars().find(|&c| !is_xml_char(c as u32)) {
            return Err(
                self.malformed(format!("U+{:04X} is not a legal XML character", bad as u32))
            );
        }
        let normalized = normalize_line_endings(text);
        self.append_text(&normalized)
    }

    fn append_text(&mut self, piece: &str) -> Result<(), Diagnostic> {
        let total = self.text.len() as u64 + piece.len() as u64;
        self.check_token(total)?;
        if self.text_span.is_none() {
            self.text_span = Some(self.current);
        }
        if let Some((_, end)) = &mut self.text_span {
            *end = self.current.1;
        }
        self.text.push_str(piece);
        Ok(())
    }

    fn flush_text(&mut self) {
        if let Some((start, end)) = self.text_span.take() {
            let text = std::mem::take(&mut self.text);
            self.queue.push_back(XmlEvent::Text(TextContent {
                text,
                span: make_span(start, end),
            }));
        }
    }

    fn start_element(&mut self, tag: &BytesStart<'_>, empty: bool) -> Result<(), Diagnostic> {
        if self.open.is_empty() && self.root_seen {
            return Err(self.malformed("content after the root element"));
        }
        self.root_seen = true;
        if self.open.len() >= self.limits.max_depth as usize {
            return Err(self.error(
                rules::LIMIT_DEPTH,
                format!(
                    "element nesting exceeds the limit of {}",
                    self.limits.max_depth
                ),
            ));
        }
        let element_name = self.utf8(tag.name().as_ref())?.to_owned();
        self.check_token(element_name.len() as u64)?;

        // Collect raw attributes; split namespace declarations from the rest.
        let mut seen_keys: HashSet<String> = HashSet::new();
        let mut plain: Vec<(String, String)> = Vec::new();
        let mut declarations: Vec<NamespaceBinding> = Vec::new();
        let mut count = 0_u32;
        for attribute in tag.attributes().with_checks(false) {
            count += 1;
            if count > self.limits.max_attributes_per_element {
                return Err(self.error(
                    rules::LIMIT_ATTRIBUTES,
                    format!(
                        "element has more than {} attributes",
                        self.limits.max_attributes_per_element
                    ),
                ));
            }
            let attribute = attribute.map_err(|_| self.malformed("malformed attribute"))?;
            let key = self.utf8(attribute.key.as_ref())?.to_owned();
            self.check_token(key.len() as u64)?;
            self.check_token(attribute.value.len() as u64)?;
            let value = self.decode_attribute_value(&attribute.value)?;
            if !seen_keys.insert(key.clone()) {
                return Err(self.error(
                    rules::DUPLICATE_ATTRIBUTE,
                    format!("attribute {key:?} appears twice on one element"),
                ));
            }
            if key == "xmlns" {
                declarations.push(self.validated_binding(None, value)?);
            } else if let Some(prefix) = key.strip_prefix("xmlns:") {
                self.check_ncname(prefix)?;
                declarations.push(self.validated_binding(Some(prefix.to_owned()), value)?);
            } else {
                plain.push((key, value));
            }
        }

        // Bring this element's declarations into scope, then resolve names.
        let mark = self.bindings.len();
        if mark + declarations.len() > self.limits.max_namespace_bindings as usize {
            return Err(self.error(
                rules::LIMIT_NAMESPACE_BINDINGS,
                format!(
                    "more than {} namespace bindings in scope",
                    self.limits.max_namespace_bindings
                ),
            ));
        }
        for binding in &declarations {
            self.bindings
                .push((binding.prefix.clone(), binding.uri.clone()));
        }
        let name = self.resolve_name(&element_name, true)?;
        let mut attributes = Vec::with_capacity(plain.len());
        let mut expanded: HashSet<(Option<String>, String)> = HashSet::new();
        for (key, value) in plain {
            let resolved = self.resolve_name(&key, false)?;
            if !expanded.insert((resolved.namespace.clone(), resolved.local_name.clone())) {
                return Err(self.error(
                    rules::DUPLICATE_ATTRIBUTE,
                    format!("two attributes expand to the same name {key:?}"),
                ));
            }
            attributes.push(Attribute {
                name: resolved,
                value,
            });
        }

        let span = make_span(self.current.0, self.current.1);
        self.queue.push_back(XmlEvent::Start(StartElement {
            name: name.clone(),
            attributes,
            namespaces: declarations,
            span,
        }));
        if empty {
            self.bindings.truncate(mark);
            self.queue
                .push_back(XmlEvent::End(EndElement { name, span }));
        } else {
            self.open.push((name, mark));
        }
        Ok(())
    }

    fn end_element(&mut self) -> Result<(), Diagnostic> {
        let Some((name, mark)) = self.open.pop() else {
            return Err(self.malformed("end tag with no open element"));
        };
        self.bindings.truncate(mark);
        self.queue.push_back(XmlEvent::End(EndElement {
            name,
            span: make_span(self.current.0, self.current.1),
        }));
        Ok(())
    }

    /// Validates a namespace declaration (Namespaces §3, §6.1 for 1.0).
    fn validated_binding(
        &self,
        prefix: Option<String>,
        uri: String,
    ) -> Result<NamespaceBinding, Diagnostic> {
        let invalid = |message: String| self.error(rules::INVALID_NAMESPACE_DECLARATION, message);
        match prefix.as_deref() {
            Some("xmlns") => return Err(invalid("the prefix xmlns must not be declared".into())),
            Some("xml") if uri != XML_NAMESPACE => {
                return Err(invalid(format!(
                    "the prefix xml must not be bound to {uri:?}"
                )));
            }
            Some(p) if p != "xml" && uri.is_empty() => {
                return Err(invalid(format!(
                    "prefix {p:?} cannot be undeclared in Namespaces 1.0"
                )));
            }
            _ => {}
        }
        if uri == XMLNS_NAMESPACE {
            return Err(invalid(format!(
                "the namespace {XMLNS_NAMESPACE} must not be declared"
            )));
        }
        if uri == XML_NAMESPACE && prefix.as_deref() != Some("xml") {
            return Err(invalid(format!(
                "the namespace {XML_NAMESPACE} may only be bound to the xml prefix"
            )));
        }
        Ok(NamespaceBinding { prefix, uri })
    }

    /// Resolves a lexical name. `is_element` selects whether the default
    /// namespace applies (it does not for attributes, Namespaces §6.2).
    fn resolve_name(&self, raw: &str, is_element: bool) -> Result<ResolvedName, Diagnostic> {
        let (prefix, local) = match raw.split_once(':') {
            Some((prefix, local)) => (Some(prefix), local),
            None => (None, raw),
        };
        if let Some(prefix) = prefix {
            self.check_ncname(prefix)?;
        }
        self.check_ncname(local)?;
        let namespace = match prefix {
            Some(prefix) => Some(self.lookup(Some(prefix)).ok_or_else(|| {
                self.error(
                    rules::UNBOUND_PREFIX,
                    format!("prefix {prefix:?} in {raw:?} is not bound to a namespace"),
                )
            })?),
            None if is_element => self.lookup(None),
            None => None,
        };
        Ok(ResolvedName {
            namespace: namespace.filter(|uri| !uri.is_empty()).map(str::to_owned),
            local_name: local.to_owned(),
            prefix: prefix.map(str::to_owned),
        })
    }

    fn lookup(&self, prefix: Option<&str>) -> Option<&str> {
        let found = self
            .bindings
            .iter()
            .rev()
            .find(|(bound, _)| bound.as_deref() == prefix)
            .map(|(_, uri)| uri.as_str());
        match (found, prefix) {
            (None, Some("xml")) => Some(XML_NAMESPACE),
            _ => found,
        }
    }

    /// Light NCName check: no empty parts, no extra colons, and a legal start
    /// and body character set. Not the full XML `NameStartChar` table; every
    /// non-ASCII character is accepted.
    fn check_ncname(&self, part: &str) -> Result<(), Diagnostic> {
        let mut chars = part.chars();
        let valid = match chars.next() {
            None => false,
            Some(first) => {
                (first.is_alphabetic() || first == '_' || !first.is_ascii())
                    && chars.all(|c| {
                        c.is_alphanumeric() || matches!(c, '_' | '-' | '.') || !c.is_ascii()
                    })
            }
        };
        if valid {
            Ok(())
        } else {
            Err(self.error(rules::INVALID_NAME, format!("{part:?} is not a valid name")))
        }
    }

    /// Expands references and normalizes an attribute value (XML 1.0 §3.3.3).
    fn decode_attribute_value(&self, raw: &[u8]) -> Result<String, Diagnostic> {
        let text = self.utf8(raw)?;
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '<' => return Err(self.malformed("'<' is not allowed in an attribute value")),
                '&' => {
                    let mut name = String::new();
                    loop {
                        match chars.next() {
                            Some(';') => break,
                            Some(n) if name.len() < 32 => name.push(n),
                            _ => return Err(self.malformed("unterminated entity reference")),
                        }
                    }
                    out.push(self.resolve_reference(&name)?);
                }
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    out.push(' ');
                }
                '\t' | '\n' => out.push(' '),
                c if is_xml_char(c as u32) => out.push(c),
                c => {
                    return Err(
                        self.malformed(format!("U+{:04X} is not a legal XML character", c as u32))
                    );
                }
            }
        }
        Ok(out)
    }

    /// Resolves the body of `&...;`: five predefined entities or a character
    /// reference. Everything else is undeclared because DOCTYPE is refused.
    fn resolve_reference(&self, name: &str) -> Result<char, Diagnostic> {
        let invalid = || {
            self.error(
                rules::INVALID_CHARACTER_REFERENCE,
                format!("&{name}; is not a legal XML 1.0 character reference"),
            )
        };
        match name {
            "lt" => return Ok('<'),
            "gt" => return Ok('>'),
            "amp" => return Ok('&'),
            "apos" => return Ok('\''),
            "quot" => return Ok('"'),
            _ => {}
        }
        let Some(number) = name.strip_prefix('#') else {
            return Err(self.error(
                rules::UNDEFINED_ENTITY,
                format!("entity &{name}; is not declared (DOCTYPE is not permitted)"),
            ));
        };
        let (digits, radix) = match number.strip_prefix('x') {
            Some(hex) => (hex, 16),
            None => (number, 10),
        };
        if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
            return Err(invalid());
        }
        let code = u32::from_str_radix(digits, radix).map_err(|_| invalid())?;
        if !is_xml_char(code) {
            return Err(invalid());
        }
        char::from_u32(code).ok_or_else(invalid)
    }

    fn check_token(&self, len: u64) -> Result<(), Diagnostic> {
        if len > self.limits.max_token_bytes {
            return Err(self.error(
                rules::LIMIT_TOKEN_SIZE,
                format!(
                    "token exceeds the limit of {} bytes",
                    self.limits.max_token_bytes
                ),
            ));
        }
        Ok(())
    }

    fn utf8<'a>(&self, bytes: &'a [u8]) -> Result<&'a str, Diagnostic> {
        std::str::from_utf8(bytes)
            .map_err(|_| self.error(rules::INVALID_ENCODING, "document is not valid UTF-8"))
    }

    fn convert(&self, error: &XmlError) -> Diagnostic {
        match error {
            XmlError::Io(_) => {
                if self.inner.get_ref().get_ref().exceeded {
                    self.error(
                        rules::LIMIT_DOCUMENT_SIZE,
                        format!(
                            "document exceeds the limit of {} bytes",
                            self.limits.max_document_bytes
                        ),
                    )
                } else {
                    self.error(rules::IO_ERROR, "failed to read the document")
                }
            }
            XmlError::Encoding(_) => {
                self.error(rules::INVALID_ENCODING, "document is not valid UTF-8")
            }
            other => self.malformed(format!("not well-formed XML: {other}")),
        }
    }

    fn malformed(&self, message: impl Into<String>) -> Diagnostic {
        self.error(rules::MALFORMED, message)
    }

    fn error(&self, rule: &str, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(
            RuleId::new(rule).expect("rule constants are valid rule ids"),
            Severity::Error,
            message,
            Some(make_span(self.current.0, self.current.1)),
        )
    }
}

fn position(offset: u64) -> usize {
    usize::try_from(offset).unwrap_or(usize::MAX)
}

fn make_span(start: usize, end: usize) -> SourceSpan {
    SourceSpan::new(start, end.max(start)).expect("end is clamped to start")
}

/// XML 1.0 (5th ed.) production \[2\] `Char`.
fn is_xml_char(code: u32) -> bool {
    matches!(code, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x1_0000..=0x10_FFFF)
}

/// XML 1.0 §2.11: `\r\n` and lone `\r` become `\n`.
fn normalize_line_endings(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_owned();
    }
    text.replace("\r\n", "\n").replace('\r', "\n")
}
