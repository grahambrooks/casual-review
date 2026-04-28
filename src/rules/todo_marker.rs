use super::{Rule, RuleCtx};
use crate::diagnostic::{Diagnostic, Severity, Span};
use crate::parse::Language;
use tree_sitter::{Query, QueryCursor};

pub struct TodoMarkerRule;

const RUST_QUERY: &str = "(line_comment) @c (block_comment) @c";
const JAVA_QUERY: &str = "(line_comment) @c (block_comment) @c";
const COMMENT_ONLY_QUERY: &str = "(comment) @c";

impl Rule for TodoMarkerRule {
    fn id(&self) -> &'static str {
        "todo-marker"
    }

    fn run(&self, ctx: &RuleCtx<'_>) -> Vec<Diagnostic> {
        let (Some(tree), Some(language)) = (ctx.tree, ctx.language) else {
            return Vec::new();
        };
        let query_src = match language {
            Language::Rust => RUST_QUERY,
            Language::Java => JAVA_QUERY,
            Language::Python | Language::TypeScript | Language::Tsx => COMMENT_ONLY_QUERY,
        };
        let ts_lang = language.ts_language();
        let Ok(query) = Query::new(&ts_lang, query_src) else {
            return Vec::new();
        };

        let mut cursor = QueryCursor::new();
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();

        for m in cursor.matches(&query, tree.root_node(), source_bytes) {
            for cap in m.captures {
                let text = cap.node.utf8_text(source_bytes).unwrap_or("");
                if let Some((marker, offset_in_comment)) = find_marker(text) {
                    let abs_start = cap.node.start_byte() + offset_in_comment;
                    let abs_end = abs_start + marker.len();
                    let span = Span::from_byte_range(
                        ctx.path.to_path_buf(),
                        ctx.source,
                        abs_start..abs_end,
                    );
                    if !ctx.line_in_changes(span.line_start) {
                        continue;
                    }
                    diagnostics.push(
                        Diagnostic::new(
                            "todo-marker",
                            Severity::Note,
                            format!("`{marker}` marker"),
                            span,
                        )
                        .with_help("address before merging or convert to a tracked issue"),
                    );
                }
            }
        }

        diagnostics
    }
}

fn find_marker(comment: &str) -> Option<(&'static str, usize)> {
    const MARKERS: &[&str] = &["TODO", "FIXME", "XXX"];
    for marker in MARKERS {
        if let Some(idx) = comment.find(marker) {
            let after = comment.as_bytes().get(idx + marker.len()).copied();
            let before = if idx == 0 {
                None
            } else {
                comment.as_bytes().get(idx - 1).copied()
            };
            let word_boundary_before =
                before.map_or(true, |b| !b.is_ascii_alphanumeric() && b != b'_');
            let word_boundary_after =
                after.map_or(true, |b| !b.is_ascii_alphanumeric() && b != b'_');
            if word_boundary_before && word_boundary_after {
                return Some((marker, idx));
            }
        }
    }
    None
}
