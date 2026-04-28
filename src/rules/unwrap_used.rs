use super::{Rule, RuleCtx};
use crate::diagnostic::{Diagnostic, Severity, Span};
use crate::parse::Language;
use tree_sitter::{Query, QueryCursor};

pub struct UnwrapUsedRule;

const RUST_QUERY: &str = r#"
    (call_expression
      function: (field_expression
        field: (field_identifier) @method)) @call
"#;

impl Rule for UnwrapUsedRule {
    fn id(&self) -> &'static str {
        "unwrap-used"
    }

    fn run(&self, ctx: &RuleCtx<'_>) -> Vec<Diagnostic> {
        let (Some(tree), Some(language)) = (ctx.tree, ctx.language) else {
            return Vec::new();
        };
        if !matches!(language, Language::Rust) {
            return Vec::new();
        }
        let Ok(query) = Query::new(&language.ts_language(), RUST_QUERY) else {
            return Vec::new();
        };

        let method_idx = query
            .capture_index_for_name("method")
            .expect("query has @method capture");
        let call_idx = query
            .capture_index_for_name("call")
            .expect("query has @call capture");

        let mut cursor = QueryCursor::new();
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();

        for m in cursor.matches(&query, tree.root_node(), source_bytes) {
            let mut method_node = None;
            let mut call_node = None;
            for cap in m.captures {
                if cap.index == method_idx {
                    method_node = Some(cap.node);
                } else if cap.index == call_idx {
                    call_node = Some(cap.node);
                }
            }
            let (Some(method_node), Some(call_node)) = (method_node, call_node) else {
                continue;
            };
            let method = method_node.utf8_text(source_bytes).unwrap_or("");
            if method != "unwrap" && method != "expect" {
                continue;
            }
            let span = Span::from_byte_range(
                ctx.path.to_path_buf(),
                ctx.source,
                call_node.start_byte()..call_node.end_byte(),
            );
            if !ctx.line_in_changes(span.line_start) {
                continue;
            }
            diagnostics.push(
                Diagnostic::new(
                    "unwrap-used",
                    Severity::Warning,
                    format!("`.{method}()` panics on error"),
                    span,
                )
                .with_help("propagate the error with `?` or handle it explicitly"),
            );
        }

        diagnostics
    }
}
