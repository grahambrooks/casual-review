/// Phase 4.2 integration tests: $EDITOR fallback and cross-ancestor projection.
use assert_cmd::Command as Cmd;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn init_repo(path: &Path) {
    Command::new("git")
        .current_dir(path)
        .arg("init")
        .output()
        .unwrap();
    Command::new("git")
        .current_dir(path)
        .args(["config", "user.email", "alice@test.com"])
        .output()
        .unwrap();
    Command::new("git")
        .current_dir(path)
        .args(["config", "user.name", "Alice"])
        .output()
        .unwrap();
    std::fs::write(
        path.join("hello.rs"),
        "fn main() {\n    println!(\"hi\");\n}\n",
    )
    .unwrap();
    Command::new("git")
        .current_dir(path)
        .args(["add", "hello.rs"])
        .output()
        .unwrap();
    Command::new("git")
        .current_dir(path)
        .args(["commit", "-m", "init"])
        .output()
        .unwrap();
}

fn head_sha(path: &Path) -> String {
    String::from_utf8_lossy(
        &Command::new("git")
            .current_dir(path)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string()
}

#[cfg(unix)]
#[test]
fn editor_fallback_supplies_body_when_flag_missing() -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let tmp = TempDir::new()?;
    init_repo(tmp.path());

    // Fake editor: writes a fixed body into the path passed by `cr`.
    let editor = tmp.path().join("fake-editor.sh");
    std::fs::write(
        &editor,
        "#!/bin/sh\nprintf 'body via $EDITOR\\n' > \"$1\"\n",
    )?;
    let mut perms = std::fs::metadata(&editor)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&editor, perms)?;

    let out = Cmd::cargo_bin("cr")?
        .current_dir(tmp.path())
        .env("EDITOR", &editor)
        .args(["comment", "add", "hello.rs", "--lines", "1:1"])
        .output()?;
    assert!(
        out.status.success(),
        "add failed: stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let list = Cmd::cargo_bin("cr")?
        .current_dir(tmp.path())
        .args(["comment", "list"])
        .output()?;
    let listing = String::from_utf8_lossy(&list.stdout);
    assert!(
        listing.contains("body via $EDITOR"),
        "listing missing editor body:\n{listing}"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn editor_returning_empty_body_aborts() -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let tmp = TempDir::new()?;
    init_repo(tmp.path());

    // Editor leaves only comment lines (which `cr` strips) → empty body.
    let editor = tmp.path().join("empty-editor.sh");
    std::fs::write(
        &editor,
        "#!/bin/sh\nprintf '# only a comment line\\n' > \"$1\"\n",
    )?;
    let mut perms = std::fs::metadata(&editor)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&editor, perms)?;

    let out = Cmd::cargo_bin("cr")?
        .current_dir(tmp.path())
        .env("EDITOR", &editor)
        .args(["comment", "add", "hello.rs", "--lines", "1:1"])
        .output()?;
    assert!(
        !out.status.success(),
        "expected non-zero exit on empty body"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("empty body") || stderr.contains("aborting"),
        "stderr should mention abort: {stderr}"
    );
    Ok(())
}

/// `cr comment list --include-ancestors` projects comments from earlier
/// commits onto the current commit, tagging each with its origin and
/// flagging staleness when the working tree drifts.
#[test]
fn ancestor_projection_via_cli() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    init_repo(tmp.path());
    let first = head_sha(tmp.path());

    // Comment on the first commit.
    let add = Cmd::cargo_bin("cr")?
        .current_dir(tmp.path())
        .args([
            "comment",
            "add",
            "hello.rs",
            "--lines",
            "1:1",
            "-m",
            "ancestor comment",
        ])
        .output()?;
    assert!(add.status.success());
    let ancestor_id = String::from_utf8(add.stdout)?.trim().to_string();

    // Make a second commit so HEAD ≠ first.
    std::fs::write(
        tmp.path().join("hello.rs"),
        "fn main() {\n    println!(\"hi there\");\n}\n",
    )?;
    Command::new("git")
        .current_dir(tmp.path())
        .args(["add", "hello.rs"])
        .output()?;
    Command::new("git")
        .current_dir(tmp.path())
        .args(["commit", "-m", "edit message"])
        .output()?;
    let second = head_sha(tmp.path());
    assert_ne!(first, second);

    // Default list on HEAD: ancestor comment is hidden.
    let list_default = Cmd::cargo_bin("cr")?
        .current_dir(tmp.path())
        .args(["comment", "list"])
        .output()?;
    assert!(list_default.status.success());
    let default_out = String::from_utf8_lossy(&list_default.stdout);
    assert!(
        !default_out.contains(&ancestor_id),
        "ancestor comment leaked into default list:\n{default_out}"
    );

    // With --include-ancestors: present, tagged with origin.
    let list_proj = Cmd::cargo_bin("cr")?
        .current_dir(tmp.path())
        .args(["comment", "list", "--include-ancestors"])
        .output()?;
    assert!(list_proj.status.success());
    let proj_out = String::from_utf8_lossy(&list_proj.stdout);
    assert!(
        proj_out.contains(&ancestor_id),
        "projection missing ancestor comment:\n{proj_out}"
    );
    let from_marker = format!("[from {}]", &first[..8]);
    assert!(
        proj_out.contains(&from_marker),
        "projection missing origin marker {from_marker}:\n{proj_out}"
    );

    Ok(())
}

#[test]
fn ancestor_projection_json_carries_origin_commit() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    init_repo(tmp.path());
    let first = head_sha(tmp.path());

    let _ = Cmd::cargo_bin("cr")?
        .current_dir(tmp.path())
        .args([
            "comment",
            "add",
            "hello.rs",
            "--lines",
            "1:1",
            "-m",
            "anchor here",
        ])
        .output()?;

    // Second commit so HEAD differs.
    std::fs::write(tmp.path().join("hello.rs"), "fn main() { /* edited */ }\n")?;
    Command::new("git")
        .current_dir(tmp.path())
        .args(["add", "hello.rs"])
        .output()?;
    Command::new("git")
        .current_dir(tmp.path())
        .args(["commit", "-m", "edit"])
        .output()?;

    let json = Cmd::cargo_bin("cr")?
        .current_dir(tmp.path())
        .args(["comment", "list", "--include-ancestors", "--format", "json"])
        .output()?;
    let stdout = String::from_utf8_lossy(&json.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout)?;
    let comments = value["comments"].as_array().expect("comments array");
    assert_eq!(
        comments.len(),
        1,
        "expected 1 projected comment, got: {value}"
    );
    assert_eq!(
        comments[0]["origin_commit"].as_str(),
        Some(first.as_str()),
        "origin_commit not set in JSON: {value}"
    );
    Ok(())
}
