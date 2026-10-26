# Rule discovery and validation map

This is a **checklist of rule families**, not a substitute for normative text or a complete enumeration of clauses. For the complete published documents and official suites, follow [standards-catalog.md](standards-catalog.md). Verify each rule in the exact linked Recommendation and its errata before coding or diagnosing a report. A validator must report unimplemented rule families explicitly.

## Core XBRL 2.1
- Instance root, namespace-qualified concept names, schemaRef/linkbaseRef, DTS discovery and concept declarations: distinguish lexical XML well-formedness from DTS and XBRL validity.
- Item/tuple semantics, contextRef/unitRef references, nil, numeric accuracy, types, units (including divide), entities and period variants (instant/duration/forever).
- Labels, references, presentation, calculation and definition relationships: validate extended links, arcs, role/arcrole declarations, target resolution and prohibited/overriding arcs as required by the exact clauses. Do not equate presentation order with calculation order.
- Use the [XBRL 2.1 full specification](https://www.xbrl.org/Specification/XBRL-2.1/REC-2003-12-31/XBRL-2.1-REC-2003-12-31+corrected-errata-2013-02-20.html) and its [official suite](https://www.xbrl.org/2025/XBRL-CONF-2025-07-16.zip).

## Dimensions, calculations and Formula
- Dimensions: hypercube/all/notAll, primary items, domain-member, explicit vs typed members, defaults, segment/scenario placement and dimensional validity. [Dimensions Recommendation](https://www.xbrl.org/specification/dimensions/rec-2012-01-25/dimensions-rec-2006-09-18+corrected-errata-2012-01-25-clean.html); [suite](https://www.xbrl.org/2025/XBRL-XDT-CONF-2025-09-09.zip).
- Calculations: select the correct XBRL 2.1 or Calculations 1.1 specification. Handle rounding, duplicates, context/unit equality and scoped summation with the chosen normative clauses—not an ad hoc float comparison. Find Calculations 1.1 in the catalog.
- Formula: variables, filters, aspect matching, value/existence/consistency assertions and severity are a *suite* of normative modules, not a single Boolean rule. See the catalog's Formula entries and suite.

## Inline XBRL, OIM and packages
- Inline XBRL: document-set discovery, fact extraction, hidden facts, continuations, exclusions, footnotes, transformation registry, XHTML and text safety. [Inline XBRL 1.1](https://www.xbrl.org/Specification/inlineXBRL-part1/REC-2013-11-18+errata-2026-07-14/inlineXBRL-part1-REC-2013-11-18+corrected-errata-2026-07-14.html); suite linked in catalog.
- OIM: syntax-independent model plus separate xBRL-XML, xBRL-JSON and xBRL-CSV mappings. Conversion is not universally lossless; classify unsupported XML constructs instead of silently dropping them. Follow each OIM document in catalog.
- Taxonomy/report packages: ZIP structure, metadata, entry points, catalog remapping and resolver limits. Reject traversal/expansion bombs and ambiguous or untrusted remote retrieval. Follow package recommendations in catalog.
- Extensible enumerations, table linkbase, generic links, registries, and versioning each have separate recommendation and conformance status; never mark them tested because the core parser passed.

## Rule-to-test record (copy into each implementation PR)
```
Spec/work product + version + errata URL:
Normative clause ID and exact expected behavior:
Official suite archive URL + test case IDs (or reason none applies):
Input and taxonomy fixture provenance/license:
Expected positive and negative outcomes:
Observed pass / fail / unsupported counts:
Current SDK support status and diagnostics code:
```

Regulatory filing acceptance requires an independently versioned SEC/ESMA/DQC rule catalog and actual regulator validation. A green core XBRL suite is not a filing approval.
