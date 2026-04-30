/// Integration tests for Phase 3: findings persistence and workflow
/// Tests the publish/show/ack command workflow with a temporary git repo

use casual_review::git_notes;
use casual_review::notes::{Finding, Location, NotesPayload};
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_publish_and_show_workflow() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    let repo_path = tmp.path();

    // Create a sample payload
    let payload = NotesPayload {
        schema: "casual-review/finding/1".to_string(),
        tool: "casual-review".to_string(),
        tool_version: "2026.4.28".to_string(),
        produced_at: "2026-04-30T10:00:00Z".to_string(),
        commit: "abc123".to_string(),
        findings: vec![Finding {
            id: "CR-12345678".to_string(),
            rule: "todo-marker".to_string(),
            severity: "note".to_string(),
            location: Location {
                file: PathBuf::from("src/lib.rs"),
                byte_range: (100, 104),
                line_range: (5, 5),
                col_range: (5, 9),
            },
            message: "TODO marker found".to_string(),
            labels: vec![],
            suggestions: vec![],
            parent: None,
        }],
    };

    // Publish the findings
    git_notes::write_notes(repo_path, "abc123", payload.clone())?;

    // Verify the file was created
    let findings_dir = repo_path.join(".cr-findings");
    assert!(findings_dir.exists(), "Findings directory should be created");

    let entries: Vec<_> = std::fs::read_dir(&findings_dir)?
        .filter_map(Result::ok)
        .collect();
    assert!(!entries.is_empty(), "At least one findings file should exist");

    // Read the findings back
    let read_payload = git_notes::read_notes(repo_path, "abc123")?;
    assert!(read_payload.is_some(), "Should be able to read findings");

    let read_payload = read_payload.unwrap();
    assert_eq!(read_payload.schema, "casual-review/finding/1");
    assert_eq!(read_payload.tool, "casual-review");
    assert_eq!(read_payload.findings.len(), 1);
    assert_eq!(read_payload.findings[0].id, "CR-12345678");
    assert_eq!(read_payload.findings[0].rule, "todo-marker");
    assert_eq!(read_payload.findings[0].message, "TODO marker found");

    Ok(())
}

#[test]
fn test_ack_appends_dismissal() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    let repo_path = tmp.path();

    // Create initial findings
    let mut payload = NotesPayload {
        schema: "casual-review/finding/1".to_string(),
        tool: "casual-review".to_string(),
        tool_version: "2026.4.28".to_string(),
        produced_at: "2026-04-30T10:00:00Z".to_string(),
        commit: "abc123".to_string(),
        findings: vec![Finding {
            id: "CR-87654321".to_string(),
            rule: "debug-print".to_string(),
            severity: "warning".to_string(),
            location: Location {
                file: PathBuf::from("src/main.rs"),
                byte_range: (50, 70),
                line_range: (10, 10),
                col_range: (1, 20),
            },
            message: "Debug print statement found".to_string(),
            labels: vec![],
            suggestions: vec![],
            parent: None,
        }],
    };

    git_notes::write_notes(repo_path, "abc123", payload.clone())?;

    // Read back and verify
    let initial_read = git_notes::read_notes(repo_path, "abc123")?
        .expect("Should have readings");
    assert_eq!(initial_read.findings.len(), 1);

    // Simulate acknowledgment by adding dismissal
    let dismissal = Finding {
        id: "CR-87654321-dismissed".to_string(),
        rule: "dismissed".to_string(),
        severity: "note".to_string(),
        location: Location {
            file: PathBuf::from(""),
            byte_range: (0, 0),
            line_range: (0, 0),
            col_range: (0, 0),
        },
        message: "Addressed in PR #456".to_string(),
        labels: vec![],
        suggestions: vec![],
        parent: Some("CR-87654321".to_string()),
    };

    payload.findings.push(dismissal);
    git_notes::write_notes(repo_path, "abc123", payload)?;

    // Read back and verify dismissal was added
    let updated_read = git_notes::read_notes(repo_path, "abc123")?
        .expect("Should have readings");
    assert_eq!(updated_read.findings.len(), 2);

    // Check the dismissal entry
    let dismissal_entry = updated_read
        .findings
        .iter()
        .find(|f| f.rule == "dismissed")
        .expect("Should have dismissal entry");
    assert_eq!(dismissal_entry.parent, Some("CR-87654321".to_string()));
    assert_eq!(
        dismissal_entry.message, "Addressed in PR #456",
        "Dismissal message should match"
    );

    Ok(())
}

