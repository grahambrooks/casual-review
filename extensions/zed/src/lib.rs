//! Casual Review Zed extension.
//!
//! Zed extensions run in a WASM sandbox without arbitrary subprocess access,
//! so this extension cannot spawn `cr` directly. Instead, the slash commands
//! emit ready-to-run shell invocations that the user (or Zed's Assistant)
//! can copy into a terminal — Zed's Assistant in particular composes well
//! with this pattern, since it can ask "draft a comment for this code" and
//! then suggest the exact `cr comment add ...` command.
//!
//! The one read-side capability is `/cr-status`, which dumps the contents
//! of `.cr-comments/*.json` when `cr` is in file-fallback mode (non-git
//! repos, tests). For git-notes-backed storage, run `cr comment list`.

use zed_extension_api as zed;
use zed_extension_api::{
    Range, Result, SlashCommand, SlashCommandOutput, SlashCommandOutputSection, Worktree,
};

struct CasualReview;

impl zed::Extension for CasualReview {
    fn new() -> Self {
        Self
    }

    fn run_slash_command(
        &self,
        command: SlashCommand,
        args: Vec<String>,
        worktree: Option<&Worktree>,
    ) -> Result<SlashCommandOutput, String> {
        match command.name.as_str() {
            "cr-help" => Ok(section("cr command reference", help_text())),
            "cr-list" => Ok(section("cr comment list", list_text())),
            "cr-add" => Ok(section("cr comment add", add_text(&args))),
            "cr-reply" => Ok(section("cr comment reply", reply_text(&args))),
            "cr-resolve" => Ok(section("cr comment resolve", resolve_text(&args))),
            "cr-sync" => Ok(section("cr fetch + push", sync_text())),
            "cr-status" => status_text(worktree),
            other => Err(format!("unknown command: {other}")),
        }
    }
}

zed::register_extension!(CasualReview);

fn section(label: &str, body: String) -> SlashCommandOutput {
    let len = body.len() as u32;
    SlashCommandOutput {
        text: body,
        sections: vec![SlashCommandOutputSection {
            range: Range { start: 0, end: len },
            label: label.into(),
        }],
    }
}

