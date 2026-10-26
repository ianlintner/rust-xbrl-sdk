//! Baseline micro-benchmarks for qualified-name handling.
//!
//! This is a *harness* baseline, not a performance claim: it measures the two
//! hot operations later milestones depend on (parsing Clark notation and
//! formatting it back) over the shared lexical fixture
//! `fixtures/qname/lexical.tsv`. Record numbers with `cargo bench -p xbrl-core`
//! and cite the fixture provenance (see `fixtures/README.md`) when reporting them.
//!
//! `missing_docs` is allowed here because a benchmark harness is an internal test
//! target: the `criterion_group!`/`criterion_main!` expansions generate public
//! items that are not part of any published API surface.

#![allow(missing_docs)]

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use xbrl_core::qname::QName;

const FIXTURE: &str = include_str!("../../../fixtures/qname/lexical.tsv");

/// Valid Clark-notation inputs from the shared fixture file.
fn valid_inputs() -> Vec<&'static str> {
    FIXTURE
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split('\t').nth(1))
        .filter(|input| input.starts_with('{') && input.contains('}'))
        .collect()
}

fn bench_parse(c: &mut Criterion) {
    let inputs = valid_inputs();
    let mut group = c.benchmark_group("qname_parse_clark");
    group.throughput(Throughput::Elements(inputs.len() as u64));
    group.bench_function("fixture_set", |b| {
        b.iter(|| {
            let mut parsed = 0_u64;
            for input in &inputs {
                if QName::parse_clark(black_box(input)).is_ok() {
                    parsed += 1;
                }
            }
            parsed
        });
    });
    group.finish();
}

fn bench_to_clark(c: &mut Criterion) {
    let names: Vec<QName> = valid_inputs()
        .into_iter()
        .filter_map(|input| QName::parse_clark(input).ok())
        .collect();
    let mut group = c.benchmark_group("qname_to_clark");
    group.throughput(Throughput::Elements(names.len() as u64));
    group.bench_function("fixture_set", |b| {
        b.iter(|| {
            let mut bytes = 0_usize;
            for name in &names {
                bytes += black_box(name).to_clark().len();
            }
            bytes
        });
    });
    group.finish();
}

fn bench_reject(c: &mut Criterion) {
    let mut group = c.benchmark_group("qname_reject");
    group.bench_function("malformed_clark", |b| {
        b.iter(|| QName::parse_clark(black_box("http://www.xbrl.org/2003/instance}context")));
    });
    group.finish();
}

fn benchmarks(c: &mut Criterion) {
    bench_parse(c);
    bench_to_clark(c);
    bench_reject(c);
}

criterion_group!(benches, benchmarks);
criterion_main!(benches);
