# Rendering and agent tool contract

## Common headless views
- Fact explorer: filter by concept QName, taxonomy, entity, period, dimensions and unit; show source location and exact numeric lexical value.
- Presentation tree: use taxonomy presentation linkbase roles/labels and arc ordering; preserve language/role selection. Show missing/duplicate fact states instead of manufacturing cells.
- Calculation/validation view: list relationship, inputs, rule ID, accuracy metadata, observed result and source locations. Only report a calculation as checked when the selected rule engine actually ran.
- Export: escaped HTML, deterministic text, and CSV with correct quoting and spreadsheet-formula injection precautions. Rendering never implies filing validity.

## Agent tool boundary (design contract, not deployed MCP)
A future MCP server should be a thin Rust wrapper over deterministic SDK operations, not an LLM validator. Proposed read-only operations: `inspect_report`, `list_taxonomy_entrypoints`, `list_facts`, `get_fact`, `validate_report`, `render_presentation`, `get_diagnostics`, `get_supported_rules`. Inputs should be local report/package handles (not arbitrary network URLs), bounded pagination and explicit taxonomy/resolver policy. Outputs should carry support status (`supported`, `unsupported`, `unknown`), diagnostics with rule/clause IDs, provenance and truncation counts. Never offer a `valid: true` result if a required validator/profile is unavailable.

A file-creation/authoring tool should be separate, opt-in and reviewable; never overwrite a filing or publish one as a side effect of read-only inspection. Do not expose raw filesystem paths outside a configured workspace. Put all network, resource and size limits in Rust SDK/resolver so MCP callers cannot bypass them.

## Before using a tool
Inspect `README.md`, the implemented CLI `--help`, and code/tests for the operation and output schema. This skill does not claim the proposed MCP methods or CLI flags exist today. Do not invent a JSON result; capture a real run and cite the exact input/artifact.
