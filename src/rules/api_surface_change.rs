use super::{Rule, RuleCtx};
use crate::diagnostic::{Diagnostic, Severity, Span};
use crate::parse::Language;
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

pub struct ApiSurfaceChangeRule;

#[derive(Debug, Clone)]
struct Symbol {
    kind: &'static str,
    name: String,
    byte_range: std::ops::Range<usize>,
}

impl Rule for ApiSurfaceChangeRule {
    fn id(&self) -> &'static str {
        "api-surface-change"
    }

    fn run(&self, ctx: &RuleCtx<'_>) -> Vec<Diagnostic> {
        let (Some(new_tree), Some(language)) = (ctx.tree, ctx.language) else {
            return Vec::new();
        };
        let (Some(old_tree), Some(old_source)) = (ctx.old_tree, ctx.old_source) else {
            return Vec::new();
        };

        let new_symbols = extract(language, new_tree, ctx.source.as_bytes());
        let old_symbols = extract(language, old_tree, old_source.as_bytes());

        let new_by_name: HashMap<&str, &Symbol> =
            new_symbols.iter().map(|s| (s.name.as_str(), s)).collect();
        let old_by_name: HashMap<&str, &Symbol> =
            old_symbols.iter().map(|s| (s.name.as_str(), s)).collect();

        let mut diagnostics = Vec::new();

        for sym in &new_symbols {
            if !old_by_name.contains_key(sym.name.as_str()) {
                let span = Span::from_byte_range(
                    ctx.path.to_path_buf(),
                    ctx.source,
                    sym.byte_range.clone(),
                );
                diagnostics.push(
                    Diagnostic::new(
                        "api-surface-change",
                        Severity::Note,
                        format!("public {} `{}` added", sym.kind, sym.name),
                        span,
                    )
                    .with_help("new public surface — confirm naming, docs, and stability"),
                );
            }
        }

        for sym in &old_symbols {
            if !new_by_name.contains_key(sym.name.as_str()) {
                let line_start = top_of_file_line(ctx.source);
                let span = Span {
                    file: ctx.path.to_path_buf(),
                    byte_range: 0..line_start.0 as usize,
                    line_start: 1,
                    col_start: 1,
                    line_end: 1,
                    col_end: line_start.0.max(1),
                };
                diagnostics.push(
                    Diagnostic::new(
                        "api-surface-change",
                        Severity::Note,
                        format!("public {} `{}` removed", sym.kind, sym.name),
                        span,
                    )
                    .with_help("downstream callers may break — check for external usages"),
                );
            }
        }

        diagnostics
    }
}

fn top_of_file_line(source: &str) -> (u32, u32) {
    let first_line = source.lines().next().unwrap_or("");
    (first_line.len() as u32 + 1, 1)
}

fn extract(language: Language, tree: &Tree, source: &[u8]) -> Vec<Symbol> {
    let root = tree.root_node();
    let mut out = Vec::new();
    match language {
        Language::Rust => extract_rust(&root, source, &mut out),
        Language::Python => extract_python(&root, source, &mut out),
        Language::TypeScript | Language::Tsx => extract_ts(&root, source, &mut out),
        Language::Java => extract_java(&root, source, &mut out),
    }
    out
}

fn extract_rust(root: &Node<'_>, source: &[u8], out: &mut Vec<Symbol>) {
    let mut walker = root.walk();
    for child in root.children(&mut walker) {
        if !child_is_pub_rust(&child, source) {
            continue;
        }
        let (kind, name_node) = match child.kind() {
            "function_item" => ("fn", child.child_by_field_name("name")),
            "struct_item" => ("struct", child.child_by_field_name("name")),
            "enum_item" => ("enum", child.child_by_field_name("name")),
            "trait_item" => ("trait", child.child_by_field_name("name")),
            "type_item" => ("type", child.child_by_field_name("name")),
            "const_item" => ("const", child.child_by_field_name("name")),
            "static_item" => ("static", child.child_by_field_name("name")),
            "mod_item" => ("mod", child.child_by_field_name("name")),
            _ => continue,
        };
        let Some(name_node) = name_node else { continue };
        let name = match name_node.utf8_text(source) {
            Ok(s) => s.to_string(),
            Err(_) => continue,
        };
        out.push(Symbol {
            kind,
            name,
            byte_range: child.start_byte()..child.end_byte().min(child.start_byte() + 80),
        });
    }
}

