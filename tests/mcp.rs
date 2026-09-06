//! Integration tests for the `cr mcp` stdio MCP server.
//!
//! Drives the server end-to-end the way an MCP host would: write
//! newline-delimited JSON-RPC to stdin, close it, and parse the responses.

use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn init_repo(path: &Path) {
    for args in [
        vec!["init"],
        vec!["config", "user.email", "alice@test.com"],
        vec!["config", "user.name", "Alice"],
    ] {
        Command::new("git")
            .current_dir(path)
            .args(&args)
            .output()
            .unwrap();
    }
    std::fs::write(path.join("hello.rs"), "fn main() {\n    let x = 1;\n}\n").unwrap();
    for args in [vec!["add", "hello.rs"], vec!["commit", "-m", "init"]] {
        Command::new("git")
            .current_dir(path)
            .args(&args)
            .output()
            .unwrap();
    }
}

/// Pipe the messages to `cr mcp` and return one parsed `Value` per response
/// line. Closing stdin (by dropping the handle) signals EOF so the server
/// loop terminates.
fn drive(repo: &Path, messages: &[Value]) -> Vec<Value> {
    let mut input = String::new();
    for m in messages {
        input.push_str(&serde_json::to_string(m).unwrap());
        input.push('\n');
    }

    let mut child = Command::new(env!("CARGO_BIN_EXE_cr"))
        .current_dir(repo)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();

    let out = child.wait_with_output().unwrap();
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn handshake_list_tools_and_roundtrip_a_comment() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());

    let responses = drive(
        tmp.path(),
        &[
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{}}}),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"add_comment","arguments":{"file":"hello.rs","lines":"2:2","body":"magic number"}}}),
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"list_comments","arguments":{}}}),
        ],
    );

    // The notification produces no response: 5 messages -> 4 responses.
    assert_eq!(responses.len(), 4, "got: {responses:#?}");

    // initialize
    assert_eq!(responses[0]["id"], 1);
    assert_eq!(
        responses[0]["result"]["serverInfo"]["name"],
        "casual-review"
    );
    assert!(responses[0]["result"]["capabilities"]["tools"].is_object());

    // tools/list advertises the review tools
    let tool_names: Vec<String> = responses[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    for expected in [
        "list_comments",
        "add_comment",
        "reply_comment",
        "resolve_comment",
        "reanchor_comment",
        "sync_comments",
    ] {
        assert!(
            tool_names.contains(&expected.to_string()),
            "missing tool {expected}"
        );
    }

    // add_comment returns the created record as JSON text content
    let add_text = responses[2]["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    let created: Value = serde_json::from_str(add_text).unwrap();
    let new_id = created["id"].as_str().unwrap();
    assert!(new_id.starts_with("CRC-"), "unexpected id: {new_id}");
    assert_eq!(created["body"], "magic number");

    // list_comments now reports exactly that comment
    let list_text = responses[3]["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    let payload: Value = serde_json::from_str(list_text).unwrap();
    assert_eq!(payload["schema"], "casual-review/comment/1");
    let comments = payload["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0]["id"], new_id);
}

#[test]
fn unknown_method_and_tool_report_errors() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());

    let responses = drive(
        tmp.path(),
        &[
            json!({"jsonrpc":"2.0","id":1,"method":"does/not/exist"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"bogus_tool","arguments":{}}}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"add_comment","arguments":{}}}),
        ],
    );

    assert_eq!(responses.len(), 3);

    // Unknown JSON-RPC method -> protocol error -32601.
    assert_eq!(responses[0]["error"]["code"], -32601);

    // Unknown tool -> invalid params -32602.
    assert_eq!(responses[1]["error"]["code"], -32602);

    // Tool execution failure (missing required `body`) -> result with isError.
    assert_eq!(responses[2]["result"]["isError"], true);
    let text = responses[2]["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    assert!(
        text.contains("body"),
        "expected error to mention body: {text}"
    );
}