fn help_text() -> String {
    r#"casual-review (`cr`) — comment subcommands

  cr comment add <FILE> --lines <START[:END]> -m "<body>"
      Anchor a new comment to a line range in <FILE>.
      Use --file-level instead of --lines to comment on the file.
      Use --commit-level (and omit FILE) to comment on the commit.

  cr comment list [--include-ancestors] [--file <FILE>]
                  [--include-resolved] [--format human|json]
      List comments on HEAD (or another commit via --commit).

  cr comment reply <COMMENT_ID> -m "<body>"
      Reply to a comment. Inherits the parent's anchor.

  cr comment resolve <COMMENT_ID> [-m "<message>"]
      Mark a thread resolved (append-only audit trail).

  cr comment reanchor <COMMENT_ID> --lines <START[:END]>
      Re-anchor a stale comment to a new line range.

  cr fetch <REMOTE>   # cr push <REMOTE>
      Sync findings + comments via refs/notes/casual-review/*.

Notes are stored in `refs/notes/casual-review/discuss` (schema:
`casual-review/comment/1`). The Zed extension only formats commands;
run them in a terminal to apply them.
"#
        .to_string()
}

fn list_text() -> String {
    "Run in your terminal:\n\n```sh\ncr comment list --include-ancestors\n```\n\n\
     Add `--file <PATH>` to filter to a single file, `--format json` for the\n\
     editor protocol, or `--include-resolved` to surface resolved threads.\n"
        .to_string()
}

fn add_text(args: &[String]) -> String {
    // Expected: `<file>:<lines> body…`  OR  `<file> <lines> body…`.
    let (file, lines, body) = parse_add_args(args);

    let body_arg = body.trim();
    let body_quoted = if body_arg.is_empty() {
        "\"<body>\"".to_string()
    } else {
        shell_quote(body_arg)
    };

    format!(
        "Run in your terminal:\n\n\
         ```sh\n\
         cr comment add {file} --lines {lines} -m {body_quoted}\n\
         ```\n",
        file = if file.is_empty() { "<FILE>".to_string() } else { file },
        lines = if lines.is_empty() { "<START[:END]>".to_string() } else { lines },
    )
}

fn reply_text(args: &[String]) -> String {
    let id = args.first().cloned().unwrap_or_else(|| "<COMMENT_ID>".to_string());
    let body = args.get(1..).map(|s| s.join(" ")).unwrap_or_default();
    let body_quoted = if body.trim().is_empty() {
        "\"<body>\"".to_string()
    } else {
        shell_quote(body.trim())
    };
    format!(
        "Run in your terminal:\n\n\
         ```sh\n\
         cr comment reply {id} -m {body_quoted}\n\
         ```\n\n\
         If the parent comment was projected from an ancestor commit, also pass\n\
         `--commit <ANCESTOR_SHA>` so the reply lands on the right note.\n",
    )
}

fn resolve_text(args: &[String]) -> String {
    let id = args.first().cloned().unwrap_or_else(|| "<COMMENT_ID>".to_string());
    let message = args.get(1..).map(|s| s.join(" ")).unwrap_or_default();
    let message_arg = if message.trim().is_empty() {
        String::new()
    } else {
        format!(" -m {}", shell_quote(message.trim()))
    };
    format!(
        "Run in your terminal:\n\n\
         ```sh\n\
         cr comment resolve {id}{message_arg}\n\
         ```\n",
    )
}

fn sync_text() -> String {
    "Run in your terminal:\n\n\
     ```sh\n\
     cr fetch origin\n\
     cr push origin\n\
     ```\n\n\
     Replace `origin` if you sync against a different remote.\n"
        .to_string()
}

fn status_text(worktree: Option<&Worktree>) -> Result<SlashCommandOutput, String> {
    let worktree = worktree.ok_or_else(|| "no worktree open".to_string())?;

    // File-fallback storage: `.cr-comments/<commit-or-ref>.json`. We can't
    // list directories from the WASM sandbox, so we try a small set of
    // likely candidates. Git-notes mode (the normal case) lives in
    // `.git/refs/notes/casual-review/discuss` and is unreadable from here.
    let candidates = ["HEAD", "abc", "main", "master"];
    let mut found = Vec::new();

    for c in &candidates {
        let path = format!(".cr-comments/{c}.json");
        if let Ok(contents) = worktree.read_text_file(&path) {
            found.push((path, contents));
        }
    }

    if found.is_empty() {
        let body = "No `.cr-comments/*.json` fallback files visible from the worktree.\n\n\
                    For git-notes-backed storage (the default when in a git repo), the\n\
                    extension cannot read the comments directly — Zed's WASM sandbox\n\
                    does not expose subprocess execution. Run `cr comment list` in a\n\
                    terminal to see them.\n"
            .to_string();
        return Ok(section("cr-status (none)", body));
    }

    let mut body = String::new();
    body.push_str("Visible fallback notes (file-storage mode):\n\n");
    for (path, contents) in &found {
        body.push_str(&format!("### {path}\n\n```json\n{contents}\n```\n\n"));
    }
    Ok(section("cr-status", body))
}

fn parse_add_args(args: &[String]) -> (String, String, String) {
    if args.is_empty() {
        return (String::new(), String::new(), String::new());
    }
    let head = &args[0];
    if let Some((file, lines)) = head.rsplit_once(':') {
        // `file:lines body…`
        let body = args.get(1..).map(|s| s.join(" ")).unwrap_or_default();
        return (file.to_string(), lines.to_string(), body);
    }
    // `file lines body…`
    let file = head.clone();
    let lines = args.get(1).cloned().unwrap_or_default();
    let body = args.get(2..).map(|s| s.join(" ")).unwrap_or_default();
    (file, lines, body)
}

/// Quote a body for safe shell use. Wraps in single quotes and escapes
/// embedded single quotes as `'\''` — the standard POSIX idiom.
fn shell_quote(s: &str) -> String {
    let escaped = s.replace('\'', "'\\''");
    format!("'{escaped}'")
}
