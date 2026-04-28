use super::util::find_capture;
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

    fn explain(&self) -> &'static str {
        "Calls to `.unwrap()` or `.expect()` on `Result` or `Option` in Rust source code.\n\n\
         Both panic on the unhappy path. In production code, panics are usually the wrong \
         response to an error: they kill the process, lose state, and produce a crash report \
         where a structured error would have been more useful.\n\n\
         Fix: propagate with the `?` operator, return a `Result`, or `match`/`if let` to \
         handle both arms. `.expect(\"reason\")` with a clear panic message is acceptable \
         when the unhappy path is genuinely impossible (post-`is_some()` check, compile-time \
         constants like regex literals, etc.) — make the why non-obvious. Test code routinely \
         uses `.unwrap()`; that's fine and path-based suppression will eventually allow it."
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

        let mut cursor = QueryCursor::new();
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();

        for m in cursor.matches(&query, tree.root_node(), source_bytes) {
            let (Some(method_node), Some(call_node)) = (
                find_capture(&query, m.captures, "method"),
                find_capture(&query, m.captures, "call"),
            ) else {
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
