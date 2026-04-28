use super::{Rule, RuleCtx};
use crate::diagnostic::{Diagnostic, Severity, Span};
use crate::parse::Language;
use tree_sitter::Node;

pub struct AssertionFreeTestRule;

impl Rule for AssertionFreeTestRule {
    fn id(&self) -> &'static str {
        "assertion-free-test"
    }

    fn explain(&self) -> &'static str {
        "A test function with no assertion calls in its body. The most common shape: \
         someone wrote a `#[test] fn shouldFoo` or `@Test public void shouldFoo()` to scaffold \
         a test name, intended to fill in the body later, and forgot.\n\n\
         An assertion-free test can never fail meaningfully — it 'passes' as long as the code \
         doesn't panic or throw, which gives false confidence about coverage.\n\n\
         The rule recognises a wide set of assertion shapes: `assert*!`/`debug_assert*!`/\
         `panic!`/`unreachable!` (Rust), `assert` statements + `pytest.raises`/`pytest.warns`/\
         `self.assert*` (Python), `expect(...)`/`assert(...)`/`chai.*` (TS), \
         `assert*`/`verify`/`fail` (Java including Mockito and AssertJ).\n\n\
         Fix: add the missing assertion, or delete the test if it's no longer needed. \
         A pure 'this code doesn't crash' check is occasionally legitimate — write the \
         assertion explicitly anyway (`assert!(thing(args).is_ok())`) to document intent."
    }

    fn run(&self, ctx: &RuleCtx<'_>) -> Vec<Diagnostic> {
        let (Some(tree), Some(language)) = (ctx.tree, ctx.language) else {
            return Vec::new();
        };
        let source = ctx.source.as_bytes();
        let mut tests = Vec::new();
        collect_tests(&tree.root_node(), language, source, &mut tests);

        let mut diagnostics = Vec::new();
        for test in tests {
            if has_assertion(language, &test.body, source) {
                continue;
            }
            let span = Span::from_byte_range(
                ctx.path.to_path_buf(),
                ctx.source,
                test.outer.start_byte()..test.outer.end_byte().min(test.outer.start_byte() + 80),
            );
            if !ctx.line_in_changes(span.line_start) {
                continue;
            }
            let label = test.name.as_deref().unwrap_or("<anonymous>");
            diagnostics.push(
                Diagnostic::new(
                    "assertion-free-test",
                    Severity::Warning,
                    format!("test `{label}` has no assertions"),
                    span,
                )
                .with_help(
                    "a test that doesn't assert anything can't meaningfully fail — \
                     add assertions or delete the test",
                ),
            );
        }
        diagnostics
    }
}

struct TestFn<'tree> {
    outer: Node<'tree>,
    body: Node<'tree>,
    name: Option<String>,
}

fn collect_tests<'tree>(
    node: &Node<'tree>,
    lang: Language,
    source: &[u8],
    out: &mut Vec<TestFn<'tree>>,
) {
    match lang {
        Language::Rust => {
            if node.kind() == "function_item" && rust_is_test(node, source) {
                if let Some(body) = node.child_by_field_name("body") {
                    let name = node
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok().map(String::from));
                    out.push(TestFn {
                        outer: *node,
                        body,
                        name,
                    });
                }
            }
        }
        Language::Python => {
            if node.kind() == "function_definition" {
                if let Some(name_node) = node.child_by_field_name("name") {
                    if let Ok(name) = name_node.utf8_text(source) {
                        if name.starts_with("test_") || name == "test" {
                            if let Some(body) = node.child_by_field_name("body") {
                                out.push(TestFn {
                                    outer: *node,
                                    body,
                                    name: Some(name.to_string()),
                                });
                            }
                        }
                    }
                }
            }
        }
        Language::TypeScript | Language::Tsx => {
            if node.kind() == "call_expression" {
                if let Some(t) = ts_test_call(node, source) {
                    out.push(t);
                }
            }
        }
        Language::Java => {
            if node.kind() == "method_declaration" && java_is_test(node, source) {
                if let Some(body) = node.child_by_field_name("body") {
                    let name = node
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok().map(String::from));
                    out.push(TestFn {
                        outer: *node,
                        body,
                        name,
                    });
                }
            }
        }
    }

    let mut walker = node.walk();
    for child in node.children(&mut walker) {
        collect_tests(&child, lang, source, out);
    }
}

