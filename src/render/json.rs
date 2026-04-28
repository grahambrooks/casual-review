use crate::diagnostic::Diagnostic;
use std::io::Write;

pub fn render(diagnostics: &[Diagnostic], out: &mut dyn Write) -> std::io::Result<()> {
    for diag in diagnostics {
        let line = serde_json::to_string(diag).map_err(std::io::Error::other)?;
        writeln!(out, "{line}")?;
    }
    Ok(())
}
