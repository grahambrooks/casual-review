use casual_review::engine::run_paths;
use casual_review::parse::{self, Language};
use casual_review::rules::{default_rules, RuleCtx};
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;

const RUST_SAMPLE: &str = include_str!("../src/engine.rs");

fn build_corpus(target_loc: usize) -> (TempDir, Vec<PathBuf>, usize) {
    let tmp = TempDir::new().expect("tempdir");
    let sample_loc = RUST_SAMPLE.lines().count();
    let copies = (target_loc / sample_loc).max(1);
    let mut paths = Vec::with_capacity(copies);
    let mut total_loc = 0usize;

    for i in 0..copies {
        let path = tmp.path().join(format!("file_{i}.rs"));
        std::fs::write(&path, RUST_SAMPLE).expect("write sample");
        paths.push(path);
        total_loc += sample_loc;
    }

    (tmp, paths, total_loc)
}

fn bench_full_pipeline(c: &mut Criterion) {
    let (_tmp, paths, total_loc) = build_corpus(50_000);

    let mut group = c.benchmark_group("full_pipeline");
    group.throughput(Throughput::Elements(total_loc as u64));
    group.measurement_time(Duration::from_secs(8));
    group.bench_function("parse + 3 rules (parallel)", |b| {
        b.iter(|| {
            let out = run_paths(&paths).expect("engine run");
            std::hint::black_box(out);
        });
    });
    group.finish();
}

fn bench_parse_only(c: &mut Criterion) {
    let source = RUST_SAMPLE.repeat(50);
    let loc = source.lines().count();
    let bytes = source.as_bytes();

    let mut group = c.benchmark_group("parse_only");
    group.throughput(Throughput::Elements(loc as u64));
    group.bench_function("tree-sitter rust parse", |b| {
        b.iter(|| {
            let tree = parse::parse(Language::Rust, bytes).expect("parse");
            std::hint::black_box(tree);
        });
    });
    group.finish();
}

fn bench_rules_only(c: &mut Criterion) {
    let source = RUST_SAMPLE.repeat(50);
    let loc = source.lines().count();
    let path = PathBuf::from("bench.rs");
    let tree = parse::parse(Language::Rust, source.as_bytes()).expect("parse");
    let rules = default_rules();

    let mut group = c.benchmark_group("rules_only");
    group.throughput(Throughput::Elements(loc as u64));
    group.bench_function("3 rules over pre-parsed tree", |b| {
        b.iter(|| {
            let ctx = RuleCtx {
                path: &path,
                source: &source,
                tree: Some(&tree),
                language: Some(Language::Rust),
                changed_lines: None,
                old_source: None,
                old_tree: None,
                config: None,
            };
            let mut total = Vec::new();
            for rule in &rules {
                total.extend(rule.run(&ctx));
            }
            std::hint::black_box(total);
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_full_pipeline,
    bench_parse_only,
    bench_rules_only
);
criterion_main!(benches);
