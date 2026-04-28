use super::{Rule, RuleCtx};
use crate::diagnostic::{Diagnostic, Severity, Span};
use crate::parse::Language;
use tree_sitter::Node;

pub struct ComplexityRule;

/// Diagnostic threshold. Sonar's default is 15. Will eventually be configurable.
const THRESHOLD: u32 = 15;

impl Rule for ComplexityRule {
    fn id(&self) -> &'static str {
        "cognitive-complexity"
    }

    fn run(&self, ctx: &RuleCtx<'_>) -> Vec<Diagnostic> {
        let (Some(tree), Some(language)) = (ctx.tree, ctx.language) else {
            return Vec::new();
        };
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        let state = WalkState {
            language,
            source: source_bytes,
        };
        find_and_analyze(&tree.root_node(), &state, ctx, &mut diagnostics);
        diagnostics
    }
}

struct WalkState<'a> {
    language: Language,
    source: &'a [u8],
}

fn find_and_analyze(
    node: &Node<'_>,
    state: &WalkState,
    ctx: &RuleCtx,
    out: &mut Vec<Diagnostic>,
) {
    if is_function_like(state.language, node.kind()) {
        let (score, name) = analyze_function(node, state);
        if score > THRESHOLD {
            let span = Span::from_byte_range(
                ctx.path.to_path_buf(),
                ctx.source,
                node.start_byte()..node.end_byte().min(node.start_byte() + 80),
            );
            if ctx.line_in_changes(span.line_start) {
                let label = name.unwrap_or_else(|| "<anonymous>".to_string());
                out.push(
                    Diagnostic::new(
                        "cognitive-complexity",
                        Severity::Warning,
                        format!(
                            "function `{label}` has cognitive complexity {score} (threshold: {THRESHOLD})"
                        ),
                        span,
                    )
                    .with_note(
                        "score grows with nesting depth; flat code with the same number of \
                         branches scores much lower",
                    )
                    .with_help(
                        "extract helpers, return early, or invert conditions to reduce nesting",
                    ),
                );
            }
        }
    }

    let mut walker = node.walk();
    for child in node.children(&mut walker) {
        find_and_analyze(&child, state, ctx, out);
    }
}

fn analyze_function(fn_node: &Node<'_>, state: &WalkState) -> (u32, Option<String>) {
    let body = function_body(state.language, fn_node);
    let name = function_name(state.language, fn_node, state.source);
    let mut score = 0u32;
    if let Some(body) = body {
        visit(&body, 0, state, &mut score);
    }
    (score, name)
}

fn visit(node: &Node<'_>, nesting: u32, state: &WalkState, score: &mut u32) {
    let kind = node.kind();

    // Stop at nested function bodies — they're analyzed separately by the
    // outer find_and_analyze recursion. Sonar counts them once, on their own.
    if is_function_like(state.language, kind) {
        return;
    }

    let contribution = classify(state.language, node, state.source);
    *score += contribution.score_delta(nesting);
    let new_nesting = nesting + contribution.nesting_delta();

    let mut walker = node.walk();
    for child in node.children(&mut walker) {
        visit(&child, new_nesting, state, score);
    }
}

#[derive(Copy, Clone)]
enum Contribution {
    None,
    LinearBreak,
    NestingIncrement,
}

impl Contribution {
    fn score_delta(self, nesting: u32) -> u32 {
        match self {
            Self::None => 0,
            Self::LinearBreak => 1,
            Self::NestingIncrement => 1 + nesting,
        }
    }
    fn nesting_delta(self) -> u32 {
        match self {
            Self::None | Self::LinearBreak => 0,
            Self::NestingIncrement => 1,
        }
    }
}

fn classify(lang: Language, node: &Node<'_>, source: &[u8]) -> Contribution {
    let kind = node.kind();
    match lang {
        Language::Rust => match kind {
            "if_expression" | "match_expression" | "for_expression" | "while_expression"
            | "loop_expression" => Contribution::NestingIncrement,
            "binary_expression" if is_short_circuit(node, source) => Contribution::LinearBreak,
            _ => Contribution::None,
        },
        Language::Python => match kind {
            "if_statement" | "for_statement" | "while_statement" | "match_statement" => {
                Contribution::NestingIncrement
            }
            "except_clause" => Contribution::NestingIncrement,
            "boolean_operator" => Contribution::LinearBreak,
            _ => Contribution::None,
        },
        Language::TypeScript | Language::Tsx => match kind {
            "if_statement"
            | "for_statement"
            | "for_in_statement"
            | "for_of_statement"
            | "while_statement"
            | "do_statement"
            | "switch_statement"
            | "ternary_expression" => Contribution::NestingIncrement,
            "catch_clause" => Contribution::NestingIncrement,
            "binary_expression" if is_short_circuit(node, source) => Contribution::LinearBreak,
            _ => Contribution::None,
        },
        Language::Java => match kind {
            "if_statement"
            | "for_statement"
            | "enhanced_for_statement"
            | "while_statement"
            | "do_statement"
            | "switch_statement"
            | "switch_expression"
            | "ternary_expression" => Contribution::NestingIncrement,
            "catch_clause" => Contribution::NestingIncrement,
            "binary_expression" if is_short_circuit(node, source) => Contribution::LinearBreak,
            _ => Contribution::None,
        },
    }
}

fn is_short_circuit(node: &Node<'_>, source: &[u8]) -> bool {
    let mut walker = node.walk();
    for child in node.children(&mut walker) {
        let text = child.utf8_text(source).unwrap_or("");
        if text == "&&" || text == "||" {
            return true;
        }
    }
    false
}

fn is_function_like(lang: Language, kind: &str) -> bool {
    match lang {
        Language::Rust => matches!(kind, "function_item" | "closure_expression"),
        Language::Python => matches!(kind, "function_definition" | "lambda"),
        Language::TypeScript | Language::Tsx => matches!(
            kind,
            "function_declaration"
                | "function_expression"
                | "arrow_function"
                | "method_definition"
        ),
        Language::Java => matches!(
            kind,
            "method_declaration" | "constructor_declaration" | "lambda_expression"
        ),
    }
}

fn function_body<'tree>(lang: Language, fn_node: &Node<'tree>) -> Option<Node<'tree>> {
    if let Some(body) = fn_node.child_by_field_name("body") {
        return Some(body);
    }
    let kind_to_find = match lang {
        Language::Rust => "block",
        Language::Python => "block",
        Language::TypeScript | Language::Tsx => "statement_block",
        Language::Java => "block",
    };
    super::util::find_child(fn_node, kind_to_find)
}

fn function_name(lang: Language, fn_node: &Node<'_>, source: &[u8]) -> Option<String> {
    let name_node = fn_node.child_by_field_name("name")?;
    let _ = lang;
    name_node.utf8_text(source).ok().map(String::from)
}
