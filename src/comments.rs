//! `casual-review/comment/1` schema for human-authored review comments.
//!
//! Comments are stored on `refs/notes/casual-review/discuss` (see
//! `git_comments`) and follow the same append-only model as findings: replies
//! and resolutions are new records linked via `parent`, never mutations of
//! prior records.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Comment author. Sourced from `git config user.{name,email}`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Author {
    pub name: String,
    pub email: String,
}

/// Where a comment is attached. `file = None` is a commit-level comment;
/// `line_range = (0, 0)` with `file` set is a file-level comment; otherwise
/// the comment anchors to the byte range within the file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Anchor {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<PathBuf>,
    pub line_range: (u32, u32),
    pub byte_range: (usize, usize),
    /// SHA-256 hex of the bytes at `byte_range` when the comment was created.
    /// Empty for commit-level comments. Used for staleness detection: a fresh
    /// hash that differs means the anchored code has changed.
    pub anchor_text_sha: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub author: Author,
    pub created_at: String,
    pub anchor: Anchor,
    pub body: String,
    /// ID of the parent comment for replies and resolutions. Top-level
    /// comments have `parent = None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// True on the resolution record itself. Aggregated state of a thread is
    /// derived by walking its records, not stored on the root.
    #[serde(default)]
    pub resolved: bool,
    /// Set during ancestor projection by `cr comment list --include-ancestors`
    /// to the commit the comment was originally written on. Never persisted
    /// to git notes (skipped on serialize when `None`, defaulted on read).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_commit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentsPayload {
    pub schema: String,
    pub tool: String,
    pub tool_version: String,
    pub commit: String,
    pub comments: Vec<Comment>,
}

impl CommentsPayload {
    pub fn new(commit: String) -> Self {
        Self {
            schema: "casual-review/comment/1".to_string(),
            tool: "casual-review".to_string(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            commit,
            comments: vec![],
        }
    }

    pub fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json(json: &str) -> anyhow::Result<Self> {
        Ok(serde_json::from_str(json)?)
    }
}

/// SHA-256 hex of `bytes`. Used both for `anchor_text_sha` and (truncated)
/// for comment IDs. Cross-machine stable.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// Stable comment ID: `CRC-<first-12-hex-chars-of-sha256>` over the fields
/// that uniquely identify the comment at creation time.
pub fn comment_id(author: &Author, created_at: &str, anchor: &Anchor, body: &str) -> String {
    let mut h = Sha256::new();
    h.update(author.email.as_bytes());
    h.update(b"\0");
    h.update(created_at.as_bytes());
    h.update(b"\0");
    if let Some(f) = &anchor.file {
        h.update(f.to_string_lossy().as_bytes());
    }
    h.update(b"\0");
    h.update(format!("{}-{}", anchor.line_range.0, anchor.line_range.1).as_bytes());
    h.update(b"\0");
    h.update(format!("{}-{}", anchor.byte_range.0, anchor.byte_range.1).as_bytes());
    h.update(b"\0");
    h.update(body.as_bytes());
    let hex = format!("{:x}", h.finalize());
    format!("CRC-{}", &hex[..12])
}

/// Read author identity from `git config user.name` / `user.email`. Falls
/// back to the global config; errors if neither is set so we never write
/// anonymous comments.
pub fn author_from_git(repo: &Path) -> anyhow::Result<Author> {
    let name = git_config(repo, "user.name")?;
    let email = git_config(repo, "user.email")?;
    if name.is_empty() || email.is_empty() {
        return Err(anyhow::anyhow!(
            "git config user.name/user.email is unset; set them before commenting"
        ));
    }
    Ok(Author { name, email })
}

fn git_config(repo: &Path, key: &str) -> anyhow::Result<String> {
    let output = Command::new("git")
        .current_dir(repo)
        .args(["config", "--get", key])
        .output()
        .map_err(|e| anyhow::anyhow!("git config {key} failed: {e}"))?;
    if !output.status.success() {
        return Ok(String::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_is_deterministic() {
        let author = Author {
            name: "Test".into(),
            email: "t@e.com".into(),
        };
        let anchor = Anchor {
            file: Some(PathBuf::from("a.rs")),
            line_range: (1, 1),
            byte_range: (0, 5),
            anchor_text_sha: sha256_hex(b"hello"),
        };
        let a = comment_id(&author, "2026-04-30T00:00:00Z", &anchor, "body");
        let b = comment_id(&author, "2026-04-30T00:00:00Z", &anchor, "body");
        assert_eq!(a, b);
        assert!(a.starts_with("CRC-"));
        assert_eq!(a.len(), 4 + 12);
    }

    #[test]
    fn id_differs_with_body() {
        let author = Author {
            name: "T".into(),
            email: "t@e.com".into(),
        };
        let anchor = Anchor {
            file: None,
            line_range: (0, 0),
            byte_range: (0, 0),
            anchor_text_sha: String::new(),
        };
        let a = comment_id(&author, "t", &anchor, "one");
        let b = comment_id(&author, "t", &anchor, "two");
        assert_ne!(a, b);
    }

    #[test]
    fn payload_roundtrips() {
        let p = CommentsPayload::new("abc".to_string());
        let json = p.to_json().unwrap();
        let back = CommentsPayload::from_json(&json).unwrap();
        assert_eq!(back.schema, "casual-review/comment/1");
        assert!(back.comments.is_empty());
    }
}
