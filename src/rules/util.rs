use tree_sitter::{Node, Query, QueryCapture};

/// Look up a capture in a match by its `@name` in the query.
///
/// Tree-sitter's `m.captures` slice is ordered by tree position, not by capture
/// index — indexing it by position has bitten this codebase twice. Always go
/// through this helper instead.
pub fn find_capture<'tree>(
    query: &Query,
    captures: &[QueryCapture<'tree>],
    name: &str,
) -> Option<Node<'tree>> {
    let idx = query.capture_index_for_name(name)?;
    captures.iter().find(|c| c.index == idx).map(|c| c.node)
}

/// First direct child whose kind matches `kind`. Materializes the result
/// before the cursor goes out of scope — returning the iterator directly
/// causes E0597.
pub fn find_child<'tree>(node: &Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    let mut walker = node.walk();
    let result = node.children(&mut walker).find(|c| c.kind() == kind);
    result
}

/// Block contains nothing meaningful — only braces, semicolons, comments, or
/// whitespace. Used by `empty-catch` for Rust/TS/Java.
pub fn is_empty_block(node: &Node<'_>, source: &[u8]) -> bool {
    let mut walker = node.walk();
    for child in node.children(&mut walker) {
        match child.kind() {
            "{" | "}" | ";" | "comment" | "line_comment" | "block_comment" => continue,
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