#[test]
fn test_multiple_findings_per_commit() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    let repo_path = tmp.path();

    // Create payload with multiple findings
    let payload = NotesPayload {
        schema: "casual-review/finding/1".to_string(),
        tool: "casual-review".to_string(),
        tool_version: "2026.4.28".to_string(),
        produced_at: "2026-04-30T11:00:00Z".to_string(),
        commit: "def456".to_string(),
        findings: vec![
            Finding {
                id: "CR-11111111".to_string(),
                rule: "todo-marker".to_string(),
                severity: "note".to_string(),
                location: Location {
                    file: PathBuf::from("src/lib.rs"),
                    byte_range: (10, 14),
                    line_range: (1, 1),
                    col_range: (1, 5),
                },
                message: "TODO: refactor this".to_string(),
                labels: vec![],
                suggestions: vec![],
                parent: None,
            },
            Finding {
                id: "CR-22222222".to_string(),
                rule: "debug-print".to_string(),
                severity: "warning".to_string(),
                location: Location {
                    file: PathBuf::from("src/main.rs"),
                    byte_range: (200, 220),
                    line_range: (25, 25),
                    col_range: (10, 30),
                },
                message: "println! debug statement".to_string(),
                labels: vec![],
                suggestions: vec![],
                parent: None,
            },
            Finding {
                id: "CR-33333333".to_string(),
                rule: "empty-catch".to_string(),
                severity: "error".to_string(),
                location: Location {
                    file: PathBuf::from("src/handlers.rs"),
                    byte_range: (500, 550),
                    line_range: (42, 45),
                    col_range: (5, 10),
                },
                message: "Empty exception handler".to_string(),
                labels: vec![],
                suggestions: vec![],
                parent: None,
            },
        ],
    };

    git_notes::write_notes(repo_path, "def456", payload)?;

    // Read back and verify all findings are preserved
    let read_payload = git_notes::read_notes(repo_path, "def456")?
        .expect("Should have readings");
    assert_eq!(read_payload.findings.len(), 3, "All 3 findings should be preserved");

    // Verify each finding's details
    assert!(read_payload
        .findings
        .iter()
        .any(|f| f.id == "CR-11111111" && f.rule == "todo-marker"));
    assert!(read_payload
        .findings
        .iter()
        .any(|f| f.id == "CR-22222222" && f.rule == "debug-print"));
    assert!(read_payload
        .findings
        .iter()
        .any(|f| f.id == "CR-33333333" && f.rule == "empty-catch"));

    Ok(())
}

#[test]
fn test_empty_findings() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    let repo_path = tmp.path();

    // Create payload with no findings
    let payload = NotesPayload {
        schema: "casual-review/finding/1".to_string(),
        tool: "casual-review".to_string(),
        tool_version: "2026.4.28".to_string(),
        produced_at: "2026-04-30T12:00:00Z".to_string(),
        commit: "ghi789".to_string(),
        findings: vec![],
    };

    git_notes::write_notes(repo_path, "ghi789", payload)?;

    // Read back and verify
    let read_payload = git_notes::read_notes(repo_path, "ghi789")?
        .expect("Should be able to read empty findings");
    assert_eq!(read_payload.findings.len(), 0, "Should have no findings");

    Ok(())
}
