use super::{Rule, RuleCtx};
use crate::diagnostic::{Diagnostic, Severity, Span};
use crate::parse::Language;
use tree_sitter::{Query, QueryCursor};

pub struct AnyTypeRule;

const TS_QUERY: &str = "(predefined_type) @t";

impl Rule for AnyTypeRule {
    fn id(&self) -> &'static str {
        "any-type"
    }

    fn run(&self, ctx: &RuleCtx<'_>) -> Vec<Diagnostic> {
        let (Some(tree), Some(language)) = (ctx.tree, ctx.language) else {
            return Vec::new();
        };
        if !matches!(language, Language::TypeScript | Language::Tsx) {
            return Vec::new();
        }
        let Ok(query) = Query::new(&language.ts_language(), TS_QUERY) else {
            return Vec::new();
        };

        let mut cursor = QueryCursor::new();
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();

        for m in cursor.matches(&query, tree.root_node(), source_bytes) {
            for cap in m.captures {
                let text = cap.node.utf8_text(source_bytes).unwrap_or("");
                if text != "any" {
                    continue;
                }
                let span = Span::from_byte_range(
                    ctx.path.to_path_buf(),
                    ctx.source,
                    cap.node.start_byte()..cap.node.end_byte(),
                );
                if !ctx.line_in_changes(span.line_start) {
                    continue;
                }
                diagnostics.push(
                    Diagnostic::new(
                        "any-type",
                        Severity::Warning,
                        "`any` type defeats the type checker",
                        span,
                    )
                    .with_help("use `unknown` for unknown shapes, or a specific type"),
                );
            }
        }

        diagnostics
    }
}