fn child_is_pub_rust(node: &Node<'_>, source: &[u8]) -> bool {
    let mut walker = node.walk();
    for c in node.children(&mut walker) {
        if c.kind() == "visibility_modifier" {
            let text = c.utf8_text(source).unwrap_or("");
            return text.starts_with("pub");
        }
    }
    false
}

fn extract_python(root: &Node<'_>, source: &[u8], out: &mut Vec<Symbol>) {
    let mut walker = root.walk();
    for child in root.children(&mut walker) {
        let (kind, name_node) = match child.kind() {
            "function_definition" => ("def", child.child_by_field_name("name")),
            "class_definition" => ("class", child.child_by_field_name("name")),
            _ => continue,
        };
        let Some(name_node) = name_node else { continue };
        let Ok(name) = name_node.utf8_text(source) else {
            continue;
        };
        if name.starts_with('_') {
            continue;
        }
        out.push(Symbol {
            kind,
            name: name.to_string(),
            byte_range: child.start_byte()..child.end_byte().min(child.start_byte() + 80),
        });
    }
}

fn extract_java(root: &Node<'_>, source: &[u8], out: &mut Vec<Symbol>) {
    let mut walker = root.walk();
    for child in root.children(&mut walker) {
        let kind = match child.kind() {
            "class_declaration" => "class",
            "interface_declaration" => "interface",
            "enum_declaration" => "enum",
            "record_declaration" => "record",
            _ => continue,
        };
        if !java_is_public(&child, source) {
            continue;
        }
        let Some(name_node) = child.child_by_field_name("name") else {
            continue;
        };
        let Ok(name) = name_node.utf8_text(source) else {
            continue;
        };
        out.push(Symbol {
            kind,
            name: name.to_string(),
            byte_range: child.start_byte()..child.end_byte().min(child.start_byte() + 80),
        });
    }
}

fn java_is_public(node: &Node<'_>, source: &[u8]) -> bool {
    let mut walker = node.walk();
    for c in node.children(&mut walker) {
        if c.kind() == "modifiers" {
            let text = c.utf8_text(source).unwrap_or("");
            return text.split_whitespace().any(|w| w == "public");
        }
    }
    false
}

fn extract_ts(root: &Node<'_>, source: &[u8], out: &mut Vec<Symbol>) {
    let mut walker = root.walk();
    for child in root.children(&mut walker) {
        if child.kind() != "export_statement" {
            continue;
        }
        let mut sub = child.walk();
        for inner in child.children(&mut sub) {
            let (kind, name_node) = match inner.kind() {
                "function_declaration" => ("function", inner.child_by_field_name("name")),
                "class_declaration" => ("class", inner.child_by_field_name("name")),
                "interface_declaration" => ("interface", inner.child_by_field_name("name")),
                "type_alias_declaration" => ("type", inner.child_by_field_name("name")),
                "enum_declaration" => ("enum", inner.child_by_field_name("name")),
                "lexical_declaration" | "variable_declaration" => {
                    let mut decl_walker = inner.walk();
                    for decl in inner.children(&mut decl_walker) {
                        if decl.kind() == "variable_declarator" {
                            if let Some(name_node) = decl.child_by_field_name("name") {
                                if let Ok(name) = name_node.utf8_text(source) {
                                    out.push(Symbol {
                                        kind: "const",
                                        name: name.to_string(),
                                        byte_range: inner.start_byte()
                                            ..inner.end_byte().min(inner.start_byte() + 80),
                                    });
                                }
                            }
                        }
                    }
                    continue;
                }
                _ => continue,
            };
            let Some(name_node) = name_node else { continue };
            if let Ok(name) = name_node.utf8_text(source) {
                out.push(Symbol {
                    kind,
                    name: name.to_string(),
                    byte_range: inner.start_byte()..inner.end_byte().min(inner.start_byte() + 80),
                });
            }
        }
    }
}
