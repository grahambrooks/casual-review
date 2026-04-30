use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "cr", version, about = "Ultra-fast code review CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Run review checks. With no paths, reviews the working-tree diff against HEAD.
    Check(CheckArgs),
    /// Print documentation for a rule. With no argument, lists all rules.
    Explain(ExplainArgs),
    /// Publish findings to git notes on a commit.
    Publish(PublishArgs),
    /// Show findings stored in git notes for a commit.
    Show(ShowArgs),
    /// Dismiss a finding by appending an ack note.
    Ack(AckArgs),
    /// Fetch findings from a remote repository.
    Fetch(FetchArgs),
    /// Push findings to a remote repository.
    Push(PushArgs),
}

#[derive(clap::Args, Debug)]
pub struct ExplainArgs {
    /// Rule id (e.g. `cognitive-complexity`). Omit to list all rules.
    pub rule: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct CheckArgs {
    /// In default mode: explicit files to check. In `--repo` mode: roots to walk (default: cwd).
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// Walk a directory tree (respects .gitignore). Bypasses diff filtering entirely.
    /// Roots default to cwd; pass paths positionally to override.
    #[arg(long, conflicts_with_all = ["staged", "all"])]
    pub repo: bool,

    /// Use the staged diff (HEAD vs index) instead of the working-tree diff.
    #[arg(long, conflicts_with = "paths")]
    pub staged: bool,

    /// Report diagnostics on unchanged lines too (within changed files).
    #[arg(long)]
    pub all: bool,

    /// Print a one-line summary to stderr after the run.
    #[arg(short, long)]
    pub verbose: bool,

    /// Output format.
    #[arg(long, value_enum, default_value_t = FormatArg::Human)]
    pub format: FormatArg,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum FormatArg {
    Human,
    Json,
    Github,
    Sarif,
}

#[derive(clap::Args, Debug)]
pub struct PublishArgs {
    /// Commit to attach findings to (default: HEAD).
    #[arg(value_name = "COMMIT", default_value = "HEAD")]
    pub commit: String,

    /// Output format for findings.
    #[arg(long, value_enum, default_value_t = FormatArg::Json)]
    pub format: FormatArg,

    /// Read findings from stdin instead of running checks.
    #[arg(long)]
    pub from_stdin: bool,
}

#[derive(clap::Args, Debug)]
pub struct ShowArgs {
    /// Commit to read findings from (default: HEAD).
    #[arg(value_name = "COMMIT", default_value = "HEAD")]
    pub commit: String,

    /// Output format for findings.
    #[arg(long, value_enum, default_value_t = FormatArg::Human)]
    pub format: FormatArg,
}

#[derive(clap::Args, Debug)]
pub struct AckArgs {
    /// Finding ID to dismiss.
    #[arg(value_name = "FINDING_ID")]
    pub finding_id: String,

    /// Optional message for this dismissal.
    #[arg(value_name = "MESSAGE", default_value = "")]
    pub message: String,

    /// Commit that the finding is attached to (default: HEAD).
    #[arg(long, default_value = "HEAD")]
    pub commit: String,
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
