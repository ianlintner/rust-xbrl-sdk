# Rust XBRL SDK — plan

## Goal and assumptions
A pure-Rust, high-performance SDK and headless CLI for XBRL authoring, ingestion, validation, querying, and common rendering. No UI or service dependency. Repository `ianlintner/rust-xbrl-sdk` is public under the MIT license, as chosen by Ian. A complete standards implementation is a multi-release program, not an MVP claim. Prefer existing Rust XML/HTML/ZIP primitives after security and license review, but implement XBRL semantics in Rust; no Arelle/Python runtime. Preserve unknown extension content when feasible; fail closed on unsupported validation rather than declaring success.

## Architecture
Cargo workspace: `xbrl-core` (qualified names, values, contexts, units, facts, OIM), `xbrl-taxonomy` (DTS resolution, schemas, linkbases, taxonomy packages/catalog), `xbrl-xml` (XBRL 2.1), `xbrl-inline` (iXBRL), `xbrl-validate` (rule engine), `xbrl-render` (headless HTML/text/CSV tables), `xbrl-cli`. Keep stable typed diagnostics with rule IDs, source spans, severity; limits on XML/ZIP expansion and resolver IO. Fast path: streaming XML, interning, indexed lookups, bounded caches, measured benchmark fixtures and no performance claim without data. Separate parsing, semantic validation, and profile rules (SEC EFM/ESMA/DQC).

## MVP (honest subset)
1. Workspace, CI, fixtures, docs and benchmarking harness.
2. Secure resolver and streaming XML base with namespace handling and input limits.
3. XBRL 2.1 instance facts, contexts, units, decimals/precision, schema refs; typed API.
4. DTS discovery and taxonomy concept/type/labels and presentation/calculation linkbases.
5. Core validation (structural, references, duplicate consistency, calculation checks) with diagnostic provenance.
6. Headless report summaries and common presentation-tree rendering to text/HTML/CSV; escaping and deterministic output.
7. CLI parse/validate/render with JSON diagnostics and developer API example.
8. Integration/conformance fixture checks, benchmarks, docs and limitations matrix.

MVP non-goals: claiming full XBRL spec conformance, financial advice, browser UI, EDGAR filing certification, SEC/ESMA/DQC profile completeness, AI-generated validation verdicts. AI skills/tools initially documented usage and machine-readable CLI/JSON schema; separate optional agent integration after the deterministic API stabilizes. No remote downloads by default.

## Acceptance
`cargo test --workspace`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo fmt --all -- --check`; fixture CLI parse/validate/render produces expected JSON and escaped HTML; malicious XML/ZIP/resolver fixtures fail within configured limits; benchmark baseline recorded with reproducible fixture provenance. Every feature records source spec version, relevant official conformance test set, pass/fail/unsupported counts. No fabricated full-coverage claim.

## MVP card sequence
| # | Title | Depends on | Assignee/route | Acceptance |
|---|---|---|---|---|
| 1 | Scaffold: toolchain, lint, test harness, CI, README | — | coder-claude / smart-v2 | Workspace builds; CI and commands run |
| 2 | Secure resolver + streaming XML foundations | 1 | coder-claude / smart-frontier-v2 | Namespace/URI fixtures and resource-limit tests |
| 3 | XBRL 2.1 instance parser and typed model | 2 | coder-claude / smart-frontier-v2 | Fact/context/unit roundtrip-ish extraction and negative fixtures |
| 4 | DTS, taxonomy concepts and common linkbases | 3 | coder-claude / smart-frontier-v2 | Local DTS, labels, presentation, calculation fixtures |
| 5 | Core validation and diagnostics | 4 | coder-claude / smart-frontier-v2 | Rule-coded valid/invalid fixture comparison; no false green |
| 6 | Headless common rendering | 5 | coder-claude / smart-v2 | Deterministic escaped text/HTML/CSV golden fixtures |
| 7 | CLI, JSON output, SDK examples and agent tool contract | 6 | coder-claude / smart-v2 | CLI fixture smoke and Rust example run |
| 8 | MVP integration, benchmark baseline, docs and limitations | 7 | coder-claude / smart-frontier-v2 | Full gates, benchmark record and spec coverage matrix |

Each card: read PLAN.md first, work in existing module boundaries; scoped PR, test-first behavior slices, no UI. Sequencing requires code merged into main before next card release, not only task completion. Feature PRs get independent review and Ian approval. Post-MVP consolidation and architecture lock before broad parallel feature waves.

## Later standards coverage (separate milestones, only after MVP)
- Dimensions 1.0 and hypercube validation; taxonomy packages 1.0 and catalogs; typed dimensions.
- Inline XBRL 1.1/IXDS, transformation registries and continuation/exclusion; XHTML security.
- OIM and xBRL-JSON/xBRL-CSV mapping and conversion fidelity; duplicate handling.
- Formula, assertions and filters with safe XPath subset/engine; extensible enumerations; generic and reference linkbases; calculation 1.1, table linkbase, footnotes, rendering nuances.
- SEC EFM/DEI/DQC and ESMA ESEF profiles, versioned independently from core specs; large reference taxonomies.
- Authoring/serialization, query/index APIs, batched processing, differential tests vs Arelle, official conformance suites and performance regression CI.
- Optional MCP/agent skills over deterministic CLI/API with explicit non-authoritative AI boundary.

References: https://www.xbrl.org/the-standard/what/specifications and https://specifications.xbrl.org/ . Use official current specs/test suites as authoritative while implementing each slice.
