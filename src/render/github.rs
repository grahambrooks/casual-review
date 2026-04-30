use crate::diagnostic::{Diagnostic, Severity};
use std::io::Write;

/// Render diagnostics as GitHub Actions workflow commands.
///
/// Output format:
/// ::notice file=path,line=1,col=5,title=rule-id::message
/// ::warning file=path,line=1,col=5,title=rule-id::message
/// ::error file=path,line=1,col=5,title=rule-id::message
///
/// This format works natively with GitHub Actions and will surface as PR annotations.
pub fn render(diagnostics: &[Diagnostic], out: &mut dyn Write) -> std::io::Result<()> {
    for diag in diagnostics {
        let level = match diag.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "notice",
            Severity::Help => "notice",
        };

        let file = diag
            .primary
            .file
            .to_string_lossy()
            .to_string()
            .replace('\\', "/");

        // GitHub Actions format: ::level file=path,line=L,col=C,title=title::message
        writeln!(
            out,
            "::{} file={},line={},col={},title={}::{}",
            level, file, diag.primary.line_start, diag.primary.col_start, &diag.code, diag.message
        )?;
    }
    Ok(())
}
