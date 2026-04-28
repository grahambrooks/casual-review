use super::{Rule, RuleCtx};
use crate::diagnostic::{Diagnostic, Severity, Span};
use crate::parse::Language;
use tree_sitter::{Query, QueryCursor};

// ts-escape-hatch needs to dispatch on capture index since the query has two
// alternative patterns (`@c` for comment directives, `@nn` for non-null
// assertions). The capture-name lookup is done inline rather than via
// find_capture because each match yields exactly one capture and we want to
// branch on which one it is.

pub struct TsEscapeHatchRule;

const TS_QUERY: &str = r#"
    (comment) @c
    (non_null_expression) @nn
"#;

const COMMENT_DIRECTIVES: &[(&str, &str)] = &[
    ("@ts-ignore", "`@ts-ignore` suppresses the next-line type error"),
    (
        "@ts-nocheck",
        "`@ts-nocheck` disables type checking for the whole file",
    ),
    (
        "@ts-expect-error",
        "`@ts-expect-error` suppresses an expected type error",
    ),
];

impl Rule for TsEscapeHatchRule {
    fn id(&self) -> &'static str {
        "ts-escape-hatch"
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

        let comment_idx = query.capture_index_for_name("c");
        let nn_idx = query.capture_index_for_name("nn");

        let mut cursor = QueryCursor::new();
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();

        for m in cursor.matches(&query, tree.root_node(), source_bytes) {
            for cap in m.captures {
                if Some(cap.index) == comment_idx {
                    let text = cap.node.utf8_text(source_bytes).unwrap_or("");
                    for (needle, msg) in COMMENT_DIRECTIVES {
                        if text.contains(needle) {
                            let span = Span::from_byte_range(
                                ctx.path.to_path_buf(),
                                ctx.source,
                                cap.node.start_byte()..cap.node.end_byte(),
                            );
                            if !ctx.line_in_changes(span.line_start) {
                                break;
                            }
                            diagnostics.push(
                                Diagnostic::new(
                                    "ts-escape-hatch",
                                    Severity::Warning,
                                    msg.to_string(),
                                    span,
                                )
                                .with_help("address the underlying type error or narrow the assertion"),
                            );
                            break;
                        }
                    }
                } else if Some(cap.index) == nn_idx {
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
                            "ts-escape-hatch",
                            Severity::Warning,
                            "non-null assertion (`!`) bypasses the null check",
                            span,
                        )
                        .with_help("guard with an explicit check, or refine the type"),
                    );
                }
            }
        }

        diagnostics
    }
}
