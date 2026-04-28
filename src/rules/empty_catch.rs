use super::util::{find_child, is_empty_block};
use super::{Rule, RuleCtx};
use crate::diagnostic::{Diagnostic, Severity, Span};
use crate::parse::Language;
use tree_sitter::{Node, Query, QueryCursor};

pub struct EmptyCatchRule;

const RUST_QUERY: &str = "(match_arm) @arm";
const PYTHON_QUERY: &str = "(except_clause) @clause";
const TS_QUERY: &str = "(catch_clause) @clause";
const JAVA_QUERY: &str = "(catch_clause) @clause";

impl Rule for EmptyCatchRule {
    fn id(&self) -> &'static str {
        "empty-catch"
    }

    fn explain(&self) -> &'static str {
        "An error handler with an empty body — `Err(_) => {}` (Rust), `except: pass` or \
         `except E: pass` (Python), `catch (e) {}` (TS), `catch (Exception e) {}` (Java).\n\n\
         Empty handlers silently swallow errors. The first time a real failure flows through, \
         it disappears with no trace. This is one of the highest-signal review smells across \
         every language.\n\n\
         Fix: handle the error meaningfully, log it, re-throw, or — if you genuinely intend \
         to ignore — leave a comment explaining why so the next reader doesn't have to guess. \
         For Python `except: pass` specifically, also see `bare-except`."
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
        let Ok(query) = Query::new(&language.ts_language(), query_src) else {
            return Vec::new();
        };

        let mut cursor = QueryCursor::new();
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();

        for m in cursor.matches(&query, tree.root_node(), source_bytes) {
            for cap in m.captures {
                let report = match language {
                    Language::Rust => empty_rust_err_arm(&cap.node, source_bytes),
                    Language::Python => empty_python_except(&cap.node, source_bytes),
                    Language::TypeScript | Language::Tsx => empty_ts_catch(&cap.node, source_bytes),
                    Language::Java => empty_java_catch(&cap.node, source_bytes),
                };
                let Some(label) = report else { continue };

                let span = Span::from_byte_range(
                    ctx.path.to_path_buf(),
                    ctx.source,
                    cap.node.start_byte()..cap.node.end_byte(),
                );
                if !ctx.line_in_changes(span.line_start) {
                    continue;
                }
                diagnostics.push(
                    Diagnostic::new("empty-catch", Severity::Warning, label, span).with_help(
                        "either handle the error, log it, or comment why it's safe to ignore",
                    ),
                );
            }
        }

        diagnostics
    }
}

fn empty_rust_err_arm(arm: &Node<'_>, source: &[u8]) -> Option<String> {
    let pattern = arm.child_by_field_name("pattern")?;
    let pat_text = pattern.utf8_text(source).ok()?.trim();
    if !(pat_text.starts_with("Err(") || pat_text == "Err(_)" || pat_text.starts_with("Err ")) {
        return None;
    }
    let value = arm.child_by_field_name("value")?;
    if !is_empty_block(&value, source) {
        return None;
    }
    Some("empty `Err` arm swallows the error".to_string())
}

fn empty_python_except(clause: &Node<'_>, source: &[u8]) -> Option<String> {
    let body = clause
        .child_by_field_name("body")
        .or_else(|| find_child(clause, "block"))?;
    if !python_block_is_pass_only(&body, source) {
        return None;
    }
    Some("empty `except` body silently swallows the exception".to_string())
}

fn empty_ts_catch(clause: &Node<'_>, source: &[u8]) -> Option<String> {
    let body = clause
        .child_by_field_name("body")
        .or_else(|| find_child(clause, "statement_block"))?;
    if !is_empty_block(&body, source) {
        return None;
    }
    Some("empty `catch` block silently swallows the error".to_string())
}

fn empty_java_catch(clause: &Node<'_>, source: &[u8]) -> Option<String> {
    let body = clause
        .child_by_field_name("body")
        .or_else(|| find_child(clause, "block"))?;
    if !is_empty_block(&body, source) {
        return None;
    }
    Some("empty `catch` block silently swallows the exception".to_string())
}

fn python_block_is_pass_only(block: &Node<'_>, source: &[u8]) -> bool {
    let mut walker = block.walk();
    let mut saw_meaningful = false;
    let mut only_pass = true;
    for child in block.children(&mut walker) {
        match child.kind() {
            "comment" => continue,
            "pass_statement" => {
                saw_meaningful = true;
                continue;
            }
            _ => {
                let text = child.utf8_text(source).unwrap_or("");
                if text.trim().is_empty() {
                    continue;
                }
                only_pass = false;
                saw_meaningful = true;
            }
        }
    }
    saw_meaningful && only_pass
}
