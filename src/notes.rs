/// Git notes support for persisting findings across commits.
/// Findings are stored in refs/notes/casual-review as JSON.
use crate::diagnostic::Diagnostic;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Finding persisted in git notes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// Unique finding ID (hash-based, stable within a commit)
    pub id: String,

    /// Rule that produced this finding
    pub rule: String,

    /// Severity level
    pub severity: String,

    /// Location information
    pub location: Location,

    /// Main message
    pub message: String,

    /// Secondary labels (e.g., "related location")
    #[serde(default)]
    pub labels: Vec<Label>,

    /// Suggested fixes (future use)
    #[serde(default)]
    pub suggestions: Vec<String>,

    /// Parent finding ID for threading discussions (future use)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

/// Physical location in source code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    /// File path relative to repo root
    pub file: PathBuf,

    /// Byte range in source (start, end)
    pub byte_range: (usize, usize),

    /// Line range (1-based)
    pub line_range: (u32, u32),

    /// Column range (1-based)
    pub col_range: (u32, u32),
}

/// Secondary location with a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Label {
    pub message: String,
    pub location: Location,
}

/// JSON payload written to git notes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotesPayload {
    /// Schema version (e.g., "casual-review/finding/1")
    pub schema: String,

    /// Tool name
    pub tool: String,

    /// Tool version
    pub tool_version: String,

    /// When this payload was produced (ISO 8601)
    pub produced_at: String,

    /// Commit these findings were produced from
    pub commit: String,

    /// Findings in this note
    pub findings: Vec<Finding>,
}

impl NotesPayload {
    /// Create a new payload for the given commit and diagnostics.
    pub fn new(commit: String, diagnostics: Vec<Diagnostic>) -> Self {
        let findings = diagnostics
            .into_iter()
            .map(|d| Finding::from_diagnostic(&d))
            .collect();

        Self {
            schema: "casual-review/finding/1".to_string(),
            tool: "casual-review".to_string(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            produced_at: timestamp_now(),
            commit,
            findings,
        }
    }

    /// Serialize to JSON string for storage in git notes.
    pub fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Deserialize from JSON string stored in git notes.
    pub fn from_json(json: &str) -> anyhow::Result<Self> {
        Ok(serde_json::from_str(json)?)
    }
}

impl Finding {
    /// Create a Finding from a Diagnostic.
    fn from_diagnostic(diag: &Diagnostic) -> Self {
        let id = format!("CR-{}", finding_hash(diag));

        Self {
            id,
            rule: diag.code.clone(),
            severity: format!("{:?}", diag.severity).to_lowercase(),
            location: Location {
                file: diag.primary.file.clone(),
                byte_range: (diag.primary.byte_range.start, diag.primary.byte_range.end),
                line_range: (diag.primary.line_start, diag.primary.line_end),
                col_range: (diag.primary.col_start, diag.primary.col_end),
            },
            message: diag.message.clone(),
            labels: diag
                .labels
                .iter()
                .map(|l| Label {
                    message: l.message.clone(),
                    location: Location {
                        file: l.span.file.clone(),
                        byte_range: (l.span.byte_range.start, l.span.byte_range.end),
                        line_range: (l.span.line_start, l.span.line_end),
                        col_range: (l.span.col_start, l.span.col_end),
                    },
                })
                .collect(),
            suggestions: vec![], // TODO: convert Suggestion to string representation
            parent: None,
        }
    }
}

/// Generate a stable hash for a finding based on its content.
fn finding_hash(diag: &Diagnostic) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    diag.code.hash(&mut hasher);
    diag.message.hash(&mut hasher);
    diag.primary.file.hash(&mut hasher);
    diag.primary.byte_range.start.hash(&mut hasher);
    let hash = hasher.finish();
    format!("{:x}", hash).chars().take(8).collect()
}

/// Get current timestamp in ISO 8601 format.
fn timestamp_now() -> String {
    let now = chrono::Utc::now();
    now.to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payload_roundtrip() {
        let payload = NotesPayload {
            schema: "casual-review/finding/1".to_string(),
            tool: "casual-review".to_string(),
            tool_version: "2026.4.29".to_string(),
            produced_at: "2026-04-29T20:00:00Z".to_string(),
            commit: "abc123".to_string(),
            findings: vec![],
        };

        let json = payload.to_json().expect("serialize");
        let restored = NotesPayload::from_json(&json).expect("deserialize");
        assert_eq!(restored.schema, "casual-review/finding/1");
    }
}
