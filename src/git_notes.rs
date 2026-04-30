/// Git notes I/O for persisting findings.
/// Stores findings as JSON files that can be committed to the repo.
///
/// MVP approach: store in .cr-findings/ directory (excluded from git by default)
/// Phase 3+ approach: use refs/notes/casual-review for true git notes integration
use crate::notes::NotesPayload;
use std::path::Path;

const FINDINGS_DIR: &str = ".cr-findings";

/// Read findings from local storage.
/// Returns the most recent findings file if it exists.
pub fn read_notes(repo_path: &Path, _commit: &str) -> anyhow::Result<Option<NotesPayload>> {
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

/// Write findings to local storage.
pub fn write_notes(repo_path: &Path, _commit: &str, payload: NotesPayload) -> anyhow::Result<()> {
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
    use tempfile::TempDir;

    #[test]
    fn test_notes_storage() -> anyhow::Result<()> {
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

        // Write notes
        write_notes(repo_path, "HEAD", payload.clone())?;

        // Verify file was created
        let findings_dir = repo_path.join(FINDINGS_DIR);
        assert!(findings_dir.exists());

        let entries: Vec<_> = std::fs::read_dir(findings_dir)?
            .filter_map(Result::ok)
            .collect();
        assert!(!entries.is_empty(), "findings file should be created");

        Ok(())
    }
}
