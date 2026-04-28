use casual_review::diagnostic::Diagnostic;
use casual_review::engine::run_paths;
use std::path::PathBuf;
use tempfile::TempDir;

fn run_on(content: &str, file_name: &str) -> Vec<Diagnostic> {
    let tmp = TempDir::new().expect("create tempdir");
    let path = tmp.path().join(file_name);
    std::fs::write(&path, content).expect("write fixture");

    let output = run_paths(&[path]).expect("engine run");
    normalize(output.diagnostics, tmp.path())
}

fn normalize(mut diagnostics: Vec<Diagnostic>, base: &std::path::Path) -> Vec<Diagnostic> {
    for d in &mut diagnostics {
        d.primary.file = strip(&d.primary.file, base);
        for label in &mut d.labels {
            label.span.file = strip(&label.span.file, base);
        }
    }
    diagnostics
}

fn strip(p: &std::path::Path, base: &std::path::Path) -> PathBuf {
    p.strip_prefix(base).unwrap_or(p).to_path_buf()
}

#[test]
fn rust_todo_marker() {
    let diagnostics = run_on(
        "// TODO finish this\nfn add(a: i32, b: i32) -> i32 { a + b }\n",
        "x.rs",
    );
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn rust_trailing_whitespace() {
    let diagnostics = run_on("fn ok() {}   \nfn ok2() {}\n", "x.rs");
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn rust_parse_error() {
    let diagnostics = run_on("fn broken(x: i32) -> i32 {\n    x +\n}\n", "x.rs");
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn python_todo_marker() {
    let diagnostics = run_on("# TODO finish this\ndef add(a, b):\n    return a + b\n", "x.py");
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn python_trailing_whitespace() {
    let diagnostics = run_on("def ok():   \n    pass\n", "x.py");
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn typescript_todo_marker() {
    let diagnostics = run_on(
        "// TODO refactor\nexport function add(a: number, b: number): number { return a + b; }\n",
        "x.ts",
    );
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn typescript_block_comment_fixme() {
    let diagnostics = run_on(
        "/* FIXME validate */\nexport const parse = (s: string): unknown => JSON.parse(s);\n",
        "x.ts",
    );
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn tsx_jsx_with_marker() {
    let diagnostics = run_on(
        "// XXX accessibility\nexport const Btn = ({label}: {label: string}) => <button>{label}</button>;\n",
        "x.tsx",
    );
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn typescript_trailing_whitespace() {
    let diagnostics = run_on("export const x = 1;   \nexport const y = 2;\n", "x.ts");
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn tsx_parse_error() {
    let diagnostics = run_on(
        "export const Bad = () => <div>unclosed;\n",
        "x.tsx",
    );
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn fixme_and_xxx_markers() {
    let diagnostics = run_on(
        "// FIXME bug here\n// XXX also here\nfn f() {}\n",
        "x.rs",
    );
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn rust_unwrap_and_expect() {
    let diagnostics = run_on(
        "fn t() {\n    let r: Result<i32, ()> = Ok(1);\n    r.unwrap();\n    let o: Option<i32> = Some(1);\n    o.expect(\"oops\");\n}\n",
        "x.rs",
    );
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn rust_debug_macros() {
    let diagnostics = run_on(
        "fn t() {\n    println!(\"a\");\n    eprintln!(\"b\");\n    dbg!(1);\n}\n",
        "x.rs",
    );
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn python_bare_except_vs_typed() {
    let diagnostics = run_on(
        "def a():\n    try:\n        pass\n    except:\n        pass\n\ndef b():\n    try:\n        pass\n    except ValueError:\n        pass\n",
        "x.py",
    );
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn python_debug_calls() {
    let diagnostics = run_on(
        "def t():\n    print(\"hi\")\n    breakpoint()\n    return 1\n",
        "x.py",
    );
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn typescript_any_type() {
    let diagnostics = run_on(
        "export function f(x: any): any { return x; }\n",
        "x.ts",
    );
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn typescript_console_log() {
    let diagnostics = run_on(
        "export function f(x: number) { console.log(x); console.warn(x); return x; }\n",
        "x.ts",
    );
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn rust_empty_err_arm() {
    let diagnostics = run_on(
        "fn handle(r: Result<i32, ()>) {\n    match r {\n        Ok(_) => { let _ = 1; }\n        Err(_) => {}\n    }\n}\n",
        "x.rs",
    );
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn python_empty_except() {
    let diagnostics = run_on(
        "def a():\n    try:\n        x = 1\n    except ValueError:\n        pass\n",
        "x.py",
    );
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn typescript_empty_catch() {
    let diagnostics = run_on(
        "export function f() {\n    try { risky(); } catch (e) {}\n}\ndeclare function risky(): void;\n",
        "x.ts",
    );
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn rust_disabled_test_ignore() {
    let diagnostics = run_on(
        "#[ignore]\n#[test]\nfn t() { assert_eq!(1, 2); }\n",
        "x.rs",
    );
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn python_disabled_test_pytest_skip() {
    let diagnostics = run_on(
        "import pytest\n\n@pytest.mark.skip(reason=\"flaky\")\ndef test_a():\n    assert False\n",
        "x.py",
    );
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn typescript_disabled_test_skip_and_only() {
    let diagnostics = run_on(
        "export function s() {\n    it.skip(\"a\", () => {});\n    xit(\"b\", () => {});\n    it.only(\"c\", () => {});\n}\ndeclare const it: any;\ndeclare const xit: any;\n",
        "x.ts",
    );
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn typescript_escape_hatches() {
    let diagnostics = run_on(
        "// @ts-nocheck\nexport function f(x: { a?: string }) {\n    // @ts-ignore\n    return x.a!.toUpperCase();\n}\n",
        "x.ts",
    );
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn hardcoded_secrets_basic() {
    let diagnostics = run_on(
        "const k = \"AKIAIOSFODNN7EXAMPLE\";\nconst t = \"ghp_abcdefghijklmnopqrstuvwxyz0123456789ABCD\";\n",
        "x.ts",
    );
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn java_todo_and_debug() {
    let diagnostics = run_on(
        "// TODO: refactor\npublic class C {\n    public void f(int x) {\n        System.out.println(x);\n    }\n}\n",
        "C.java",
    );
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn java_empty_catch() {
    let diagnostics = run_on(
        "public class C {\n    public void f() {\n        try { throw new Exception(); }\n        catch (Exception e) {}\n    }\n}\n",
        "C.java",
    );
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn java_disabled_test() {
    let diagnostics = run_on(
        "public class CTest {\n    @org.junit.jupiter.api.Disabled\n    public void t() {}\n}\n",
        "CTest.java",
    );
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn java_print_stack_trace() {
    let diagnostics = run_on(
        "public class C {\n    public void f() {\n        try { throw new Exception(); }\n        catch (Exception e) { e.printStackTrace(); }\n    }\n}\n",
        "C.java",
    );
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn rust_large_function() {
    let mut body = String::from("fn big() {\n");
    for i in 0..50 {
        body.push_str(&format!("    let _x{i} = {i};\n"));
    }
    body.push_str("}\n");
    let diagnostics = run_on(&body, "x.rs");
    insta::assert_yaml_snapshot!(diagnostics);
}

#[test]
fn marker_word_boundary_no_match() {
    let diagnostics = run_on("// TODOs are stored separately\nfn f() {}\n", "x.rs");
    insta::assert_yaml_snapshot!(diagnostics);
}
