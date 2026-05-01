//! Findings persistence on `refs/notes/casual-review/findings`.
//!
//! Calls into `notes_io` for the git side and falls back to file storage
//! under `.cr-findings/` when the working directory is not a git repo (used
//! by integration tests and to keep `cr publish` useful in non-git contexts).

use crate::notes::NotesPayload;
use crate::notes_io;
use std::path::Path;

const FINDINGS_REF: &str = "casual-review/findings";
const FINDINGS_DIR: &str = ".cr-findings";

/// Read findings from git notes (or fallback to file-based storage).
pub fn read_notes(repo_path: &Path, commit: &str) -> anyhow::Result<Option<NotesPayload>> {
    // Idempotent migration of any pre-Phase-4 ref. Safe to call on every read.
    let _ = notes_io::migrate_legacy_findings_ref(repo_path);

    if let Ok(bytes) = notes_io::read_git(repo_path, FINDINGS_REF, commit) {
        let payload: NotesPayload = serde_json::from_slice(&bytes)
            .map_err(|e| anyhow::anyhow!("failed to parse findings JSON from git notes: {e}"))?;
        return Ok(Some(payload));
    }

    read_from_files(repo_path)
}

/// Write findings to git notes (or fallback to file-based storage).
pub fn write_notes(repo_path: &Path, commit: &str, payload: NotesPayload) -> anyhow::Result<()> {
    let _ = notes_io::migrate_legacy_findings_ref(repo_path);

    let json = payload.to_json()?;
    if notes_io::write_git(repo_path, FINDINGS_REF, commit, json.as_bytes()).is_ok() {
        return Ok(());
    }

    write_to_files(repo_path, &payload)
}

/// `git fetch <remote>` for the findings ref. Missing remote ref is OK.
pub fn fetch(repo_path: &Path, remote: &str) -> anyhow::Result<()> {
    let _ = notes_io::migrate_legacy_findings_ref(repo_path);
    notes_io::fetch(repo_path, remote, FINDINGS_REF)
}

/// `git push <remote>` for the findings ref.
pub fn push(repo_path: &Path, remote: &str) -> anyhow::Result<()> {
    let _ = notes_io::migrate_legacy_findings_ref(repo_path);
    notes_io::push(repo_path, remote, FINDINGS_REF)
}

fn read_from_files(repo_path: &Path) -> anyhow::Result<Option<NotesPayload>> {
    let findings_dir = repo_path.join(FINDINGS_DIR);
    if !findings_dir.exists() {
        return Ok(None);
    }

    let mut latest: Option<(std::path::PathBuf, i64)> = None;
    for entry in std::fs::read_dir(&findings_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let Some(stem) = name_str
            .strip_prefix("findings-")
            .and_then(|s| s.strip_suffix(".json"))
        else {
            continue;
        };
        let Ok(ts) = stem.parse::<i64>() else {
            continue;
        };
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        match &latest {
            Some((_, max_ts)) if ts <= *max_ts => {}
            _ => latest = Some((entry.path(), ts)),
        }
    }

    let Some((path, _)) = latest else {
        return Ok(None);
    };
    let contents = std::fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&contents)?))
}

fn write_to_files(repo_path: &Path, payload: &NotesPayload) -> anyhow::Result<()> {
    let findings_dir = repo_path.join(FINDINGS_DIR);
    std::fs::create_dir_all(&findings_dir)?;
    let filename = format!("findings-{}.json", chrono::Utc::now().timestamp());
    let path = findings_dir.join(filename);
    let json = payload.to_json()?;
    std::fs::write(path, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
            .args(["config", "user.email", "test@test.com"])
            .output()
            .unwrap();
        Command::new("git")
            .current_dir(path)
            .args(["config", "user.name", "Test"])
            .output()
            .unwrap();
        std::fs::write(path.join("a.txt"), "hello").unwrap();
        Command::new("git")
            .current_dir(path)
            .args(["add", "a.txt"])
            .output()
            .unwrap();
        Command::new("git")
            .current_dir(path)
            .args(["commit", "-m", "init"])
            .output()
            .unwrap();
    }

    #[test]
    fn writes_and_reads_via_git_notes() -> anyhow::Result<()> {
        let tmp = TempDir::new()?;
        init_repo(tmp.path());

        let payload = NotesPayload {
            schema: "casual-review/finding/1".to_string(),
            tool: "casual-review".to_string(),
            tool_version: "test".to_string(),
            produced_at: "2026-04-30T00:00:00Z".to_string(),
            commit: "HEAD".to_string(),
            findings: vec![],
        };

        write_notes(tmp.path(), "HEAD", payload.clone())?;
        let got = read_notes(tmp.path(), "HEAD")?.expect("findings");
        assert_eq!(got.schema, payload.schema);
        Ok(())
    }

    #[test]
    fn fallback_storage_works() -> anyhow::Result<()> {
        let tmp = TempDir::new()?;
        let payload = NotesPayload {
            schema: "casual-review/finding/1".to_string(),
            tool: "casual-review".to_string(),
            tool_version: "test".to_string(),
            produced_at: "2026-04-30T00:00:00Z".to_string(),
            commit: "abc".to_string(),
            findings: vec![],
        };

        write_to_files(tmp.path(), &payload)?;
        let got = read_from_files(tmp.path())?.expect("findings");
        assert_eq!(got.schema, payload.schema);
        Ok(())
    }

    #[test]
    fn legacy_ref_is_migrated_on_read() -> anyhow::Result<()> {
        let tmp = TempDir::new()?;
        init_repo(tmp.path());

        // Seed a note on the legacy ref.
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

        // First read triggers migration and should still return the payload.
        let got = read_notes(tmp.path(), "HEAD")?.expect("payload after migration");
        assert_eq!(got.schema, "casual-review/finding/1");

        // Legacy ref is gone.
        let rev = Command::new("git")
            .current_dir(tmp.path())
            .args([
                "rev-parse",
                "--verify",
                "--quiet",
                "refs/notes/casual-review",
            ])
            .output()?;
        assert!(!rev.status.success(), "legacy ref should be deleted");

        Ok(())
    }
}
