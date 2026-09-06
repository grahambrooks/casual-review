use anyhow::Context;
use casual_review::cli::{
    Cli, Command, CommentAddArgs, CommentCmd, CommentListArgs, CommentReanchorArgs,
    CommentReplyArgs, CommentResolveArgs, CommentSubcommand, FetchArgs, FormatArg, McpArgs,
    PushArgs, SyncArgs,
};
use casual_review::comments::{Anchor, Comment};
use casual_review::review::{self, AddInput, ListInput};
use casual_review::{git_comments, mcp};
use clap::Parser;
use std::path::Path;
use std::process::{Command as ProcCommand, ExitCode};

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let result = match cli.command {
        Command::Comment(cmd) => run_comment(cmd),
        Command::Fetch(args) => run_fetch(args),
        Command::Push(args) => run_push(args),
        Command::Sync(args) => run_sync(args),
        Command::Mcp(args) => run_mcp(args),
    };
    run_or_fail(result)
}

fn run_or_fail(result: anyhow::Result<()>) -> ExitCode {
    match result {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run_fetch(args: FetchArgs) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("getting current directory")?;
    git_comments::fetch(&cwd, &args.remote)?;
    eprintln!(
        "Fetched review comments from {} (refs/notes/casual-review/discuss)",
        args.remote
    );
    Ok(())
}

fn run_push(args: PushArgs) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("getting current directory")?;
    git_comments::push(&cwd, &args.remote)?;
    eprintln!(
        "Pushed review comments to {} (refs/notes/casual-review/discuss)",
        args.remote
    );
    Ok(())
}

fn run_sync(args: SyncArgs) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("getting current directory")?;
    git_comments::fetch(&cwd, &args.remote)?;
    git_comments::push(&cwd, &args.remote)?;
    eprintln!(
        "Synced review comments with {} (refs/notes/casual-review/discuss)",
        args.remote
    );
    Ok(())
}

fn run_mcp(_args: McpArgs) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("getting current directory")?;
    mcp::serve(&cwd)
}

fn run_comment(cmd: CommentCmd) -> anyhow::Result<()> {
    match cmd.subcommand {
        CommentSubcommand::Add(args) => run_comment_add(args),
        CommentSubcommand::List(args) => run_comment_list(args),
        CommentSubcommand::Reply(args) => run_comment_reply(args),
        CommentSubcommand::Resolve(args) => run_comment_resolve(args),
        CommentSubcommand::Reanchor(args) => run_comment_reanchor(args),
    }
}

fn run_comment_add(args: CommentAddArgs) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("getting current directory")?;
    let body = read_body_or_editor(args.body.as_deref(), "comment")?;
    let comment = review::add(
        &cwd,
        AddInput {
            file: args.file,
            lines: args.lines,
            file_level: args.file_level,
            commit_level: args.commit_level,
            body,
            commit: args.commit.clone(),
        },
    )?;
    println!("{}", comment.id);
    eprintln!("Added comment {} on {}", comment.id, args.commit);
    Ok(())
}

fn run_comment_list(args: CommentListArgs) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("getting current directory")?;
    let payload = review::list(
        &cwd,
        ListInput {
            commit: args.commit.clone(),
            file: args.file,
            include_resolved: args.include_resolved,
            include_ancestors: args.include_ancestors,
        },
    )?;

    match args.format {
        FormatArg::Json => println!("{}", payload.to_json()?),
        FormatArg::Human => print_comments_human(&cwd, &payload.commit, &payload.comments)?,
    }
    Ok(())
}

fn run_comment_reply(args: CommentReplyArgs) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("getting current directory")?;
    let body = read_body_or_editor(args.body.as_deref(), "reply")?;
    let record = review::reply(&cwd, &args.comment_id, &body, &args.commit)?;
    println!("{}", record.id);
    eprintln!("Replied to {} with {}", args.comment_id, record.id);
    Ok(())
}

fn run_comment_resolve(args: CommentResolveArgs) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("getting current directory")?;
    let record = review::resolve(
        &cwd,
        &args.comment_id,
        args.message.as_deref(),
        &args.commit,
    )?;
    println!("{}", record.id);
    eprintln!("Resolved {} with {}", args.comment_id, record.id);
    Ok(())
}

fn run_comment_reanchor(args: CommentReanchorArgs) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("getting current directory")?;
    let record = review::reanchor(&cwd, &args.comment_id, &args.lines, &args.commit)?;
    println!("{}", record.id);
    Ok(())
}

fn print_comments_human(repo: &Path, target_sha: &str, comments: &[Comment]) -> anyhow::Result<()> {
    if comments.is_empty() {
        eprintln!("(no comments)");
        return Ok(());
    }
    for c in comments {
        let stale = review::is_stale(repo, &c.anchor)?;
        let stale_marker = if stale { " [stale]" } else { "" };
        let origin_marker = match c.origin_commit.as_deref() {
            Some(o) if o != target_sha => format!(" [from {}]", &o[..o.len().min(8)]),
            _ => String::new(),
        };
        let anchor_str = anchor_label(&c.anchor);
        let parent_str = match &c.parent {
            Some(p) => format!(" (reply to {p})"),
            None => String::new(),
        };
        let resolved_str = if c.resolved { " [resolved]" } else { "" };
        println!(
            "{} {} {}{}{}{}{}",
            c.id, c.author.name, anchor_str, parent_str, resolved_str, stale_marker, origin_marker
        );
        println!("  {} <{}>", c.created_at, c.author.email);
        for line in c.body.lines() {
            println!("  | {line}");
        }
        println!();
    }
    Ok(())
}

fn anchor_label(anchor: &Anchor) -> String {
    match &anchor.file {
        None => "<commit>".to_string(),
        Some(f) if anchor.line_range == (0, 0) => format!("{}", f.display()),
        Some(f) => format!(
            "{}:{}-{}",
            f.display(),
            anchor.line_range.0,
            anchor.line_range.1
        ),
    }
}

/// Resolve a comment body. If `body` is non-empty, return it verbatim.
/// Otherwise open `$EDITOR` (or `vi`) on a tempfile, strip lines starting
/// with `#`, and return the trimmed result. Errors on empty body so we never
/// store empty comments.
fn read_body_or_editor(body: Option<&str>, kind: &str) -> anyhow::Result<String> {
    if let Some(b) = body {
        if !b.trim().is_empty() {
            return Ok(b.to_string());
        }
    }

    let editor = std::env::var("EDITOR")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "vi".to_string());

    let stamp = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let path = std::env::temp_dir().join(format!("cr-{kind}-{stamp}.txt"));
    let header = format!(
        "\n\
         # Enter your {kind} above. Lines starting with `#` are ignored.\n\
         # Save and exit to submit; abort or leave blank to cancel.\n"
    );
    std::fs::write(&path, &header)?;

    // `sh -c` so $EDITOR can be `vim -c "set ft=markdown"` etc.
    let status = ProcCommand::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\"", editor = editor))
        .arg("--")
        .arg(&path)
        .status()
        .with_context(|| format!("launching editor {editor:?}"))?;

    let content = std::fs::read_to_string(&path)?;
    let _ = std::fs::remove_file(&path);

    if !status.success() {
        return Err(anyhow::anyhow!(
            "editor {editor:?} exited with {status}; aborting {kind}"
        ));
    }

    let body: String = content
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    if body.is_empty() {
        return Err(anyhow::anyhow!("aborting {kind}: empty body"));
    }
    Ok(body)
}
