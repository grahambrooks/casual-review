use super::{Rule, RuleCtx};
use crate::diagnostic::{Diagnostic, Severity, Span};

pub struct TrailingWhitespaceRule;

impl Rule for TrailingWhitespaceRule {
    fn id(&self) -> &'static str {
        "trailing-whitespace"
    }

    fn run(&self, ctx: &RuleCtx<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let bytes = ctx.source.as_bytes();
        let mut line_start = 0usize;
        let mut line_no: u32 = 1;

        for i in 0..=bytes.len() {
            let at_break = i == bytes.len() || bytes[i] == b'\n';
            if !at_break {
                continue;
            }

            let line_end = i;
            let mut trailing_start = line_end;
            while trailing_start > line_start && matches!(bytes[trailing_start - 1], b' ' | b'\t') {
                trailing_start -= 1;
            }

            if trailing_start < line_end && ctx.line_in_changes(line_no) {
                let span = Span::from_byte_range(
                    ctx.path.to_path_buf(),
                    ctx.source,
                    trailing_start..line_end,
                );
                diagnostics.push(Diagnostic::new(
                    "trailing-whitespace",
                    Severity::Warning,
                    "trailing whitespace",
                    span,
                ));
            }

            line_start = i + 1;
            line_no += 1;
        }

        diagnostics
    }
}
