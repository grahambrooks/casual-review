//! Minimal Model Context Protocol (MCP) server over stdio.
//!
//! Exposes the review-comment operations as MCP tools so an AI agent host
//! (Zed's Agent, Claude, Cursor, …) can read and write review comments stored
//! in the repo. Transport is the MCP stdio convention: newline-delimited
//! JSON-RPC 2.0 messages on stdin/stdout, UTF-8, no embedded newlines; stderr
//! is free for logging.
//!
//! The surface is intentionally tiny — `initialize`, `ping`, `tools/list`,
//! `tools/call` — and every tool delegates to `crate::review`, the same code
//! path the CLI uses.

use crate::git_comments;
use crate::review::{self, AddInput, ListInput};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

const PROTOCOL_VERSION: &str = "2025-06-18";

/// Run the stdio server loop until stdin reaches EOF.
pub fn serve(repo: &Path) -> anyhow::Result<()> {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                // No id to answer to; log and move on per the stdio contract.
                eprintln!("cr mcp: ignoring unparseable line: {e}");
                continue;
            }
        };

        // Presence of an `id` distinguishes a request (must answer) from a
        // notification (must not answer).
        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(Value::Null);

        let outcome = dispatch(repo, method, &params);
        let Some(id) = id else { continue };

        let response = match outcome {
            Outcome::Result(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Outcome::Error { code, message } => {
                json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
            }
        };
        send(&mut out, &response)?;
    }
    Ok(())
}

enum Outcome {
    Result(Value),
    Error { code: i64, message: String },
}

fn dispatch(repo: &Path, method: &str, params: &Value) -> Outcome {
    match method {
        "initialize" => Outcome::Result(initialize_result(params)),
        "ping" => Outcome::Result(json!({})),
        "tools/list" => Outcome::Result(json!({ "tools": tool_defs() })),
        "tools/call" => call_tool(repo, params),
        _ => Outcome::Error {
            code: -32601,
            message: format!("method not found: {method}"),
        },
    }
}

fn initialize_result(params: &Value) -> Value {
    let version = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(PROTOCOL_VERSION);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": {
            "name": "casual-review",
            "version": env!("CARGO_PKG_VERSION"),
        },
    })
}

fn call_tool(repo: &Path, params: &Value) -> Outcome {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return Outcome::Error {
            code: -32602,
            message: "tools/call missing `name`".to_string(),
        };
    };
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let result: anyhow::Result<String> = match name {
        "list_comments" => tool_list(repo, &args),
        "add_comment" => tool_add(repo, &args),
        "reply_comment" => tool_reply(repo, &args),
        "resolve_comment" => tool_resolve(repo, &args),
        "reanchor_comment" => tool_reanchor(repo, &args),
        "sync_comments" => tool_sync(repo, &args),
        other => {
            return Outcome::Error {
                code: -32602,
                message: format!("unknown tool: {other}"),
            }
        }
    };

    // Tool *execution* failures are returned as a successful result with
    // `isError: true`, so the agent sees and can react to the message rather
    // than the host swallowing a protocol error.
    match result {
        Ok(text) => Outcome::Result(json!({
            "content": [{ "type": "text", "text": text }],
        })),
        Err(e) => Outcome::Result(json!({
            "content": [{ "type": "text", "text": format!("Error: {e:#}") }],
            "isError": true,
        })),
    }
}

fn tool_list(repo: &Path, args: &Value) -> anyhow::Result<String> {
    let payload = review::list(
        repo,
        ListInput {
            commit: commit_arg(args),
            file: opt_path(args, "file"),
            include_resolved: bool_arg(args, "include_resolved"),
            include_ancestors: bool_arg(args, "include_ancestors"),
        },
    )?;
    payload.to_json()
}

fn tool_add(repo: &Path, args: &Value) -> anyhow::Result<String> {
    let comment = review::add(
        repo,
        AddInput {
            file: opt_path(args, "file"),
            lines: opt_str(args, "lines"),
            file_level: bool_arg(args, "file_level"),
            commit_level: bool_arg(args, "commit_level"),
            body: req_str(args, "body")?,
            commit: commit_arg(args),
        },
    )?;
    Ok(serde_json::to_string_pretty(&comment)?)
}

fn tool_reply(repo: &Path, args: &Value) -> anyhow::Result<String> {
    let record = review::reply(
        repo,
        &req_str(args, "comment_id")?,
        &req_str(args, "body")?,
        &commit_arg(args),
    )?;
    Ok(serde_json::to_string_pretty(&record)?)
}

fn tool_resolve(repo: &Path, args: &Value) -> anyhow::Result<String> {
    let record = review::resolve(
        repo,
        &req_str(args, "comment_id")?,
        opt_str(args, "message").as_deref(),
        &commit_arg(args),
    )?;
    Ok(serde_json::to_string_pretty(&record)?)
}

