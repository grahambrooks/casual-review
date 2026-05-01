//! Comments persistence on `refs/notes/casual-review/discuss`.
//!
//! Calls into `notes_io` for the git side and falls back to file storage
//! under `.cr-comments/` when the working directory is not a git repo.

use crate::comments::CommentsPayload;
use crate::notes_io;
use std::path::Path;

const DISCUSS_REF: &str = "casual-review/discuss";
const COMMENTS_DIR: &str = ".cr-comments";

pub fn read_comments(repo_path: &Path, commit: &str) -> anyhow::Result<Option<CommentsPayload>> {
    let _ = notes_io::migrate_legacy_findings_ref(repo_path);

    if let Ok(bytes) = notes_io::read_git(repo_path, DISCUSS_REF, commit) {
        let payload = CommentsPayload::from_json(&String::from_utf8_lossy(&bytes))?;
        return Ok(Some(payload));
    }

    read_from_files(repo_path, commit)
}

pub fn write_comments(
    repo_path: &Path,
    commit: &str,
    payload: &CommentsPayload,
) -> anyhow::Result<()> {
    let _ = notes_io::migrate_legacy_findings_ref(repo_path);

    let json = payload.to_json()?;
    if notes_io::write_git(repo_path, DISCUSS_REF, commit, json.as_bytes()).is_ok() {
        return Ok(());
    }

    write_to_files(repo_path, commit, payload)
}

pub fn fetch(repo_path: &Path, remote: &str) -> anyhow::Result<()> {
    let _ = notes_io::migrate_legacy_findings_ref(repo_path);
    notes_io::fetch(repo_path, remote, DISCUSS_REF)
}

pub fn push(repo_path: &Path, remote: &str) -> anyhow::Result<()> {
    let _ = notes_io::migrate_legacy_findings_ref(repo_path);
    notes_io::push(repo_path, remote, DISCUSS_REF)
}

fn fallback_path(repo_path: &Path, commit: &str) -> std::path::PathBuf {
    repo_path.join(COMMENTS_DIR).join(format!("{commit}.json"))
}

fn read_from_files(repo_path: &Path, commit: &str) -> anyhow::Result<Option<CommentsPayload>> {
    let path = fallback_path(repo_path, commit);
    if !path.exists() {
        return Ok(None);
    }
    let json = std::fs::read_to_string(path)?;
    Ok(Some(CommentsPayload::from_json(&json)?))
}

fn write_to_files(repo_path: &Path, commit: &str, payload: &CommentsPayload) -> anyhow::Result<()> {
    let path = fallback_path(repo_path, commit);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, payload.to_json()?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn fallback_roundtrip() -> anyhow::Result<()> {
        let tmp = TempDir::new()?;
        let payload = CommentsPayload::new("abc".to_string());
        write_comments(tmp.path(), "abc", &payload)?;
        let got = read_comments(tmp.path(), "abc")?.expect("payload");
        assert_eq!(got.commit, "abc");
        Ok(())
    }

    #[test]
    fn missing_commit_returns_none() -> anyhow::Result<()> {
        let tmp = TempDir::new()?;
        let got = read_comments(tmp.path(), "missing")?;
        assert!(got.is_none());
        Ok(())
    }
}
