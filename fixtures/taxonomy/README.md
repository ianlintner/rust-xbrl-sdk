# Local DTS / linkbase fixtures

Provenance: **authored** for this repository from XBRL 2.1 Recommendation
2003-12-31 with errata through 2013-02-20 (§3 namespaces, §4.3 schema
references, §5 linkbases, §6 labels, §7 presentation/calculation), XML Schema
1.0 Part 1 (2nd ed., 2004-10-28) §3.2–3.4, XLink 1.0 (2001-06-27), XML Base
(2001-06-27) and RFC 3986 (2005-01). No content is vendored. Each file carries
its own authored provenance comment.

| Path | Proves |
|---|---|
| `example.xsd`, `example-types.xsd` | Deterministic local DTS discovery through `xs:include` and `link:linkbaseRef`; concept and type extraction; element identity by expanded name; abstract/nillable/periodType metadata. |
| `example-label.xml` | Label linkbase: `link:loc` → `xlink:href` id resolution, `label` resources by role and `xml:lang`, `labelArc` traversal. |
| `example-presentation.xml` | Presentation linkbase: `parent-child` arcs, `order`, `preferredLabel` and deterministic child ordering. |
| `example-calculation.xml` | Calculation linkbase: `summation-item` arcs, `order`, `weight`. |
| `fixes/std-label.xsd`, `fixes/std-label.xml` | Conventional distinct xlink labels (`loc_revenue` → `labelArc` → `lab_revenue`); orphan resource gets no label; XBRL 2.1 §5.2.2.3. |
| `fixes/scoped-*.xsd`, `fixes/scoped-*-label.xml` | XML IDs are document-scoped; the same `id="Item"` in two schemas resolves to different concepts; href document part is honoured. |
| `fixes/elrs.xsd`, `fixes/elrs.xml` | Opposite arcs in two extended-link roles are separate networks, not a graph cycle. |
| `fixes/calc-cycle.xsd`, `fixes/calc-cycle.xml` | `summation-item` cycles are allowed (XBRL 2.1 §5.2.5.2). |
| `broken/missing-include.xsd` | Missing local schema reference is a rule-coded error, not an empty DTS. |
| `broken/cycle.xsd` | Circular `xs:include` is legal and terminates via the visited set (XML Schema 1.0 §4.2.1). |
| `broken/unresolved.xsd`, `broken/unresolved.xml` | A locator pointing at a nonexistent id is reported with its rule id. |
| `broken/remote.xsd` | Remote `xs:import` is blocked without any network I/O. |
| `broken/duplicate.xsd` | Duplicate `xs:element` id in one schema is rejected. |
| `broken/presentation-cycle.xsd`, `broken/presentation-cycle.xml` | Cyclic presentation `parent-child` arcs within one ELR are detected. |
| `broken/wrong-document-locator.*` | A locator naming an existing id in an undiscovered document fails closed. |
| `broken/schema-root-locator.*` | An id on `xs:schema` is not a concept; locators to it fail closed. |
| `broken/prohibited-arc.*`, `broken/priority-arc.*` | Unsupported arc `use`/`priority` are reported instead of silently becoming relationships. `use` uses the standard unprefixed form from XBRL 2.1 §3.5.3.9.7.4 / xl-2003-12-31.xsd. |
| `broken/type-locator.*` | A locator to a named `xs:simpleType` id is not a concept and fails closed (XBRL 2.1 §5.2.4.2). |
| `fixes/shared-locator.*` | An xlink label shared by two locators fans out to both relationship targets in document order (XLink 1.0 §5.1.3). |
| `broken/bad-decimal.xml`, `broken/bad-decimal.xsd` | Non-decimal `order` (`NaN`) is rejected (XML Schema 1.0 §3.2.3). |

Official conformance material (not copied; no pass/fail claim): the XBRL
2025-07-16 conformance archive at
<https://www.xbrl.org/2025/XBRL-CONF-2025-07-16.zip>. Relevant case families:
`Common/200-linkbase`, `Common/210-label`, `Common/220-presentation`,
`Common/230-calculation`, `Common/300-schema`. No license grant was established
for vendoring, so this fixture set stays authored and does not assert official
conformance.
