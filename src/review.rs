//! Core review-comment operations, decoupled from any front end.
//!
//! Both the CLI (`src/main.rs`) and the MCP server (`src/mcp.rs`) call into
//! these functions. They take a repo path and structured inputs and return
//! the created/visible `Comment` records — no printing, no process exit codes.

use crate::comments::{author_from_git, comment_id, sha256_hex, Anchor, Comment, CommentsPayload};
use crate::git_comments;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command as ProcCommand;

/// Inputs for adding a top-level comment. Exactly one of a line range,
/// `file_level`, or `commit_level` selects the anchor kind.
pub struct AddInput {
    pub file: Option<PathBuf>,
    pub lines: Option<String>,
    pub file_level: bool,
    pub commit_level: bool,
    pub body: String,
    /// Commit-ish to attach to; "HEAD" is the usual default.
    pub commit: String,
}

/// Inputs for listing comments on a commit.
pub struct ListInput {
    pub commit: String,
    pub file: Option<PathBuf>,
    pub include_resolved: bool,
    pub include_ancestors: bool,
}

/// Add a top-level comment. Returns the created record (with its id).
pub fn add(repo: &Path, input: AddInput) -> anyhow::Result<Comment> {
    if input.body.trim().is_empty() {
        return Err(anyhow::anyhow!("comment body is empty"));
    }
    let author = author_from_git(repo)?;
    let commit_sha = resolve_commit(repo, &input.commit)?;
    let anchor = build_anchor(
        repo,
        &commit_sha,
        input.file,
        input.lines,
        input.file_level,
        input.commit_level,
    )?;

    let created_at = chrono::Utc::now().to_rfc3339();
    let id = comment_id(&author, &created_at, &anchor, &input.body);
    let comment = Comment {
        id,
        author,
        created_at,
        anchor,
        body: input.body,
        parent: None,
        resolved: false,
        origin_commit: None,
    };

    let mut payload = git_comments::read_comments(repo, &commit_sha)?
        .unwrap_or_else(|| CommentsPayload::new(commit_sha.clone()));
    payload.comments.push(comment.clone());
    git_comments::write_comments(repo, &commit_sha, &payload)?;
    Ok(comment)
}

/// Reply to an existing comment, inheriting the parent's anchor.
pub fn reply(
    repo: &Path,
    comment_id_arg: &str,
    body: &str,
    commit: &str,
) -> anyhow::Result<Comment> {
    if body.trim().is_empty() {
        return Err(anyhow::anyhow!("reply body is empty"));
    }
    let author = author_from_git(repo)?;
    let commit_sha = resolve_commit(repo, commit)?;
    let mut payload = git_comments::read_comments(repo, &commit_sha)?
        .ok_or_else(|| anyhow::anyhow!("no comments on commit {commit}"))?;
    let parent = find_comment(&payload, comment_id_arg, commit)?.clone();

    let created_at = chrono::Utc::now().to_rfc3339();
    let anchor = parent.anchor.clone();
    let id = comment_id(&author, &created_at, &anchor, body);
    let record = Comment {
        id,
        author,
        created_at,
        anchor,
        body: body.to_string(),
        parent: Some(parent.id),
        resolved: false,
        origin_commit: None,
    };
    payload.comments.push(record.clone());
    git_comments::write_comments(repo, &commit_sha, &payload)?;
    Ok(record)
}

/// Mark a thread resolved by appending a resolution record.
pub fn resolve(
    repo: &Path,
    comment_id_arg: &str,
    message: Option<&str>,
    commit: &str,
) -> anyhow::Result<Comment> {
    let author = author_from_git(repo)?;
    let commit_sha = resolve_commit(repo, commit)?;
    let mut payload = git_comments::read_comments(repo, &commit_sha)?
        .ok_or_else(|| anyhow::anyhow!("no comments on commit {commit}"))?;
    let target = find_comment(&payload, comment_id_arg, commit)?.clone();

    let created_at = chrono::Utc::now().to_rfc3339();
    let anchor = target.anchor.clone();
    let body = message.unwrap_or_default().to_string();
    let id = comment_id(&author, &created_at, &anchor, &body);
    let record = Comment {
        id,
        author,
        created_at,
        anchor,
        body,
        parent: Some(target.id),
        resolved: true,
        origin_commit: None,
    };
    payload.comments.push(record.clone());
    git_comments::write_comments(repo, &commit_sha, &payload)?;
    Ok(record)
}

