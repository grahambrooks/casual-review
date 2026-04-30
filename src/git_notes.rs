/// Git notes I/O for persisting findings.
/// Stores findings in refs/notes/casual-review using git commands.
/// Falls back to file-based storage if repo is not a git repo.
use crate::notes::NotesPayload;
use std::path::Path;
use std::process::Command;

const NOTES_REF: &str = "casual-review";
const FINDINGS_DIR: &str = ".cr-findings";

/// Read findings from git notes (or fallback to file-based storage).
pub fn read_notes(repo_path: &Path, commit: &str) -> anyhow::Result<Option<NotesPayload>> {
    // Try git notes first
    if let Ok(payload) = read_from_git_notes(repo_path, commit) {
        return Ok(Some(payload));
    }

    // Fall back to file-based storage for migration/compatibility
    read_from_files(repo_path)
}

/// Write findings to git notes (or fallback to file-based storage).
pub fn write_notes(repo_path: &Path, commit: &str, payload: NotesPayload) -> anyhow::Result<()> {
    // Try git notes first
    if write_to_git_notes(repo_path, commit, &payload).is_ok() {
        return Ok(());
    }

    // Fall back to file-based storage if not a git repo
    write_to_files(repo_path, &payload)
}

/// Read findings from git notes ref using git command.
fn read_from_git_notes(repo_path: &Path, commit: &str) -> anyhow::Result<NotesPayload> {
    let output = Command::new("git")
        .current_dir(repo_path)
        .arg("notes")
        .arg("--ref")
        .arg(NOTES_REF)
        .arg("show")
        .arg(commit)
        .output()
        .map_err(|e| anyhow::anyhow!("failed to run git notes show: {}", e))?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "git notes show failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // Parse the JSON
    let payload: NotesPayload = serde_json::from_slice(&output.stdout)
        .map_err(|e| anyhow::anyhow!("failed to parse findings JSON from git notes: {}", e))?;

    Ok(payload)
}

/// Write findings to git notes ref using git command.
fn write_to_git_notes(
    repo_path: &Path,
    commit: &str,
    payload: &NotesPayload,
) -> anyhow::Result<()> {
    // Serialize findings to JSON
    let json = payload.to_json()?;

    let mut child = Command::new("git")
        .current_dir(repo_path)
        .arg("notes")
        .arg("--ref")
        .arg(NOTES_REF)
        .arg("add")
        .arg("-f") // Force: overwrite existing note
        .arg("-F") // Read from stdin
        .arg("-") // Read from stdin
        .arg(commit)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to run git notes add: {}", e))?;

    // Write JSON to stdin
    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("failed to open stdin for git notes add"))?;
        use std::io::Write;
        stdin.write_all(json.as_bytes())?;
    }

    let status = child
        .wait()
        .map_err(|e| anyhow::anyhow!("failed to wait for git notes add: {}", e))?;

    if !status.success() {
        return Err(anyhow::anyhow!(
            "git notes add failed with status: {}",
            status
        ));
    }

    Ok(())
}

/// Read findings from file-based storage (fallback/migration).
fn read_from_files(repo_path: &Path) -> anyhow::Result<Option<NotesPayload>> {
    let findings_dir = repo_path.join(FINDINGS_DIR);

    // If directory doesn't exist, no findings are stored
    if !findings_dir.exists() {
        return Ok(None);
    }

    // Read the most recent findings file
    let mut latest_file: Option<(std::fs::DirEntry, i64)> = None;

    for entry in std::fs::read_dir(&findings_dir)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();

        // Extract timestamp from filename (findings-{timestamp}.json)
        if file_name_str.starts_with("findings-") && file_name_str.ends_with(".json") {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() {
                    if let Some((_, max_ts)) = &latest_file {
                        // Get timestamp from filename
                        let ts_str = file_name_str
                            .strip_prefix("findings-")
                            .and_then(|s| s.strip_suffix(".json"))
                            .unwrap_or("0");
                        if let Ok(ts) = ts_str.parse::<i64>() {
                            if ts > *max_ts {
                                latest_file = Some((entry, ts));
                            }
                        }
                    } else {
                        let ts_str = file_name_str
                            .strip_prefix("findings-")
                            .and_then(|s| s.strip_suffix(".json"))
                            .unwrap_or("0");
                        if let Ok(ts) = ts_str.parse::<i64>() {
                            latest_file = Some((entry, ts));
                        }
                    }
                }
            }
        }
    }

    // Read the latest file if found
    if let Some((entry, _)) = latest_file {
        let path = entry.path();
        let contents = std::fs::read_to_string(path)?;
        let payload: NotesPayload = serde_json::from_str(&contents)?;
        return Ok(Some(payload));
    }

    Ok(None)
}

