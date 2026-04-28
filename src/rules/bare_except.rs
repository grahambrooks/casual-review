use super::{Rule, RuleCtx};
use crate::diagnostic::{Diagnostic, Severity, Span};
use crate::parse::Language;
use tree_sitter::{Query, QueryCursor};

pub struct BareExceptRule;

const PY_QUERY: &str = "(except_clause) @e";

impl Rule for BareExceptRule {
    fn id(&self) -> &'static str {
        "bare-except"
    }

    fn explain(&self) -> &'static str {
        "A Python `except:` clause with no exception type. Catches *everything*, including \
         `KeyboardInterrupt` and `SystemExit` — making the program impossible to ctrl-C and \
         masking errors that should propagate.\n\n\
         Fix: catch a specific type. `except Exception:` is the broad-but-safe choice (excludes \
         `KeyboardInterrupt` and `SystemExit`, includes everything else). If you want \
         truly-everything for a top-level handler, name `BaseException` so the intent is \
         explicit.\n\n\
         For empty bodies (any exception type), see `empty-catch`."
    }

    fn run(&self, ctx: &RuleCtx<'_>) -> Vec<Diagnostic> {
        let (Some(tree), Some(language)) = (ctx.tree, ctx.language) else {
            return Vec::new();
        };
        if !matches!(language, Language::Python) {
            return Vec::new();
        }
        let Ok(query) = Query::new(&language.ts_language(), PY_QUERY) else {
            return Vec::new();
        };

        let mut cursor = QueryCursor::new();
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();

        for m in cursor.matches(&query, tree.root_node(), source_bytes) {
            for cap in m.captures {
                if !is_bare(&cap.node, source_bytes) {
                    continue;
                }
                let header_end = cap
                    .node
                    .child(0)
                    .map(|n| n.end_byte())
                    .unwrap_or(cap.node.start_byte() + 6);
                let span = Span::from_byte_range(
                    ctx.path.to_path_buf(),
                    ctx.source,
                    cap.node.start_byte()..header_end,
                );
                if !ctx.line_in_changes(span.line_start) {
                    continue;
                }
                diagnostics.push(
                    Diagnostic::new(
                        "bare-except",
                        Severity::Warning,
                        "bare `except:` catches everything including SystemExit and KeyboardInterrupt",
                        span,
                    )
                    .with_help("catch a specific exception type, e.g. `except Exception:`"),
                );
            }
        }

        diagnostics
    }
}

fn is_bare(node: &tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut walker = node.walk();
    for child in node.children(&mut walker) {
        match child.kind() {
            "except" | ":" | "block" | "comment" => continue,
            _ => {
                let text = child.utf8_text(source).unwrap_or("");
                if text.trim().is_empty() {
                    continue;
                }
                return false;
            }
        }
    }
    true
}
