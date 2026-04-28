use super::util::find_capture;
use super::{Rule, RuleCtx};
use crate::diagnostic::{Diagnostic, Severity, Span};
use crate::parse::Language;
use tree_sitter::{Query, QueryCursor};

pub struct DebugPrintRule;

const RUST_QUERY: &str = "(macro_invocation macro: (identifier) @m)";
const PYTHON_QUERY: &str = "(call function: (identifier) @f)";
const TS_QUERY: &str = r#"
    (call_expression
      function: (member_expression
        object: (identifier) @obj
        property: (property_identifier) @method))
"#;
const JAVA_QUERY: &str = "(method_invocation) @call";

const RUST_NAMES: &[&str] = &["println", "eprintln", "print", "eprint", "dbg"];
const PYTHON_NAMES: &[&str] = &["print", "pprint", "breakpoint"];
const TS_METHODS: &[&str] = &["log", "debug", "info", "warn", "trace"];

impl Rule for DebugPrintRule {
    fn id(&self) -> &'static str {
        "debug-print"
    }

    fn explain(&self) -> &'static str {
        "Calls to language-specific debug-output primitives — `println!`/`eprintln!`/`dbg!` \
         (Rust), `print()`/`pprint()`/`breakpoint()` (Python), \
         `console.log/debug/info/warn/trace` (TS), `System.out.println`/`System.err.println`/\
         `*.printStackTrace()` (Java).\n\n\
         These often start as quick instrumentation during development and slip into the \
         final commit. They pollute production output, sometimes leak data, and indicate \
         the missing piece is real logging.\n\n\
         Fix: remove the call, or replace with a structured logger (`tracing` in Rust, \
         `logging` in Python, a real logger in TS/Java). Tests and main-program output are \
         legitimate exceptions; path-based suppression in a config will eventually let those \
         skip the rule."
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
        let source_bytes = ctx.source.as_bytes();

        for m in cursor.matches(&query, tree.root_node(), source_bytes) {
            let hit = match language {
                Language::Rust => {
                    let Some(name_cap) = m.captures.first() else {
                        continue;
                    };
                    let name = name_cap.node.utf8_text(source_bytes).unwrap_or("");
                    if RUST_NAMES.contains(&name) {
                        Some((name.to_string(), name_cap.node))
                    } else {
                        None
                    }
                }
                Language::Python => {
                    let Some(name_cap) = m.captures.first() else {
                        continue;
                    };
                    let name = name_cap.node.utf8_text(source_bytes).unwrap_or("");
                    if PYTHON_NAMES.contains(&name) {
                        Some((format!("{name}()"), name_cap.node))
                    } else {
                        None
                    }
                }
                Language::TypeScript | Language::Tsx => {
                    let (Some(obj_node), Some(method_node)) = (
                        find_capture(&query, m.captures, "obj"),
                        find_capture(&query, m.captures, "method"),
                    ) else {
                        continue;
                    };
                    let obj = obj_node.utf8_text(source_bytes).unwrap_or("");
                    let method = method_node.utf8_text(source_bytes).unwrap_or("");
                    if obj == "console" && TS_METHODS.contains(&method) {
                        Some((format!("console.{method}"), method_node))
                    } else {
                        None
                    }
                }
                Language::Java => {
                    let Some(call_node) = m.captures.first().map(|c| c.node) else {
                        continue;
                    };
                    let name_node = call_node.child_by_field_name("name");
                    let object_node = call_node.child_by_field_name("object");
                    let method_name = name_node
                        .and_then(|n| n.utf8_text(source_bytes).ok())
                        .unwrap_or("");
                    let object_text = object_node
                        .and_then(|n| n.utf8_text(source_bytes).ok())
                        .unwrap_or("");

                    let label = if method_name == "printStackTrace" {
                        Some(format!("`{object_text}.printStackTrace()`"))
                    } else if (object_text == "System.out" || object_text == "System.err")
                        && (method_name == "println" || method_name == "print")
                    {
                        Some(format!("`{object_text}.{method_name}`"))
                    } else {
                        None
                    };
                    label.map(|l| (l, call_node))
                }
            };

            let Some((label, node)) = hit else { continue };
            let span = Span::from_byte_range(
                ctx.path.to_path_buf(),
                ctx.source,
                node.start_byte()..node.end_byte(),
            );
            if !ctx.line_in_changes(span.line_start) {
                continue;
            }
            diagnostics.push(
                Diagnostic::new(
                    "debug-print",
                    Severity::Warning,
                    format!("debug output: `{label}`"),
                    span,
                )
                .with_help("remove before merging or replace with a real logger"),
            );
        }

        diagnostics
    }
}
