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

    fn explain(&self) -> &'static str {
        "Explicit `any` type annotations in TypeScript source. Catches `x: any`, return-type \
         `any`, generic argument `any`, and `as any` assertions (which use the same predefined \
         `any` token under the hood).\n\n\
         `any` opts out of type checking. Once it's introduced, type errors silently flow \
         through the rest of the program. It's almost always the wrong choice — most codebases \
         that use `any` have lost track of where the boundary of trusted vs untrusted shape \
         data is.\n\n\
         Fix: use `unknown` for unknown shapes (it forces narrowing before use), declare a \
         specific interface for what you actually want, or use a generic `T` if the value's \
         type should propagate. Library boundaries with truly opaque external data are a \
         legitimate case for `unknown` + a parser; reach for `any` only when no other type \
         system escape exists.\n\n\
         See also `ts-escape-hatch`, which catches sibling patterns (`@ts-ignore`, non-null \
         assertion `!`)."
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
