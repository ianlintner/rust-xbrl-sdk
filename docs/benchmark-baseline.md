# Benchmark baseline (MVP)

This records the first reproducible QName micro-benchmark baseline. CI only
builds the criterion harness (`cargo bench -p xbrl-core --no-run`); running it is
a local, developer-driven act. These numbers are a regression tripwire for the
authored lexical fixture set, **not** a performance guarantee or a conformance
result.

## Exact command

From the repository root:

```sh
cargo bench --locked -p xbrl-core --bench qname -- \
  --warm-up-time 2 --measurement-time 5 --sample-size 100 --save-baseline mvp
```

Criterion 0.5.1, 100 samples per benchmark, 2 s warm-up, 5 s measurement.
`--save-baseline mvp` stores an unverified local reference under
`target/criterion`; it is not committed. Re-run the same command with
`--load-baseline mvp` to compare a later tree.

## Provenance

| Item | Value |
| --- | --- |
| Measured (UTC) | 2026-11-27T13:35Z (log finished 13:35:54Z) |
| Base commit | `c1a53ee0477a20125896374b6dad9fa136a07c75` (merged PR #8) |
| Local tree | same commit plus the MVP integration test and documentation edits; **no `xbrl-core` source or bench code changed** |
| Toolchain | rustc 1.98.1 (48a229cea 2026-09-01), cargo 1.98.1 (797e8a9bc 2026-08-05), host `x86_64-unknown-linux-gnu` |
| OS / kernel | Omarchy, Linux 7.2.15-1-omarchy |
| CPU | AMD Ryzen 7 255 w/ Radeon 780M Graphics |
| Input fixture | `fixtures/qname/lexical.tsv` (14 authored cases), SHA-256 `71169b48fc676d9ad73352d457169d804bb3f2cd7e9131ad1adf118df6392076` |
| Fixture provenance | `authored`, see [fixtures/README.md](../fixtures/README.md) |

## Measured estimates

Criterion reports a lower bound, point estimate and upper bound (95 % CI).

| Benchmark | Lower | Estimate | Upper | Throughput (estimate) |
| --- | ---: | ---: | ---: | ---: |
| `qname_parse_clark/fixture_set` | 602.36 ns | 606.29 ns | 610.12 ns | 19.793 Melem/s |
| `qname_to_clark/fixture_set` | 480.93 ns | 484.24 ns | 487.68 ns | 12.391 Melem/s |
| `qname_reject/malformed_clark` | 9.9500 ns | 9.9983 ns | 10.049 ns | — |

Outliers: 15 % (`parse`), 4 % (`to_clark`), 6 % (`reject`) of 100 samples.
Noise is machine-dependent; treat deviations well outside these intervals as a
prompt to re-run with identical settings before investigating a regression.

## What is and is not measured

* Measured: Clark-notation parsing and serialization over the 14 authored
  lexical cases, and rejection of one malformed Clark name.
* Not measured: whole-instance parse/validate/render latency, XML reader
  throughput, DTS discovery cost, memory use, or any filing-scale workload.
* No official XBRL conformance suite was executed. See
  [support-matrix.md](support-matrix.md) for the supported/unsupported scope.
