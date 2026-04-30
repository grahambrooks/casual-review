pub mod github;
pub mod human;
pub mod json;
pub mod sarif;

use crate::diagnostic::Diagnostic;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Format {
    Human,
    Json,
    Github,
    Sarif,
}

pub fn render(
    format: Format,
    diagnostics: &[Diagnostic],
    sources: &HashMap<PathBuf, String>,
    out: &mut dyn std::io::Write,
) -> std::io::Result<()> {
    match format {
        Format::Human => human::render(diagnostics, sources, out),
        Format::Json => json::render(diagnostics, out),
        Format::Github => github::render(diagnostics, out),
        Format::Sarif => sarif::render(diagnostics, out),
    }
}
