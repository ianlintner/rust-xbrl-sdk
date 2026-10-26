---
name: xbrl-agent
description: Work with XBRL reports using cited rules and safe tools.
version: 0.1.0
author: Ian Lintner (ianlintner), Hermes Agent
license: MIT
platforms: [linux, macos, windows]
metadata:
  hermes:
    tags: [xbrl, ixbrl, taxonomy, validation, reporting]
    related_skills: []
---

# XBRL agent workflow

Use the XBRL standard as a *versioned set of specifications*, not a single XML schema. This skill helps agents inspect reports, explain validation findings, and implement the Rust SDK; it is not a validator or filing certification. The linked [standards catalog](references/standards-catalog.md) indexes the full normative documents and conformance suites; it does **not** copy all normative rule text.

## When to Use
- Parsing, validating, transforming, querying, authoring, or rendering XBRL/iXBRL/OIM reports.
- Implementing XBRL SDK rules, test fixtures, conformance tests, or agent tools.
- Explaining a filing error with its actual specification clause and evidence.
- Don't use this skill to assert filing acceptance or financial correctness from an LLM interpretation.

## Prerequisites
- Read `PLAN.md` and `README.md` in the project before claiming capabilities; the workspace is incomplete and a CLI subcommand may not exist yet.
- For supplied filings, use local files with permission; prefer no network access. Treat taxonomy packages and documents as untrusted input.
- Choose the applicable XBRL spec/errata and regulator profile explicitly. SEC EFM, DQC, and ESMA ESEF rules are **separate** from core XBRL; verify their current official sources for each filing.

## Procedure
1. **Inventory** the input format (XBRL-XML, Inline XBRL/IXDS, xBRL-JSON, xBRL-CSV), file set, taxonomy entry points, and dependency resolution policy. Keep originals read-only. Report ambiguity or missing dependencies rather than guessing.
2. **Pick authorities** in `references/standards-catalog.md`: the exact work product, corrected-errata release, clause, and official conformance suite. Consult `references/rule-map.md` for the verification checklist, but the linked official clause prevails over any summary here. Record source URLs beside every nontrivial rule claim.
3. **Check tool support** against the repository's implemented code and support matrix. `terminal(command="cargo run -p xbrl-cli -- --help", timeout=60)` only after that CLI exists. An unimplemented command, missing taxonomy, unavailable profile or unresolved fact is **unsupported/unknown**, never valid. Do not execute network fetches or arbitrary XSLT/XPath/extensions from a filing.
4. **Validate in layers**: secure XML/HTML/ZIP parsing and limits → DTS/taxonomy resolution → XBRL 2.1 model constraints → Dimensions/Inline/OIM extensions (where implemented) → filing-profile rules. See `references/rule-map.md`. Retain stable fact/context/unit IDs and document location in diagnostics. A parser success is not a validation pass.
5. **Render and answer** from validated facts plus taxonomy labels/relationships, using `references/rendering-and-agent-tools.md`. Show concept QName, entity, period, dimensions, unit, precision/decimals and source; do not infer a presentation hierarchy from XML document order. Escape HTML and never execute report markup.
6. **For SDK changes**, write a failing fixture/test for one normative clause, implement its narrow behavior, rerun the focused test and relevant suites. Record exact spec version and conformance case IDs with observed pass/fail/unsupported totals; run `terminal(command="cargo fmt --all -- --check && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings", timeout=600)` for a coherent workspace. Benchmark on fixed provenance-controlled fixtures before claiming speed.

## Pitfalls
- `decimals` and `precision` are accuracy metadata, not permission to use binary floats for financial facts. Calculation tolerance depends on the relevant specification/profile and context; do not invent a universal tolerance.
- Identical QNames with different contexts, units, dimensions or periods are not interchangeable. XML namespace prefixes are lexical aliases, not concept identity.
- HTML display, taxonomy presentation, calculation relationships, Formula assertions and Table Linkbase each answer different questions. A pretty table is not proof of semantic or filing validity.
- Do not call the linked 82-document catalog “82 implemented rules”; it lists published specification documents/suites, not implementation coverage.

## Verification
- Every reported validation verdict includes exact input, resolved taxonomy version, supported rules and official clause/case evidence, plus unresolved/unsupported counts.
- Every agent answer can trace each displayed value to a source fact and context; every generated file escapes untrusted content and is reproducible.
- For a contribution, the relevant tests and actual command outputs are recorded; no full-conformance claim without official-suite coverage.