/// Re-anchor a stale comment to a new line range (append-only record).
pub fn reanchor(
    repo: &Path,
    comment_id_arg: &str,
    lines: &str,
    commit: &str,
) -> anyhow::Result<Comment> {
    let author = author_from_git(repo)?;
    let commit_sha = resolve_commit(repo, commit)?;
    let mut payload = git_comments::read_comments(repo, &commit_sha)?
        .ok_or_else(|| anyhow::anyhow!("no comments on commit {commit}"))?;
    let target = find_comment(&payload, comment_id_arg, commit)?.clone();

    let file = target
        .anchor
        .file
        .clone()
        .ok_or_else(|| anyhow::anyhow!("cannot reanchor a commit-level comment"))?;
    let content = read_file_at_commit(repo, &commit_sha, &file)?;
    let (line_start, line_end) = parse_line_range(lines)?;
    let (start, end) = line_range_to_byte_range(&content, line_start, line_end)?;
    let new_anchor = Anchor {
        file: Some(file),
        line_range: (line_start, line_end),
        byte_range: (start, end),
        anchor_text_sha: sha256_hex(&content[start..end]),
    };

    let created_at = chrono::Utc::now().to_rfc3339();
    let body = format!("reanchored {comment_id_arg} → lines {line_start}:{line_end}");
    let id = comment_id(&author, &created_at, &new_anchor, &body);
    let record = Comment {
        id,
        author,
        created_at,
        anchor: new_anchor,
        body,
        parent: Some(target.id),
        resolved: false,
        origin_commit: None,
    };
    payload.comments.push(record.clone());
    git_comments::write_comments(repo, &commit_sha, &payload)?;
    Ok(record)
}

/// List visible comments on a commit, applying file filtering, resolved-thread
/// hiding, and optional ancestor projection. Returns a `CommentsPayload` whose
/// `comments` are exactly what callers should display or serialize.
pub fn list(repo: &Path, input: ListInput) -> anyhow::Result<CommentsPayload> {
    let commit_sha = resolve_commit(repo, &input.commit)?;
    let mut payload = git_comments::read_comments(repo, &commit_sha)?
        .unwrap_or_else(|| CommentsPayload::new(commit_sha.clone()));

    if input.include_ancestors {
        for sha in list_commented_commits(repo) {
            if sha == commit_sha || !is_ancestor(repo, &sha, &commit_sha) {
                continue;
            }
            if let Some(ancestor) = git_comments::read_comments(repo, &sha)? {
                for mut c in ancestor.comments {
                    c.origin_commit = Some(sha.clone());
                    payload.comments.push(c);
                }
            }
        }
    }

    let resolved_roots: HashSet<String> = payload
        .comments
        .iter()
        .filter(|c| c.resolved)
        .filter_map(|c| c.parent.clone())
        .collect();

    let visible: Vec<Comment> = payload
        .comments
        .iter()
        .filter(|c| {
            if let Some(file_filter) = &input.file {
                if c.anchor.file.as_deref() != Some(file_filter.as_path()) {
                    return false;
                }
            }
            if !input.include_resolved {
                let root = c.parent.clone().unwrap_or_else(|| c.id.clone());
                if resolved_roots.contains(&root) {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect();

    Ok(CommentsPayload {
        schema: payload.schema,
        tool: payload.tool,
        tool_version: payload.tool_version,
        commit: commit_sha,
        comments: visible,
    })
}

fn find_comment<'a>(
    payload: &'a CommentsPayload,
    id: &str,
    commit: &str,
) -> anyhow::Result<&'a Comment> {
    payload
        .comments
        .iter()
        .find(|c| c.id == id)
        .ok_or_else(|| anyhow::anyhow!("comment {id} not found on commit {commit}"))
}

fn build_anchor(
    repo: &Path,
    commit_sha: &str,
    file: Option<PathBuf>,
    lines: Option<String>,
    file_level: bool,
    commit_level: bool,
) -> anyhow::Result<Anchor> {
    if commit_level {
        return Ok(Anchor {
            file: None,
            line_range: (0, 0),
            byte_range: (0, 0),
            anchor_text_sha: String::new(),
        });
    }

    let file =
        file.ok_or_else(|| anyhow::anyhow!("file is required unless commit_level is set"))?;
    let file = repo_relative(repo, &file)?;
    let content = read_file_at_commit(repo, commit_sha, &file)?;

    if file_level {
        return Ok(Anchor {
            file: Some(file),
            line_range: (0, 0),
            byte_range: (0, content.len()),
            anchor_text_sha: sha256_hex(&content),
        });
    }

    let lines = lines
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("lines is required (or set file_level)"))?;
    let (line_start, line_end) = parse_line_range(lines)?;
    let (start, end) = line_range_to_byte_range(&content, line_start, line_end)?;
    Ok(Anchor {
        file: Some(file),
        line_range: (line_start, line_end),
        byte_range: (start, end),
        anchor_text_sha: sha256_hex(&content[start..end]),
    })
}

/// All commits on which any comment has been written, regardless of ancestry.
/// Returns empty when the discuss ref does not yet exist.
pub fn list_commented_commits(repo: &Path) -> Vec<String> {
    let output = ProcCommand::new("git")
        .current_dir(repo)
        .args(["notes", "--ref", "casual-review/discuss", "list"])
        .output();
    let Ok(output) = output else {
        return vec![];
    };
    if !output.status.success() {
        return vec![];
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1).map(String::from))
        .collect()
}