/// Write findings to file-based storage (fallback/migration).
fn write_to_files(repo_path: &Path, payload: &NotesPayload) -> anyhow::Result<()> {
    // Create findings directory if it doesn't exist
    let findings_dir = repo_path.join(FINDINGS_DIR);
    std::fs::create_dir_all(&findings_dir)?;

    // Write payload to JSON file
    let filename = format!("findings-{}.json", chrono::Utc::now().timestamp());
    let file_path = findings_dir.join(filename);
    let json = payload.to_json()?;
    std::fs::write(file_path, json)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    #[test]
    fn test_git_notes_storage() -> anyhow::Result<()> {
        let tmp = TempDir::new()?;
        let repo_path = tmp.path();

        // Initialize a git repo
        Command::new("git")
            .current_dir(repo_path)
            .arg("init")
            .output()?;

        Command::new("git")
            .current_dir(repo_path)
            .arg("config")
            .arg("user.email")
            .arg("test@test.com")
            .output()?;

        Command::new("git")
            .current_dir(repo_path)
            .arg("config")
            .arg("user.name")
            .arg("Test")
            .output()?;

        // Create and commit a file
        let file_path = repo_path.join("test.txt");
        std::fs::write(&file_path, "test content")?;
        Command::new("git")
            .current_dir(repo_path)
            .arg("add")
            .arg("test.txt")
            .output()?;

        Command::new("git")
            .current_dir(repo_path)
            .arg("commit")
            .arg("-m")
            .arg("initial")
            .output()?;

        // Create a payload
        let payload = NotesPayload {
            schema: "casual-review/finding/1".to_string(),
            tool: "casual-review".to_string(),
            tool_version: "2026.4.28".to_string(),
            produced_at: "2026-04-29T20:00:00Z".to_string(),
            commit: "HEAD".to_string(),
            findings: vec![],
        };

        // Write findings to git notes
        write_to_git_notes(repo_path, "HEAD", &payload)?;

        // Read findings back
        let read_payload = read_from_git_notes(repo_path, "HEAD")?;

        assert_eq!(read_payload.schema, payload.schema);
        assert_eq!(read_payload.tool, payload.tool);
        assert_eq!(read_payload.findings.len(), 0);

        Ok(())
    }

    #[test]
    fn test_notes_storage_fallback() -> anyhow::Result<()> {
        let tmp = TempDir::new()?;
        let repo_path = tmp.path();

        let payload = NotesPayload {
            schema: "casual-review/finding/1".to_string(),
            tool: "casual-review".to_string(),
            tool_version: "0.0.0".to_string(),
            produced_at: "2026-04-29T20:00:00Z".to_string(),
            commit: "abc123".to_string(),
            findings: vec![],
        };

        // Write to file-based storage
        write_to_files(repo_path, &payload)?;

        // Verify file was created
        let findings_dir = repo_path.join(FINDINGS_DIR);
        assert!(findings_dir.exists());

        let entries: Vec<_> = std::fs::read_dir(findings_dir)?
            .filter_map(Result::ok)
            .collect();
        assert!(!entries.is_empty(), "findings file should be created");

        // Read back from files
        let read_payload = read_from_files(repo_path)?;
        assert!(read_payload.is_some());
        assert_eq!(read_payload.unwrap().schema, payload.schema);

        Ok(())
    }
}
