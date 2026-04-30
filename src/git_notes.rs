/// Git notes I/O for persisting findings.
/// Stores findings as JSON files that can be committed to the repo.
///
/// MVP approach: store in .cr-findings/ directory (excluded from git by default)
/// Phase 3+ approach: use refs/notes/casual-review for true git notes integration
use crate::notes::NotesPayload;
use std::path::Path;

const FINDINGS_DIR: &str = ".cr-findings";

/// Read findings from local storage.
pub fn read_notes(_repo_path: &Path, _commit: &str) -> anyhow::Result<Option<NotesPayload>> {
    // TODO: Implement reading from git notes or findings storage
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
