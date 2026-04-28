use super::{Rule, RuleCtx};
use crate::diagnostic::{Diagnostic, Severity, Span};
use crate::parse::Language;
use tree_sitter::{Query, QueryCursor};

pub struct LargeFunctionRule;

const MAX_BODY_LINES: u32 = 40;

const RUST_QUERY: &str = "(function_item body: (block) @body)";
const PYTHON_QUERY: &str = "(function_definition body: (block) @body)";
const TS_QUERY: &str = r#"
    (function_declaration body: (statement_block) @body)
    (method_definition body: (statement_block) @body)
    (function_expression body: (statement_block) @body)
    (arrow_function body: (statement_block) @body)
"#;
const JAVA_QUERY: &str = r#"
    (method_declaration body: (block) @body)
    (constructor_declaration body: (constructor_body) @body)
"#;

impl Rule for LargeFunctionRule {
    fn id(&self) -> &'static str {
        "large-function"
    }

    fn run(&self, ctx: &RuleCtx<'_>) -> Vec<Diagnostic> {
        let (Some(tree), Some(language)) = (ctx.tree, ctx.language) else {
            return Vec::new();
        };
        let query_src = match language {
            Language::Rust => RUST_QUERY,
            Language::Python => PYTHON_QUERY,
            Language::TypeScript | Language::Tsx => TS_QUERY,
            Language::Java => JAVA_QUERY,
        };
        let ts_lang = language.ts_language();
        let Ok(query) = Query::new(&ts_lang, query_src) else {
            return Vec::new();
        };

        let mut cursor = QueryCursor::new();
        let mut diagnostics = Vec::new();

        for m in cursor.matches(&query, tree.root_node(), ctx.source.as_bytes()) {
            for cap in m.captures {
                let start_pos = cap.node.start_position();
                let end_pos = cap.node.end_position();
                let body_lines = end_pos.row.saturating_sub(start_pos.row) as u32;

                if body_lines <= MAX_BODY_LINES {
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
                        "large-function",
                        Severity::Warning,
                        format!(
                            "function body is {body_lines} lines (threshold: {MAX_BODY_LINES})"
                        ),
                        span,
                    )
                    .with_help(
                        "consider extracting helpers; long functions are harder to test and review",
                    ),
                );
            }
        }

        diagnostics
    }
}
