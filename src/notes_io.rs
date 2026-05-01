//! Byte-level git-notes operations for `refs/notes/<ref>` refs.
//!
//! Schema-agnostic. `git_notes` (findings) and `git_comments` (discuss) both
//! call into this for the git side; each handles its own non-git fallback.

use std::path::Path;
use std::process::{Command, Stdio};

/// Read the raw note bytes for `commit` from `refs/notes/<note_ref>`. Returns
/// `Err` if the read fails (no note, not a git repo, etc).
pub fn read_git(repo: &Path, note_ref: &str, commit: &str) -> anyhow::Result<Vec<u8>> {
    let output = Command::new("git")
        .current_dir(repo)
        .args(["notes", "--ref", note_ref, "show", commit])
        .output()
        .map_err(|e| anyhow::anyhow!("failed to run git notes show: {e}"))?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "git notes show failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output.stdout)
}

/// Write `bytes` as the note for `commit` on `refs/notes/<note_ref>`,
/// overwriting any existing note atomically.
pub fn write_git(repo: &Path, note_ref: &str, commit: &str, bytes: &[u8]) -> anyhow::Result<()> {
    let mut child = Command::new("git")
        .current_dir(repo)
        .args(["notes", "--ref", note_ref, "add", "-f", "-F", "-", commit])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to run git notes add: {e}"))?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("failed to open stdin for git notes add"))?;
        use std::io::Write;
        stdin.write_all(bytes)?;
    }

    let status = child
        .wait()
        .map_err(|e| anyhow::anyhow!("failed to wait for git notes add: {e}"))?;
    if !status.success() {
        return Err(anyhow::anyhow!(
            "git notes add failed with status: {status}"
        ));
    }
    Ok(())
}

/// `git fetch <remote> refs/notes/<note_ref>:refs/notes/<note_ref>`. A missing
/// remote ref is treated as success (nothing to fetch yet).
pub fn fetch(repo: &Path, remote: &str, note_ref: &str) -> anyhow::Result<()> {
    let refspec = format!("refs/notes/{0}:refs/notes/{0}", note_ref);
    let output = Command::new("git")
        .current_dir(repo)
        .args(["fetch", remote, &refspec])
        .output()
        .map_err(|e| anyhow::anyhow!("failed to run git fetch: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("couldn't find remote ref") {
            return Ok(());
        }
        return Err(anyhow::anyhow!("git fetch failed: {stderr}"));
    }
    Ok(())
}

/// `git push <remote> refs/notes/<note_ref>:refs/notes/<note_ref>`.
pub fn push(repo: &Path, remote: &str, note_ref: &str) -> anyhow::Result<()> {
    let refspec = format!("refs/notes/{0}:refs/notes/{0}", note_ref);
    let output = Command::new("git")
        .current_dir(repo)
        .args(["push", remote, &refspec])
        .output()
        .map_err(|e| anyhow::anyhow!("failed to run git push: {e}"))?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "git push failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// One-shot migration of the legacy `refs/notes/casual-review` (Phase 3) to
/// the hierarchical `refs/notes/casual-review/findings` (Phase 4). Idempotent:
/// no-op if the legacy ref is absent or the directory is not a git repo.
///
/// Git refuses to have both `casual-review` and `casual-review/<sub>` — so
/// this MUST run before touching any of the new sub-refs.
pub fn migrate_legacy_findings_ref(repo: &Path) -> anyhow::Result<()> {
    let legacy = "refs/notes/casual-review";
    let target = "refs/notes/casual-review/findings";

    let rev = Command::new("git")
        .current_dir(repo)
        .args(["rev-parse", "--verify", "--quiet", legacy])
        .output();
    let Ok(out) = rev else {
        return Ok(());
    };
    if !out.status.success() {
        return Ok(());
    }
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if sha.is_empty() {
        return Ok(());
    }

    // Delete first: git refuses to have both `casual-review` and
    // `casual-review/<sub>` simultaneously. The note commit at `sha` becomes
    // unreachable for ~14 days (default GC grace) until the new ref points
    // back at it on the next line — well within any plausible window.
    let delete = Command::new("git")
        .current_dir(repo)
        .args(["update-ref", "-d", legacy])
        .output()
        .map_err(|e| anyhow::anyhow!("failed to delete {legacy}: {e}"))?;
    if !delete.status.success() {
        return Err(anyhow::anyhow!(
            "git update-ref -d {legacy} failed: {}",
            String::from_utf8_lossy(&delete.stderr)
        ));
    }

    let create = Command::new("git")
        .current_dir(repo)
        .args(["update-ref", target, &sha])
        .output()
        .map_err(|e| anyhow::anyhow!("failed to create {target}: {e}"))?;
    if !create.status.success() {
        return Err(anyhow::anyhow!(
            "git update-ref {target} failed: {}",
            String::from_utf8_lossy(&create.stderr)
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn migrate_is_noop_without_git_repo() {
        let tmp = TempDir::new().unwrap();
        migrate_legacy_findings_ref(tmp.path()).unwrap();
    }
}
