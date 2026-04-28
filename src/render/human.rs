use crate::diagnostic::{Diagnostic, Severity};
use ariadne::{Color, Label, Report, ReportKind, Source};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

pub fn render(
    diagnostics: &[Diagnostic],
    sources: &HashMap<PathBuf, String>,
    out: &mut dyn Write,
) -> std::io::Result<()> {
    for diag in diagnostics {
        render_one(diag, sources, out)?;
    }
    Ok(())
}

fn render_one(
    diag: &Diagnostic,
    sources: &HashMap<PathBuf, String>,
    out: &mut dyn Write,
) -> std::io::Result<()> {
    let kind = match diag.severity {
        Severity::Error => ReportKind::Error,
        Severity::Warning => ReportKind::Warning,
        Severity::Help | Severity::Note => ReportKind::Advice,
    };

    let primary_id = path_id(&diag.primary.file);
    let primary_color = severity_color(diag.severity);

    let mut report = Report::build(kind, primary_id.clone(), diag.primary.byte_range.start)
        .with_code(&diag.code)
        .with_message(&diag.message)
        .with_label(
            Label::new((primary_id.clone(), diag.primary.byte_range.clone()))
                .with_message(&diag.message)
                .with_color(primary_color),
        );

    for label in &diag.labels {
        let id = path_id(&label.span.file);
        report = report.with_label(
            Label::new((id, label.span.byte_range.clone()))
                .with_message(&label.message)
                .with_color(Color::Cyan),
        );
    }

    for note in &diag.notes {
        report = report.with_note(note);
    }
    for help in &diag.helps {
        report = report.with_help(help);
    }

    let report = report.finish();

    let cache = SourceCache::new(sources);
    report.write(cache, &mut *out)?;
    Ok(())
}

fn path_id(p: &Path) -> String {
    p.display().to_string()
}

fn severity_color(severity: Severity) -> Color {
    match severity {
        Severity::Error => Color::Red,
        Severity::Warning => Color::Yellow,
        Severity::Help => Color::Green,
        Severity::Note => Color::Blue,
    }
}

struct SourceCache<'a> {
    sources: &'a HashMap<PathBuf, String>,
    cache: HashMap<String, Source<String>>,
}

impl<'a> SourceCache<'a> {
    fn new(sources: &'a HashMap<PathBuf, String>) -> Self {
        Self {
            sources,
            cache: HashMap::new(),
        }
    }
}

impl<'a> ariadne::Cache<String> for SourceCache<'a> {
    type Storage = String;

    fn fetch(&mut self, id: &String) -> Result<&Source<String>, Box<dyn std::fmt::Debug + '_>> {
        if !self.cache.contains_key(id) {
            let path = PathBuf::from(id);
            let text = match self.sources.get(&path).cloned() {
                Some(t) => t,
                None => {
                    return Err(
                        Box::new(format!("missing source for {id}")) as Box<dyn std::fmt::Debug>
                    );
                }
            };
            self.cache.insert(id.clone(), Source::from(text));
        }
        Ok(self.cache.get(id).expect("inserted above when missing"))
    }

    fn display<'b>(&self, id: &'b String) -> Option<Box<dyn std::fmt::Display + 'b>> {
        Some(Box::new(id.clone()))
    }
}