fn rust_is_test(fn_node: &Node<'_>, source: &[u8]) -> bool {
    let mut prev = fn_node.prev_sibling();
    while let Some(p) = prev {
        if p.kind() != "attribute_item" {
            break;
        }
        let text = p.utf8_text(source).unwrap_or("");
        if text == "#[test]"
            || text.starts_with("#[test(")
            || text.starts_with("#[tokio::test")
            || text.starts_with("#[async_std::test")
            || text.starts_with("#[wasm_bindgen_test")
        {
            return true;
        }
        prev = p.prev_sibling();
    }
    false
}

fn java_is_test(method: &Node<'_>, source: &[u8]) -> bool {
    let mut walker = method.walk();
    for child in method.children(&mut walker) {
        if child.kind() != "modifiers" {
            continue;
        }
        let mut sub = child.walk();
        for m in child.children(&mut sub) {
            if !matches!(m.kind(), "annotation" | "marker_annotation") {
                continue;
            }
            let Some(name) = m.child_by_field_name("name") else {
                continue;
            };
            let text = name.utf8_text(source).unwrap_or("");
            if text == "Test"
                || text == "ParameterizedTest"
                || text == "RepeatedTest"
                || text.ends_with(".Test")
            {
                return true;
            }
        }
    }
    false
}

fn ts_test_call<'tree>(call: &Node<'tree>, source: &[u8]) -> Option<TestFn<'tree>> {
    let function_node = call.child_by_field_name("function")?;
    let name = function_node.utf8_text(source).ok()?;
    if !matches!(name, "it" | "test") {
        return None;
    }

    let args = call.child_by_field_name("arguments")?;
    let mut named_args = args
        .children(&mut args.walk())
        .filter(|c| !matches!(c.kind(), "(" | ")" | ","))
        .collect::<Vec<_>>();
    if named_args.len() < 2 {
        return None;
    }

    let test_name = named_args[0].utf8_text(source).ok().map(|s| {
        s.trim_matches('"')
            .trim_matches('\'')
            .trim_matches('`')
            .to_string()
    });

    let callback = named_args.remove(1);
    if !matches!(callback.kind(), "arrow_function" | "function_expression") {
        return None;
    }
    let body = callback.child_by_field_name("body")?;
    if body.kind() != "statement_block" {
        return None;
    }

    Some(TestFn {
        outer: *call,
        body,
        name: test_name,
    })
}

fn has_assertion(lang: Language, body: &Node<'_>, source: &[u8]) -> bool {
    let mut found = false;
    walk_for_assertion(body, lang, source, &mut found);
    found
}

fn walk_for_assertion(node: &Node<'_>, lang: Language, source: &[u8], found: &mut bool) {
    if *found {
        return;
    }

    if node_is_assertion(lang, node, source) {
        *found = true;
        return;
    }

    let mut walker = node.walk();
    for child in node.children(&mut walker) {
        walk_for_assertion(&child, lang, source, found);
        if *found {
            return;
        }
    }
}

fn node_is_assertion(lang: Language, node: &Node<'_>, source: &[u8]) -> bool {
    let kind = node.kind();
    match lang {
        Language::Rust => {
            if kind != "macro_invocation" {
                return false;
            }
            let Some(macro_name) = node.child_by_field_name("macro") else {
                return false;
            };
            let text = macro_name.utf8_text(source).unwrap_or("");
            text.starts_with("assert")
                || text.starts_with("debug_assert")
                || text == "panic"
                || text == "unreachable"
        }
        Language::Python => {
            if kind == "assert_statement" {
                return true;
            }
            if kind == "call" {
                let text = node.utf8_text(source).unwrap_or("");
                return text.contains("assert")
                    || text.contains("pytest.raises")
                    || text.contains("pytest.warns")
                    || text.contains(".fail(");
            }
            false
        }
        Language::TypeScript | Language::Tsx => {
            if kind != "call_expression" {
                return false;
            }
            let Some(fn_node) = node.child_by_field_name("function") else {
                return false;
            };
            let text = fn_node.utf8_text(source).unwrap_or("");
            text == "expect"
                || text == "assert"
                || text.starts_with("assert.")
                || text.starts_with("expect.")
                || text.starts_with("chai.")
                || text == "should"
                || text.ends_with(".should")
        }
        Language::Java => {
            if kind != "method_invocation" {
                return false;
            }
            let Some(name_node) = node.child_by_field_name("name") else {
                return false;
            };
            let text = name_node.utf8_text(source).unwrap_or("");
            text.starts_with("assert") || text == "verify" || text == "fail"
        }
    }
}
