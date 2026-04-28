use super::{Rule, RuleCtx};
use crate::diagnostic::{Diagnostic, Severity, Span};

pub struct ParseErrorRule;

impl Rule for ParseErrorRule {
    fn id(&self) -> &'static str {
        "parse-error"
    }

    fn explain(&self) -> &'static str {
        "The tree-sitter grammar could not parse a region of the file. \
         This is severity Error because every other lint depends on a parseable tree.\n\n\
         Common causes: incomplete code mid-edit, an unclosed brace or paren, a syntax error \
         the language compiler would also reject. Fix by completing the syntax. \
         If the parser disagrees with code that you believe is valid, the grammar may need \
         updating — open an issue."
    }

    fn run(&self, ctx: &RuleCtx<'_>) -> Vec<Diagnostic> {
        let Some(tree) = ctx.tree else {
            return Vec::new();
        };
        let mut diagnostics = Vec::new();
        let mut cursor = tree.walk();
        let root = tree.root_node();
        visit(&root, &mut cursor, ctx, &mut diagnostics);
        diagnostics
    }
}

fn visit(
    node: &tree_sitter::Node<'_>,
    cursor: &mut tree_sitter::TreeCursor<'_>,
    ctx: &RuleCtx<'_>,
    out: &mut Vec<Diagnostic>,
) {
    if node.is_error() || node.is_missing() {
        let span = Span::from_byte_range(
            ctx.path.to_path_buf(),
            ctx.source,
            node.start_byte()..node.end_byte().max(node.start_byte() + 1),
        );
        if ctx.line_in_changes(span.line_start) {
            let message = if node.is_missing() {
                format!("missing `{}`", node.kind())
            } else {
                "syntax error".to_string()
            };
            out.push(
                Diagnostic::new("parse-error", Severity::Error, message, span)
                    .with_note("the tree-sitter grammar could not parse this region"),
            );
        }
        return;
    }

    if node.child_count() > 0 && cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            visit(&child, &mut child.walk(), ctx, out);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}
