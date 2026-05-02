//! Throughput benchmarks for casual-review's parse + rules pipeline.
//!
//! ## Bench groups
//! - `parse_only/<lang>`        — tree-sitter parse alone, per language.
//! - `rules_only/all`           — every default rule in series over a pre-parsed Rust tree.
//! - `rules_only/per_rule/<id>` — each default rule isolated, so the slow rules are visible.
//! - `full_pipeline/<scale>`    — `engine::run_paths` over corpora at three scales.
//!
//! ## Comparing runs across changes
//! Criterion auto-compares the current run against the immediately previous run.
//! For named baselines (e.g. before/after a refactor):
//!
//! ```sh
//! cargo bench --bench throughput -- --save-baseline before
//! # ...edit code...
//! cargo bench --bench throughput -- --baseline before
//! ```
//!
//! ## Reading the throughput numbers
//! `Throughput::Elements(loc)` makes criterion report `elem/s`, which we
//! interpret as **LOC/sec**. The targets in PLAN.md §8 are:
//!   - parse_only:    ≥ 250k LOC/sec single-thread
//!   - full_pipeline: ≥ 1.5M LOC/sec on 8 cores (current ~150k — known gap, see PLAN.md §12 N2)

use casual_review::engine::run_paths;
use casual_review::parse::{self, Language};
use casual_review::rules::{default_rules, RuleCtx};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;

// Per-language source samples. Rust is the in-tree engine.rs; the others
// pull from fixtures so we exercise real grammar shapes, not synthetic
// minimal programs that would parse in microseconds.
const RUST_SAMPLE: &str = include_str!("../src/engine.rs");
const PYTHON_SAMPLE: &str = include_str!("../fixtures/eval_python.py");
const TS_SAMPLE: &str = include_str!("../fixtures/eval_typescript.ts");
const JAVA_SAMPLE: &str = include_str!("../fixtures/eval_java.java");

/// Repeat `sample` enough times to reach `target_loc`, write to `.<ext>` files
/// in a tempdir, and return the dir + paths + actual LOC. The dir's lifetime
/// must outlive the bench iterations or paths point at deleted files.
fn build_corpus(sample: &str, ext: &str, target_loc: usize) -> (TempDir, Vec<PathBuf>, usize) {
    let tmp = TempDir::new().expect("tempdir");
    let sample_loc = sample.lines().count().max(1);
    let copies = (target_loc / sample_loc).max(1);
    let mut paths = Vec::with_capacity(copies);
    let mut total_loc = 0usize;

    for i in 0..copies {
        let path = tmp.path().join(format!("file_{i}.{ext}"));
        std::fs::write(&path, sample).expect("write sample");
        paths.push(path);
        total_loc += sample_loc;
    }

    (tmp, paths, total_loc)
}

/// Repeat `sample` to roughly `target_loc` lines as a single string.
/// Used by parse_only / rules_only benches that don't need disk files.
fn replicate(sample: &str, target_loc: usize) -> (String, usize) {
    let sample_loc = sample.lines().count().max(1);
    let copies = (target_loc / sample_loc).max(1);
    let s = sample.repeat(copies);
    let loc = s.lines().count();
    (s, loc)
}

fn bench_parse_only(c: &mut Criterion) {
    let cases: &[(&str, Language, &str)] = &[
        ("rust", Language::Rust, RUST_SAMPLE),
        ("python", Language::Python, PYTHON_SAMPLE),
        ("typescript", Language::TypeScript, TS_SAMPLE),
        ("java", Language::Java, JAVA_SAMPLE),
    ];

    let mut group = c.benchmark_group("parse_only");
    for (name, lang, sample) in cases {
        let (source, loc) = replicate(sample, 10_000);
        let bytes = source.as_bytes();
        group.throughput(Throughput::Elements(loc as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), &(*lang, bytes), |b, (lang, bytes)| {
            b.iter(|| {
                let tree = parse::parse(*lang, bytes).expect("parse");
                std::hint::black_box(tree);
            });
        });
    }
    group.finish();
}

fn bench_rules_all(c: &mut Criterion) {
    // Rules are exercised against a pre-parsed Rust tree representative of
    // the codebase the developer is most likely to lint. ~10k LOC keeps each
    // iteration in the millisecond range so criterion can collect 100 samples.
    let (source, loc) = replicate(RUST_SAMPLE, 10_000);
    let path = PathBuf::from("bench.rs");
    let tree = parse::parse(Language::Rust, source.as_bytes()).expect("parse");
    let rules = default_rules();
    let rule_count = rules.len();

    let mut group = c.benchmark_group("rules_only");
    group.throughput(Throughput::Elements(loc as u64));
    let label = format!("all default rules ({rule_count})");
    group.bench_function(label, |b| {
        b.iter(|| {
            let ctx = make_ctx(&path, &source, &tree);
            let mut total = Vec::new();
            for rule in &rules {
                total.extend(rule.run(&ctx));
            }
            std::hint::black_box(total);
        });
    });
    group.finish();
}

fn bench_rules_per_rule(c: &mut Criterion) {
    // Smaller corpus per-rule — we run N rules sequentially, each up to 100
    // samples; without trimming the corpus this would push the suite past
    // ten minutes on slower laptops.
    let (source, loc) = replicate(RUST_SAMPLE, 2_000);
    let path = PathBuf::from("bench.rs");
    let tree = parse::parse(Language::Rust, source.as_bytes()).expect("parse");
    let rules = default_rules();

    let mut group = c.benchmark_group("rules_only/per_rule");
    group.throughput(Throughput::Elements(loc as u64));
    group.measurement_time(Duration::from_secs(3));
    group.sample_size(50);

    for rule in &rules {
        let id = rule.id();
        group.bench_with_input(BenchmarkId::from_parameter(id), &(rule.as_ref()), |b, rule| {
            b.iter(|| {
                let ctx = make_ctx(&path, &source, &tree);
                let diagnostics = rule.run(&ctx);
                std::hint::black_box(diagnostics);
            });
        });
    }
    group.finish();
}

fn bench_full_pipeline(c: &mut Criterion) {
    // Three scales surface scaling behaviour. small ~ daily-edit set,
    // medium ~ moderate repo, large ~ stress test.
    let scales: &[(&str, usize)] = &[
        ("small_5k", 5_000),
        ("medium_50k", 50_000),
        ("large_200k", 200_000),
    ];

    let mut group = c.benchmark_group("full_pipeline");
    group.measurement_time(Duration::from_secs(8));
    group.sample_size(30); // large_200k otherwise needs ~10 minutes

    for (name, target_loc) in scales {
        let (_tmp, paths, total_loc) = build_corpus(RUST_SAMPLE, "rs", *target_loc);
        group.throughput(Throughput::Elements(total_loc as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), &paths, |b, paths| {
            b.iter(|| {
                let out = run_paths(paths).expect("engine run");
                std::hint::black_box(out);
            });
        });
        // _tmp dropped here, after the bench finishes for this scale.
        drop(_tmp);
    }
    group.finish();
}

fn make_ctx<'a>(path: &'a PathBuf, source: &'a str, tree: &'a tree_sitter::Tree) -> RuleCtx<'a> {
    RuleCtx {
        path,
        source,
        tree: Some(tree),
        language: Some(Language::Rust),
        changed_lines: None,
        old_source: None,
        old_tree: None,
        config: None,
    }
}

criterion_group!(
    benches,
    bench_parse_only,
    bench_rules_all,
    bench_rules_per_rule,
    bench_full_pipeline,
);
criterion_main!(benches);
