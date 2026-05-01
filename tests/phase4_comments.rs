/// Phase 4 integration tests: user-authored comments on `casual-review/discuss`.
use casual_review::comments::{
    author_from_git, comment_id, sha256_hex, Anchor, Author, Comment, CommentsPayload,
};
use casual_review::git_comments;
use std::path::{Path, PathBuf};
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
    let out = Command::new("git")
        .current_dir(path)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn make_comment(author: &Author, anchor: Anchor, body: &str, parent: Option<&str>) -> Comment {
    let created_at = chrono::Utc::now().to_rfc3339();
    let id = comment_id(author, &created_at, &anchor, body);
    Comment {
        id,
        author: author.clone(),
        created_at,
        anchor,
        body: body.to_string(),
        parent: parent.map(String::from),
        resolved: false,
        origin_commit: None,
    }
}

#[test]
fn add_and_list_comment() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    init_repo(tmp.path());
    let sha = head_sha(tmp.path());
    let author = author_from_git(tmp.path())?;

    let bytes = b"fn main() {";
    let anchor = Anchor {
        file: Some(PathBuf::from("hello.rs")),
        line_range: (1, 1),
        byte_range: (0, bytes.len()),
        anchor_text_sha: sha256_hex(bytes),
    };
    let comment = make_comment(&author, anchor, "why a main fn?", None);

    let mut payload = CommentsPayload::new(sha.clone());
    payload.comments.push(comment.clone());
    git_comments::write_comments(tmp.path(), &sha, &payload)?;

    let read = git_comments::read_comments(tmp.path(), &sha)?.expect("payload");
    assert_eq!(read.comments.len(), 1);
    assert_eq!(read.comments[0].id, comment.id);
    assert_eq!(read.comments[0].author.email, "alice@test.com");
    Ok(())
}

#[test]
fn replies_thread_via_parent() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    init_repo(tmp.path());
    let sha = head_sha(tmp.path());
    let author = author_from_git(tmp.path())?;

    let anchor = Anchor {
        file: Some(PathBuf::from("hello.rs")),
        line_range: (2, 2),
        byte_range: (12, 30),
        anchor_text_sha: sha256_hex(b"    println!(\"hi\");"),
    };
    let parent = make_comment(&author, anchor.clone(), "is println the right call?", None);
    let reply = make_comment(&author, anchor, "yeah, leave it", Some(&parent.id));

    let mut payload = CommentsPayload::new(sha.clone());
    payload.comments.push(parent.clone());
    payload.comments.push(reply.clone());
    git_comments::write_comments(tmp.path(), &sha, &payload)?;

    let read = git_comments::read_comments(tmp.path(), &sha)?.expect("payload");
    assert_eq!(read.comments.len(), 2);

    let threaded = read
        .comments
        .iter()
        .find(|c| c.parent.as_deref() == Some(&parent.id))
        .expect("reply present");
    assert_eq!(threaded.body, "yeah, leave it");
    Ok(())
}

#[test]
fn resolve_marks_thread_resolved() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    init_repo(tmp.path());
    let sha = head_sha(tmp.path());
    let author = author_from_git(tmp.path())?;

    let anchor = Anchor {
        file: Some(PathBuf::from("hello.rs")),
        line_range: (1, 1),
        byte_range: (0, 11),
        anchor_text_sha: sha256_hex(b"fn main() {"),
    };
    let root = make_comment(&author, anchor.clone(), "rename?", None);
    let mut resolution = make_comment(&author, anchor, "fixed in next commit", Some(&root.id));
    resolution.resolved = true;

    let mut payload = CommentsPayload::new(sha.clone());
    payload.comments.push(root.clone());
    payload.comments.push(resolution.clone());
    git_comments::write_comments(tmp.path(), &sha, &payload)?;

    let read = git_comments::read_comments(tmp.path(), &sha)?.expect("payload");
    assert!(read
        .comments
        .iter()
        .any(|c| c.resolved && c.parent.as_deref() == Some(&root.id)));
    // Append-only: original record unmodified
    assert!(read.comments.iter().any(|c| c.id == root.id && !c.resolved));
    Ok(())
}

#[test]
fn anchor_sha_detects_drift() {
    let original = b"fn main() {";
    let drifted = b"fn run() {";
    assert_ne!(sha256_hex(original), sha256_hex(drifted));
    assert_eq!(sha256_hex(original), sha256_hex(b"fn main() {"));
}