fn tool_reanchor(repo: &Path, args: &Value) -> anyhow::Result<String> {
    let record = review::reanchor(
        repo,
        &req_str(args, "comment_id")?,
        &req_str(args, "lines")?,
        &commit_arg(args),
    )?;
    Ok(serde_json::to_string_pretty(&record)?)
}

fn tool_sync(repo: &Path, args: &Value) -> anyhow::Result<String> {
    let remote = opt_str(args, "remote").unwrap_or_else(|| "origin".to_string());
    match opt_str(args, "mode").as_deref().unwrap_or("sync") {
        "fetch" => {
            git_comments::fetch(repo, &remote)?;
            Ok(format!("Fetched review comments from {remote}"))
        }
        "push" => {
            git_comments::push(repo, &remote)?;
            Ok(format!("Pushed review comments to {remote}"))
        }
        "sync" => {
            git_comments::fetch(repo, &remote)?;
            git_comments::push(repo, &remote)?;
            Ok(format!("Synced review comments with {remote}"))
        }
        other => Err(anyhow::anyhow!(
            "invalid mode `{other}` (expected fetch|push|sync)"
        )),
    }
}

fn opt_str(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_string)
}

fn opt_path(args: &Value, key: &str) -> Option<PathBuf> {
    opt_str(args, key).map(PathBuf::from)
}

fn bool_arg(args: &Value, key: &str) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn commit_arg(args: &Value) -> String {
    opt_str(args, "commit").unwrap_or_else(|| "HEAD".to_string())
}

fn req_str(args: &Value, key: &str) -> anyhow::Result<String> {
    opt_str(args, key)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required argument `{key}`"))
}

fn send(out: &mut impl Write, value: &Value) -> anyhow::Result<()> {
    // Compact serialization guarantees no embedded newlines, as the stdio
    // transport requires; one trailing newline frames the message.
    let s = serde_json::to_string(value)?;
    out.write_all(s.as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()?;
    Ok(())
}

fn tool_defs() -> Vec<Value> {
    let commit_prop = json!({
        "type": "string",
        "description": "Commit-ish the comments live on. Defaults to HEAD.",
    });
    vec![
        json!({
            "name": "list_comments",
            "description": "List open review comments on a commit (default HEAD), as a casual-review/comment/1 JSON payload. Use this first to read existing review state before adding feedback.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "commit": commit_prop,
                    "file": { "type": "string", "description": "Filter to a single repo-relative file path." },
                    "include_resolved": { "type": "boolean", "description": "Include resolved threads (default false)." },
                    "include_ancestors": { "type": "boolean", "description": "Project comments from ancestor commits onto the target (default false)." }
                }
            }
        }),
        json!({
            "name": "add_comment",
            "description": "Add a review comment. Anchor it to a line range (lines), the whole file (file_level), or the commit (commit_level). Prefer line-level with a concrete suggestion.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file": { "type": "string", "description": "Repo-relative file to anchor to. Required unless commit_level." },
                    "lines": { "type": "string", "description": "Line range like \"42\" or \"42:44\" (1-based, inclusive)." },
                    "file_level": { "type": "boolean", "description": "Anchor to the whole file instead of a line range." },
                    "commit_level": { "type": "boolean", "description": "Anchor to the commit with no file (omit file)." },
                    "body": { "type": "string", "description": "The comment text." },
                    "commit": commit_prop
                },
                "required": ["body"]
            }
        }),
        json!({
            "name": "reply_comment",
            "description": "Reply to an existing comment, inheriting its anchor. Use this to continue a thread rather than opening a duplicate.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "comment_id": { "type": "string", "description": "ID of the comment to reply to (e.g. CRC-…)." },
                    "body": { "type": "string", "description": "The reply text." },
                    "commit": commit_prop
                },
                "required": ["comment_id", "body"]
            }
        }),
        json!({
            "name": "resolve_comment",
            "description": "Mark a thread resolved by appending a resolution record (append-only; nothing is deleted).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "comment_id": { "type": "string", "description": "ID of the thread root (or any comment in it) to resolve." },
                    "message": { "type": "string", "description": "Optional resolution note." },
                    "commit": commit_prop
                },
                "required": ["comment_id"]
            }
        }),
        json!({
            "name": "reanchor_comment",
            "description": "Move a stale comment to a new line range when the code it pointed at has shifted.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "comment_id": { "type": "string", "description": "ID of the comment to re-anchor." },
                    "lines": { "type": "string", "description": "New line range like \"51:53\"." },
                    "commit": commit_prop
                },
                "required": ["comment_id", "lines"]
            }
        }),
        json!({
            "name": "sync_comments",
            "description": "Share comments with a git remote. mode=sync (default) does fetch then push; mode=fetch or mode=push does one direction.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "remote": { "type": "string", "description": "Git remote name. Defaults to origin." },
                    "mode": { "type": "string", "enum": ["sync", "fetch", "push"], "description": "Direction (default sync)." }
                }
            }
        }),
    ]
}
