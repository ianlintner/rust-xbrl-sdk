# Agent tool contract (CLI, machine-readable)

Status: **implemented as the `xbrl` CLI with `--json`**. It is *not* an MCP
server. An MCP wrapper, if built later, must be a thin layer over exactly these
deterministic operations. No operation uses an AI model to reach a validation
verdict, and none performs network access.

Envelope schema: [`cli-result.schema.json`](cli-result.schema.json), id
`xbrl-sdk/cli-result/v1` (JSON Schema draft 2020-12; JSON per RFC 8259).
Instance semantics follow XBRL 2.1 (REC 2003-12-31, errata 2013-02-20).
Fixture provenance: [`../fixtures/README.md`](../fixtures/README.md).
A breaking change to field names or vocabulary requires a new schema id.

## Operations

| Operation | Command | Purpose |
| --- | --- | --- |
| parse | `xbrl --json parse INSTANCE` | Typed-model counts; parse failures as diagnostics |
| validate | `xbrl --json validate INSTANCE [--dts DIR --entry REF]` | Rule engine in `xbrl-validate` |
| render | `xbrl --json render INSTANCE --dts DIR --entry REF [--format text\|html\|csv]` | Presentation tables from the typed model |

`--json` may appear anywhere. All paths are local; `--dts` is a local directory
and `--entry` a path inside it. Nothing is downloaded.

## Result and exit semantics

Stdout carries exactly one JSON line; stderr is empty in `--json` mode.

| `exit_code` | `status` | Meaning |
| --- | --- | --- |
| 0 | `ok` | Completed with no error-severity diagnostic |
| 1 | `findings` | At least one error-severity diagnostic (parse, DTS or validation) |
| 2 | `error` (`summary.kind = usage`) | Bad command line; `error` holds the message |
| 4 | `error` (`summary.kind = input`) | Instance file unreadable |

Agents must key on `status`/`exit_code` and `diagnostics[*].rule`, never on
prose. `validate` without a DTS returns `xbrl-validate:unsupported` (error), so
a missing validator is never reported as valid. `ok` means "no error findings
from the rules listed in `summary.rules_executed`", not filing validity or
SEC/ESMA conformance.

Each diagnostic: `rule` (stable id, renaming is breaking), `severity`
(`info|warning|error`), `message`, `span` (`{start,end}` half-open byte offsets
or `null`). `diagnostics_truncated` is true when a component dropped findings.

## Captured runs (from `target/debug/xbrl`, fixtures in this repository)

`xbrl --json parse fixtures/instance/basic.xml`

```json
{"schema":"xbrl-sdk/cli-result/v1","command":"parse","status":"ok","exit_code":0,"input":{"path":"fixtures/instance/basic.xml"},"summary":{"schema_refs":1,"contexts":2,"units":2,"facts":4},"diagnostics":[],"diagnostics_truncated":false,"error":null,"output":null}
```

`xbrl --json validate fixtures/instance/basic.xml` (no DTS: fails closed)

```json
{"schema":"xbrl-sdk/cli-result/v1","command":"validate","status":"findings","exit_code":1,"input":{"path":"fixtures/instance/basic.xml"},"summary":{"rules_executed":["structural","references","duplicate-consistency","calculation"],"findings":1,"errors":1,"warnings":0,"info":0,"findings_truncated":false,"dts_entry":null},"diagnostics":[{"rule":"xbrl-validate:unsupported","severity":"error","message":"calculation checks require a DTS; none supplied","span":null}],"diagnostics_truncated":false,"error":null,"output":null}
```

`xbrl --json parse fixtures/instance/invalid-root.xml`

```json
{"schema":"xbrl-sdk/cli-result/v1","command":"parse","status":"findings","exit_code":1,"input":{"path":"fixtures/instance/invalid-root.xml"},"summary":{"kind":"parse"},"diagnostics":[{"rule":"xbrl-instance:invalid-root","severity":"error","message":"expected xbrli:xbrl","span":{"start":39,"end":104}}],"diagnostics_truncated":false,"error":null,"output":null}
```

## Limits and non-goals

Resource limits live in the SDK (`xbrl-xml` `Limits`, DTS limits, validation
limits); the CLI exposes no flag to raise them. Out of scope: AI verdicts, online
services, UI, Arelle/Python runtime, SEC/ESMA certification, inline XBRL, and
dimensional rendering (reported as `[unsupported: dimensional context]`).
The Rust walkthrough is `cargo run -p xbrl-cli --example sdk_walkthrough`.
