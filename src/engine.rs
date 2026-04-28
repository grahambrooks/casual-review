use crate::diagnostic::Diagnostic;
use crate::git::{self, ChangedFile, DiffSpec};
use crate::parse::{self, Language};
use crate::rules::{default_rules, Rule, RuleCtx};
use ignore::WalkBuilder;
use rayon::prelude::*;
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

pub struct EngineOutput {
    pub diagnostics: Vec<Diagnostic>,
    pub sources: HashMap<PathBuf, String>,
    pub files_checked: usize,
}

pub fn run_paths(paths: &[PathBuf]) -> anyhow::Result<EngineOutput> {
    let inputs: Vec<FileInput> = paths
        .iter()
        .filter_map(|p| {
            let content = std::fs::read(p).ok()?;
            Some(FileInput {
                path: p.clone(),
                content,
                old_content: None,
                changed_lines: None,
            })
        })
        .collect();
    process(inputs)
}

pub fn run_diff(repo_root: &Path, spec: DiffSpec, all: bool) -> anyhow::Result<EngineOutput> {
    let changed = git::changed_files(repo_root, &spec)?;
    let inputs = changed
        .into_iter()
        .filter_map(|c: ChangedFile| {
            let content = c.new_content?;
            let changed_lines = if all { None } else { Some(c.changed_line_ranges) };
            Some(FileInput {
                path: c.path,
                content,
                old_content: c.old_content,
                changed_lines,
            })
        })
        .collect();
    process(inputs)
}

pub fn run_repo(roots: &[PathBuf]) -> anyhow::Result<EngineOutput> {
    if roots.is_empty() {
        anyhow::bail!("run_repo requires at least one root");
    }

    let mut builder = WalkBuilder::new(&roots[0]);
    for extra in &roots[1..] {
        builder.add(extra);
    }
    builder.hidden(false);

    let mut paths = Vec::new();
    for entry in builder.build() {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        if Language::from_path(path).is_some() {
            paths.push(path.to_path_buf());
        }
    }

    let inputs: Vec<FileInput> = paths
        .into_par_iter()
        .filter_map(|p| {
            let content = std::fs::read(&p).ok()?;
            Some(FileInput {
                path: p,
                content,
                old_content: None,
                changed_lines: None,
            })
        })
        .collect();
    process(inputs)
}

struct FileInput {
    path: PathBuf,
    content: Vec<u8>,
    old_content: Option<Vec<u8>>,
    changed_lines: Option<Vec<Range<u32>>>,
}

fn process(inputs: Vec<FileInput>) -> anyhow::Result<EngineOutput> {
    let rules = default_rules();

    let per_file: Vec<(PathBuf, String, Vec<Diagnostic>)> = inputs
        .into_par_iter()
        .filter_map(|input| {
            let source = String::from_utf8(input.content).ok()?;
            let language = Language::from_path(&input.path);
            let tree = language.and_then(|lang| parse::parse(lang, source.as_bytes()).ok());
            let old_source = input
                .old_content
                .as_ref()
                .and_then(|bytes| std::str::from_utf8(bytes).ok().map(String::from));
            let old_tree = language.and_then(|lang| {
                old_source
                    .as_ref()
                    .and_then(|s| parse::parse(lang, s.as_bytes()).ok())
            });
            let changed_lines_slice = input.changed_lines.as_deref();

            let ctx = RuleCtx {
                path: &input.path,
                source: &source,
                tree: tree.as_ref(),
                language,
                changed_lines: changed_lines_slice,
                old_source: old_source.as_deref(),
                old_tree: old_tree.as_ref(),
            };

            let mut diagnostics = Vec::new();
            for rule in &rules {
                diagnostics.extend(rule.run(&ctx));
            }
            Some((input.path, source, diagnostics))
        })
        .collect();

    let mut all_diagnostics = Vec::new();
    let mut sources = HashMap::with_capacity(per_file.len());
    let files_checked = per_file.len();
    for (path, source, diagnostics) in per_file {
        all_diagnostics.extend(diagnostics);
        sources.insert(path, source);
    }

    sort_diagnostics(&mut all_diagnostics);

    Ok(EngineOutput {
        diagnostics: all_diagnostics,
        sources,
        files_checked,
    })
}

fn sort_diagnostics(diagnostics: &mut [Diagnostic]) {
    diagnostics.sort_by(|a, b| {
        a.primary
            .file
            .cmp(&b.primary.file)
            .then(a.primary.line_start.cmp(&b.primary.line_start))
            .then(a.primary.col_start.cmp(&b.primary.col_start))
            .then(a.code.cmp(&b.code))
    });
}

#[allow(dead_code)]
pub fn rules() -> Vec<Box<dyn Rule>> {
    default_rules()
}
