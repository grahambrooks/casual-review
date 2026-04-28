use super::{Rule, RuleCtx};
use crate::diagnostic::{Diagnostic, Severity, Span};
use crate::parse::Language;
use tree_sitter::{Node, Query, QueryCursor};

pub struct DisabledTestRule;

const RUST_QUERY: &str = "(attribute_item) @a";
const PYTHON_QUERY: &str = "(decorator) @d";
const TS_QUERY: &str = r#"
    (call_expression function: (identifier) @id) @call
    (call_expression function: (member_expression
        object: (identifier) @obj
        property: (property_identifier) @prop)) @call
"#;
const JAVA_QUERY: &str = r#"
    (marker_annotation name: (identifier) @name) @anno
    (annotation name: (identifier) @name) @anno
"#;

impl Rule for DisabledTestRule {
    fn id(&self) -> &'static str {
        "disabled-test"
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
                let hit = match language {
                    Language::Rust => match_rust(&cap.node, source_bytes),
                    Language::Python => match_python(&cap.node, source_bytes),
                    Language::TypeScript | Language::Tsx => {
                        match_ts(&query, m.captures, source_bytes)
                    }
                    Language::Java => match_java(&query, m.captures, source_bytes),
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
                    Diagnostic::new("disabled-test", Severity::Warning, label, span)
                        .with_help("re-enable before merging or remove if no longer needed"),
                );
                break;
            }
        }

        diagnostics
    }
}

fn match_rust<'a>(attr: &Node<'a>, source: &[u8]) -> Option<(String, Node<'a>)> {
    let text = attr.utf8_text(source).ok()?.trim();
    if text == "#[ignore]" || text.starts_with("#[ignore =") || text.starts_with("#[ignore(") {
        Some(("`#[ignore]` disables this test".to_string(), *attr))
    } else {
        None
    }
}

fn match_python<'a>(dec: &Node<'a>, source: &[u8]) -> Option<(String, Node<'a>)> {
    let text = dec.utf8_text(source).ok()?.trim();
    let after_at = text.strip_prefix('@').unwrap_or(text);
    let head = after_at.split('(').next().unwrap_or(after_at);
    let head = head.trim();
    let label = match head {
        "pytest.mark.skip" | "skip" => Some("`@pytest.mark.skip` disables this test"),
        "pytest.mark.skipif" => Some("`@pytest.mark.skipif` may disable this test"),
        "unittest.skip" | "unittest.skipIf" | "unittest.skipUnless" => {
            Some("`@unittest.skip*` disables this test")
        }
        _ => None,
    };
    label.map(|l| (l.to_string(), *dec))
}

fn match_ts<'a>(
    query: &Query,
    captures: &'a [tree_sitter::QueryCapture<'a>],
    source: &[u8],
) -> Option<(String, Node<'a>)> {
    let id_idx = query.capture_index_for_name("id");
    let obj_idx = query.capture_index_for_name("obj");
    let prop_idx = query.capture_index_for_name("prop");
    let call_idx = query.capture_index_for_name("call")?;

    let mut id_node = None;
    let mut obj_node = None;
    let mut prop_node = None;
    let mut call_node = None;
    for cap in captures {
        if Some(cap.index) == id_idx {
            id_node = Some(cap.node);
        } else if Some(cap.index) == obj_idx {
            obj_node = Some(cap.node);
        } else if Some(cap.index) == prop_idx {
            prop_node = Some(cap.node);
        } else if cap.index == call_idx {
            call_node = Some(cap.node);
        }
    }
    let call_node = call_node?;

    if let Some(id) = id_node {
        let name = id.utf8_text(source).unwrap_or("");
        match name {
            "xit" | "xdescribe" | "xtest" => {
                return Some((format!("`{name}(...)` is a disabled test"), call_node));
            }
            _ => {}
        }
    }

    if let (Some(obj), Some(prop)) = (obj_node, prop_node) {
        let obj_name = obj.utf8_text(source).unwrap_or("");
        let prop_name = prop.utf8_text(source).unwrap_or("");
        if matches!(obj_name, "it" | "describe" | "test") {
            match prop_name {
                "skip" => {
                    return Some((
                        format!("`{obj_name}.skip(...)` disables this test"),
                        call_node,
                    ));
                }
                "only" => {
                    return Some((
                        format!("`{obj_name}.only(...)` narrows the suite to this test"),
                        call_node,
                    ));
                }
                _ => {}
            }
        }
    }

    None
}

fn match_java<'a>(
    query: &Query,
    captures: &'a [tree_sitter::QueryCapture<'a>],
    source: &[u8],
) -> Option<(String, Node<'a>)> {
    let name_idx = query.capture_index_for_name("name")?;
    let anno_idx = query.capture_index_for_name("anno")?;

    let mut name_node = None;
    let mut anno_node = None;
    for cap in captures {
        if cap.index == name_idx {
            name_node = Some(cap.node);
        } else if cap.index == anno_idx {
            anno_node = Some(cap.node);
        }
    }
    let (name_node, anno_node) = (name_node?, anno_node?);
    let name = name_node.utf8_text(source).ok()?;
    match name {
        "Disabled" => Some(("`@Disabled` disables this test".to_string(), anno_node)),
        "Ignore" => Some(("`@Ignore` disables this test".to_string(), anno_node)),
        _ => None,
    }
}