#[test]
fn legacy_findings_ref_is_migrated_before_comment_write() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    init_repo(tmp.path());

    // Seed a legacy `casual-review` ref so the migration has work to do.
    let mut child = Command::new("git")
        .current_dir(tmp.path())
        .args(["notes", "--ref", "casual-review", "add", "-F", "-", "HEAD"])
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    {
        use std::io::Write;
        child.stdin.as_mut().unwrap().write_all(b"{\"schema\":\"casual-review/finding/1\",\"tool\":\"casual-review\",\"tool_version\":\"x\",\"produced_at\":\"t\",\"commit\":\"HEAD\",\"findings\":[]}")?;
    }
    assert!(child.wait()?.success());

    // Writing a comment must succeed — i.e. migration ran first so
    // refs/notes/casual-review/discuss can coexist with findings sub-ref.
    let sha = head_sha(tmp.path());
    let payload = CommentsPayload::new(sha.clone());
    git_comments::write_comments(tmp.path(), &sha, &payload)?;

    // Legacy ref is gone; new findings ref exists.
    let legacy = Command::new("git")
        .current_dir(tmp.path())
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            "refs/notes/casual-review",
        ])
        .output()?;
    assert!(!legacy.status.success(), "legacy ref should be deleted");

    let new_findings = Command::new("git")
        .current_dir(tmp.path())
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            "refs/notes/casual-review/findings",
        ])
        .output()?;
    assert!(
        new_findings.status.success(),
        "new findings ref should exist"
    );

    Ok(())
}

#[test]
fn cli_add_list_reply_resolve_end_to_end() -> anyhow::Result<()> {
    use assert_cmd::Command as Cmd;

    let tmp = TempDir::new()?;
    init_repo(tmp.path());

    let run = |args: &[&str]| -> std::process::Output {
        Cmd::cargo_bin("cr")
            .unwrap()
            .current_dir(tmp.path())
            .args(args)
            .output()
            .unwrap()
    };

    let add = run(&[
        "comment",
        "add",
        "hello.rs",
        "--lines",
        "1:1",
        "-m",
        "rename main?",
    ]);
    assert!(add.status.success(), "add failed: {:?}", add);
    let comment_id = String::from_utf8(add.stdout)?.trim().to_string();
    assert!(comment_id.starts_with("CRC-"), "got: {comment_id:?}");

    let list = run(&["comment", "list"]);
    assert!(list.status.success());
    let list_out = String::from_utf8(list.stdout)?;
    assert!(
        list_out.contains(&comment_id),
        "list missing id:\n{list_out}"
    );
    assert!(
        list_out.contains("rename main?"),
        "list missing body:\n{list_out}"
    );

    let reply = run(&["comment", "reply", &comment_id, "-m", "agree"]);
    assert!(reply.status.success(), "reply failed: {:?}", reply);
    let reply_id = String::from_utf8(reply.stdout)?.trim().to_string();
    assert!(reply_id.starts_with("CRC-"));
    assert_ne!(reply_id, comment_id);

    let resolve = run(&["comment", "resolve", &comment_id, "-m", "fixed"]);
    assert!(resolve.status.success(), "resolve failed: {:?}", resolve);

    // After resolve, default list hides the thread.
    let after = String::from_utf8(run(&["comment", "list"]).stdout)?;
    assert!(
        !after.contains(&comment_id),
        "resolved thread still visible:\n{after}"
    );

    // --include-resolved brings it back.
    let with_resolved = String::from_utf8(run(&["comment", "list", "--include-resolved"]).stdout)?;
    assert!(with_resolved.contains(&comment_id));
    assert!(with_resolved.contains("[resolved]"));

    Ok(())
}

#[test]
fn id_is_stable_across_machines() {
    // Cross-machine determinism: same inputs → same id, regardless of host.
    let author = Author {
        name: "Bob".into(),
        email: "bob@x.com".into(),
    };
    let anchor = Anchor {
        file: Some(PathBuf::from("a.rs")),
        line_range: (10, 12),
        byte_range: (100, 150),
        anchor_text_sha: "abc".into(),
    };
    let id_a = comment_id(&author, "2026-04-30T00:00:00Z", &anchor, "hello");
    let id_b = comment_id(&author, "2026-04-30T00:00:00Z", &anchor, "hello");
    assert_eq!(id_a, id_b);
    assert!(id_a.starts_with("CRC-"));
}
