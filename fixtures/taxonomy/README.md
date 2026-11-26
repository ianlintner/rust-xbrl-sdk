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
| `broken/missing-include.xsd` | Missing local schema reference is a rule-coded error, not an empty DTS. |
| `broken/cycle.xsd` | Cyclic `xs:include` terminates and reports a cycle error. |
| `broken/unresolved.xsd`, `broken/unresolved.xml` | A locator pointing at a nonexistent id is reported with its rule id. |
| `broken/remote.xsd` | Remote `xs:import` is blocked without any network I/O. |
| `broken/duplicate.xsd` | Duplicate `xs:element` id in one DTS is rejected. |
| `broken/presentation-cycle.xsd`, `broken/presentation-cycle.xml` | Cyclic presentation arcs are detected during graph traversal. |

Official conformance material (not copied; no pass/fail claim): the XBRL
2025-07-16 conformance archive at
<https://www.xbrl.org/2025/XBRL-CONF-2025-07-16.zip>. Relevant case families:
`Common/200-linkbase`, `Common/210-label`, `Common/220-presentation`,
`Common/230-calculation`, `Common/300-schema`. No license grant was established
for vendoring, so this fixture set stays authored and does not assert official
conformance.
