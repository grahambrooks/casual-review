use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "cr",
    version,
    about = "Store code-review comments and conversations inside your git repository"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Manage review comments anchored to lines, files, or commits.
    Comment(CommentCmd),
    /// Fetch review comments from a remote repository.
    Fetch(FetchArgs),
    /// Push review comments to a remote repository.
    Push(PushArgs),
    /// Fetch then push review comments (sync with a remote).
    Sync(SyncArgs),
    /// Run an MCP server over stdio, exposing review tools to AI agents.
    Mcp(McpArgs),
}

#[derive(clap::Args, Debug)]
pub struct CommentCmd {
    #[command(subcommand)]
    pub subcommand: CommentSubcommand,
}

#[derive(Subcommand, Debug)]
pub enum CommentSubcommand {
    /// Add a new comment anchored to lines, a file, or a commit.
    Add(CommentAddArgs),
    /// List comments on a commit.
    List(CommentListArgs),
    /// Reply to an existing comment (inherits the parent's anchor).
    Reply(CommentReplyArgs),
    /// Mark a comment thread resolved.
    Resolve(CommentResolveArgs),
    /// Re-anchor a stale comment to a new line range.
    Reanchor(CommentReanchorArgs),
}

#[derive(clap::Args, Debug)]
pub struct CommentAddArgs {
    /// File to anchor the comment to. Omit with `--commit-level`.
    pub file: Option<PathBuf>,

    /// Line range, e.g. `42` or `42:44`. Omit for `--file-level` or `--commit-level`.
    #[arg(long, value_name = "RANGE", conflicts_with_all = ["file_level", "commit_level"])]
    pub lines: Option<String>,

    /// Comment on the file as a whole.
    #[arg(long, conflicts_with_all = ["lines", "commit_level"])]
    pub file_level: bool,

    /// Comment on the commit as a whole (no file anchor).
    #[arg(long, conflicts_with_all = ["lines", "file_level"])]
    pub commit_level: bool,

    /// Comment body. If omitted, opens $EDITOR.
    #[arg(long, short = 'm')]
    pub body: Option<String>,

    /// Commit to attach to (default: HEAD).
    #[arg(long, default_value = "HEAD")]
    pub commit: String,
}

#[derive(clap::Args, Debug)]
pub struct CommentListArgs {
    /// Commit to read comments from (default: HEAD).
    #[arg(long, default_value = "HEAD")]
    pub commit: String,

    /// Filter to a single file.
    #[arg(long)]
    pub file: Option<PathBuf>,

    /// Output format.
    #[arg(long, value_enum, default_value_t = FormatArg::Human)]
    pub format: FormatArg,

    /// Include resolved threads in output.
    #[arg(long)]
    pub include_resolved: bool,

    /// Project comments from ancestor commits onto the target. Each projected
    /// comment is tagged with its `origin_commit`; staleness is recomputed
    /// against the working tree.
    #[arg(long)]
    pub include_ancestors: bool,
}

#[derive(clap::Args, Debug)]
pub struct CommentReplyArgs {
    /// Comment ID to reply to.
    #[arg(value_name = "COMMENT_ID")]
    pub comment_id: String,

    /// Reply body. If omitted, opens $EDITOR.
    #[arg(long, short = 'm')]
    pub body: Option<String>,

    /// Commit the parent comment lives on (default: HEAD).
    #[arg(long, default_value = "HEAD")]
    pub commit: String,
}

#[derive(clap::Args, Debug)]
pub struct CommentResolveArgs {
    /// Comment ID at the root of the thread to resolve.
    #[arg(value_name = "COMMENT_ID")]
    pub comment_id: String,

    /// Optional resolution message.
    #[arg(long, short = 'm')]
    pub message: Option<String>,

    /// Commit the comment lives on (default: HEAD).
    #[arg(long, default_value = "HEAD")]
    pub commit: String,
}

#[derive(clap::Args, Debug)]
pub struct CommentReanchorArgs {
    /// Comment ID to re-anchor.
    #[arg(value_name = "COMMENT_ID")]
    pub comment_id: String,

    /// New line range, e.g. `42` or `42:44`.
    #[arg(long, value_name = "RANGE")]
    pub lines: String,

    /// Commit the comment lives on (default: HEAD).
    #[arg(long, default_value = "HEAD")]
    pub commit: String,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum FormatArg {
    Human,
    Json,
}

#[derive(clap::Args, Debug)]
pub struct FetchArgs {
    /// Remote to fetch from (default: origin).
    #[arg(value_name = "REMOTE", default_value = "origin")]
    pub remote: String,
}

#[derive(clap::Args, Debug)]
pub struct PushArgs {
    /// Remote to push to (default: origin).
    #[arg(value_name = "REMOTE", default_value = "origin")]
    pub remote: String,
}

#[derive(clap::Args, Debug)]
pub struct SyncArgs {
    /// Remote to sync with (default: origin).
    #[arg(value_name = "REMOTE", default_value = "origin")]
    pub remote: String,
}

#[derive(clap::Args, Debug)]
pub struct McpArgs {}
