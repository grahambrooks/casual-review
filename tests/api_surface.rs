use casual_review::diagnostic::Diagnostic;
use casual_review::parse::{self, Language};
use casual_review::rules::api_surface_change::ApiSurfaceChangeRule;
use casual_review::rules::{Rule, RuleCtx};
use std::path::PathBuf;

fn run_diff(language: Language, file_name: &str, old: &str, new: &str) -> Vec<Diagnostic> {
    let new_tree = parse::parse(language, new.as_bytes()).expect("parse new");
    let old_tree = parse::parse(language, old.as_bytes()).expect("parse old");
    let path = PathBuf::from(file_name);

    let ctx = RuleCtx {
        path: &path,
        source: new,
        tree: Some(&new_tree),
        language: Some(language),
        changed_lines: None,
        old_source: Some(old),
        old_tree: Some(&old_tree),
    };

    ApiSurfaceChangeRule.run(&ctx)
}

#[test]
fn rust_pub_added() {
    let old = "pub fn a() {}\n";
    let new = "pub fn a() {}\npub fn b() {}\n";
    let d = run_diff(Language::Rust, "lib.rs", old, new);
    insta::assert_yaml_snapshot!(d);
}

#[test]
fn rust_pub_removed() {
    let old = "pub fn a() {}\npub fn b() {}\n";
    let new = "pub fn a() {}\n";
    let d = run_diff(Language::Rust, "lib.rs", old, new);
    insta::assert_yaml_snapshot!(d);
}

#[test]
fn rust_private_change_ignored() {
    let old = "fn private_a() {}\n";
    let new = "fn private_a() {}\nfn private_b() {}\n";
    let d = run_diff(Language::Rust, "lib.rs", old, new);
    insta::assert_yaml_snapshot!(d);
}

#[test]
fn rust_pub_struct_renamed() {
    let old = "pub struct Foo;\n";
    let new = "pub struct Bar;\n";
    let d = run_diff(Language::Rust, "lib.rs", old, new);
    insta::assert_yaml_snapshot!(d);
}

#[test]
fn ts_export_added() {
    let old = "export function a() {}\n";
    let new = "export function a() {}\nexport function b() {}\nexport class C {}\n";
    let d = run_diff(Language::TypeScript, "x.ts", old, new);
    insta::assert_yaml_snapshot!(d);
}

#[test]
fn ts_export_removed() {
    let old = "export function a() {}\nexport interface I {}\n";
    let new = "export function a() {}\n";
    let d = run_diff(Language::TypeScript, "x.ts", old, new);
    insta::assert_yaml_snapshot!(d);
}

#[test]
fn ts_const_export() {
    let old = "export const a = 1;\n";
    let new = "export const a = 1;\nexport const b = 2;\n";
    let d = run_diff(Language::TypeScript, "x.ts", old, new);
    insta::assert_yaml_snapshot!(d);
}

#[test]
fn python_top_level_def_added() {
    let old = "def a():\n    pass\n";
    let new = "def a():\n    pass\n\ndef b():\n    pass\n\nclass C:\n    pass\n";
    let d = run_diff(Language::Python, "x.py", old, new);
    insta::assert_yaml_snapshot!(d);
}

#[test]
fn java_public_class_added() {
    let old = "public class A {}\n";
    let new = "public class A {}\npublic class B {}\npublic interface I {}\n";
    let d = run_diff(Language::Java, "X.java", old, new);
    insta::assert_yaml_snapshot!(d);
}

#[test]
fn java_package_private_ignored() {
    let old = "class A {}\n";
    let new = "class A {}\nclass B {}\n";
    let d = run_diff(Language::Java, "X.java", old, new);
    insta::assert_yaml_snapshot!(d);
}

#[test]
fn java_record_added() {
    let old = "public class A {}\n";
    let new = "public class A {}\npublic record Point(int x, int y) {}\n";
    let d = run_diff(Language::Java, "X.java", old, new);
    insta::assert_yaml_snapshot!(d);
}

#[test]
fn python_underscore_prefix_ignored() {
    let old = "def a():\n    pass\n";
    let new = "def a():\n    pass\n\ndef _internal():\n    pass\n";
    let d = run_diff(Language::Python, "x.py", old, new);
    insta::assert_yaml_snapshot!(d);
}