pub fn is_ancestor(repo: &Path, ancestor_sha: &str, descendant_sha: &str) -> bool {
    if ancestor_sha == descendant_sha {
        return true;
    }
    ProcCommand::new("git")
        .current_dir(repo)
        .args(["merge-base", "--is-ancestor", ancestor_sha, descendant_sha])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn resolve_commit(repo: &Path, commit: &str) -> anyhow::Result<String> {
    let output = ProcCommand::new("git")
        .current_dir(repo)
        .args(["rev-parse", "--verify", commit])
        .output()
        .map_err(|e| anyhow::anyhow!("git rev-parse failed: {e}"))?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "could not resolve commit {commit}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn repo_root(repo: &Path) -> anyhow::Result<PathBuf> {
    let output = ProcCommand::new("git")
        .current_dir(repo)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|e| anyhow::anyhow!("git rev-parse --show-toplevel failed: {e}"))?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "not in a git repo: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim(),
    ))
}

fn repo_relative(repo: &Path, path: &Path) -> anyhow::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo.join(path)
    };
    let canonical = absolute
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("resolving path {}: {e}", path.display()))?;
    let root = repo_root(repo)?
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("canonicalizing repo root: {e}"))?;
    canonical
        .strip_prefix(&root)
        .map(|p| p.to_path_buf())
        .map_err(|_| {
            anyhow::anyhow!(
                "path {} is outside the repo {}",
                path.display(),
                root.display()
            )
        })
}

fn read_file_at_commit(repo: &Path, commit: &str, file: &Path) -> anyhow::Result<Vec<u8>> {
    let spec = format!("{commit}:{}", file.display());
    let output = ProcCommand::new("git")
        .current_dir(repo)
        .args(["show", &spec])
        .output()
        .map_err(|e| anyhow::anyhow!("git show {spec}: {e}"))?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "git show {spec} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

pub fn parse_line_range(s: &str) -> anyhow::Result<(u32, u32)> {
    let s = s.trim();
    if let Some((a, b)) = s.split_once(':') {
        let a: u32 = a.parse().map_err(|_| {
            anyhow::anyhow!("invalid line range `{s}`: expected NUMBER or START:END")
        })?;
        let b: u32 = b.parse().map_err(|_| {
            anyhow::anyhow!("invalid line range `{s}`: expected NUMBER or START:END")
        })?;
        if a == 0 || b < a {
            return Err(anyhow::anyhow!(
                "invalid line range `{s}`: lines are 1-based and START must be ≤ END"
            ));
        }
        Ok((a, b))
    } else {
        let n: u32 = s.parse().map_err(|_| {
            anyhow::anyhow!("invalid line range `{s}`: expected NUMBER or START:END")
        })?;
        if n == 0 {
            return Err(anyhow::anyhow!("line numbers are 1-based"));
        }
        Ok((n, n))
    }
}

fn line_range_to_byte_range(
    content: &[u8],
    line_start: u32,
    line_end: u32,
) -> anyhow::Result<(usize, usize)> {
    let mut byte = 0usize;
    let mut line: u32 = 1;
    let mut start_byte: Option<usize> = None;
    let mut end_byte: Option<usize> = None;

    if line_start == 1 {
        start_byte = Some(0);
    }

    while byte < content.len() {
        if content[byte] == b'\n' {
            if line + 1 == line_start && start_byte.is_none() {
                start_byte = Some(byte + 1);
            }
            if line == line_end {
                end_byte = Some(byte);
                break;
            }
            line += 1;
        }
        byte += 1;
    }

    let start =
        start_byte.ok_or_else(|| anyhow::anyhow!("line {line_start} is past end of file"))?;
    let end = end_byte.unwrap_or(content.len());
    if start > end {
        return Err(anyhow::anyhow!(
            "line {line_end} is past end of file (file has {line} line(s))"
        ));
    }
    Ok((start, end))
}

/// Whether the code under an anchor has drifted from when the comment was made.
pub fn is_stale(repo: &Path, anchor: &Anchor) -> anyhow::Result<bool> {
    let Some(file) = &anchor.file else {
        return Ok(false);
    };
    if anchor.anchor_text_sha.is_empty() {
        return Ok(false);
    }
    let abs = if file.is_absolute() {
        file.clone()
    } else {
        repo_root(repo)
            .unwrap_or_else(|_| repo.to_path_buf())
            .join(file)
    };
    let Ok(bytes) = std::fs::read(&abs) else {
        return Ok(true);
    };
    let cur = if anchor.line_range == (0, 0) {
        sha256_hex(&bytes)
    } else if anchor.byte_range.1 <= bytes.len() {
        sha256_hex(&bytes[anchor.byte_range.0..anchor.byte_range.1])
    } else {
        return Ok(true);
    };
    Ok(cur != anchor.anchor_text_sha)
}
