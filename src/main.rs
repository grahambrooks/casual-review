use anyhow::Context;
use casual_review::cli::{
    AckArgs, CheckArgs, Cli, Command, CommentAddArgs, CommentCmd, CommentListArgs,
    CommentReanchorArgs, CommentReplyArgs, CommentResolveArgs, CommentSubcommand, ExplainArgs,
    FetchArgs, FormatArg, PublishArgs, PushArgs, ShowArgs,
};
use casual_review::comments::{
    author_from_git, comment_id, sha256_hex, Anchor, Comment, CommentsPayload,
};
use casual_review::diagnostic::Severity;
use casual_review::engine::{run_diff, run_paths, run_repo, EngineOutput};
use casual_review::git::DiffSpec;
use casual_review::notes::{Finding, Location, NotesPayload};
use casual_review::render::{self, Format};
use casual_review::rules::default_rules;
use casual_review::{git_comments, git_notes};
use clap::Parser;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command as ProcCommand, ExitCode};

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Check(args) => match run_check(args) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("error: {e:#}");
                ExitCode::from(2)
            }
        },
        Command::Explain(args) => run_explain(args),
        Command::Publish(args) => run_or_fail(run_publish(args)),
        Command::Show(args) => run_or_fail(run_show(args)),
        Command::Ack(args) => run_or_fail(run_ack(args)),
        Command::Fetch(args) => run_or_fail(run_fetch(args)),
        Command::Push(args) => run_or_fail(run_push(args)),
        Command::Comment(cmd) => run_or_fail(run_comment(cmd)),
    }
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

fn run_explain(args: ExplainArgs) -> ExitCode {
    let rules = default_rules();
    match args.rule {
        None => {
            println!("Available rules ({}):\n", rules.len());
            for rule in &rules {
                let id = rule.id();
                let summary = rule.explain().lines().next().unwrap_or("").trim();
                println!("  {id:<24} {summary}");
            }
            println!("\nRun `cr explain <rule-id>` for full documentation.");
            ExitCode::SUCCESS
        }
        Some(needle) => match rules.iter().find(|r| r.id() == needle) {
            Some(rule) => {
                println!("# {}\n", rule.id());
                println!("{}", rule.explain());
                ExitCode::SUCCESS
            }
            None => {
                eprintln!("error: unknown rule `{needle}`");
                eprintln!("       run `cr explain` to list available rules");
                ExitCode::from(2)
            }
        },
    }
}

fn run_check(args: CheckArgs) -> anyhow::Result<ExitCode> {
    let mode = pick_mode(&args);
    let output: EngineOutput = match &mode {
        Mode::Repo => {
            let roots = if args.paths.is_empty() {
                vec![std::env::current_dir().context("getting current directory")?]
            } else {
                args.paths.clone()
            };
            run_repo(&roots)?
        }
        Mode::Diff(spec) => {
            let cwd = std::env::current_dir().context("getting current directory")?;
            run_diff(&cwd, spec.clone(), args.all)?
        }
        Mode::Paths => run_paths(&args.paths)?,
    };

    let format = match args.format {
        FormatArg::Human => Format::Human,
        FormatArg::Json => Format::Json,
        FormatArg::Github => Format::Github,
        FormatArg::Sarif => Format::Sarif,
    };

    let mut stderr = std::io::stderr().lock();
    let mut stdout = std::io::stdout().lock();
    let writer: &mut dyn Write = match format {
        Format::Human => &mut stderr,
        Format::Json => &mut stdout,
        Format::Github => &mut stdout,
        Format::Sarif => &mut stdout,
    };

    render::render(format, &output.diagnostics, &output.sources, writer)?;

    if args.verbose {
        eprintln!(
            "checked {} file(s), {} diagnostic(s)",
            output.files_checked,
            output.diagnostics.len()
        );
    }

    if output.files_checked == 0 && output.diagnostics.is_empty() {
        emit_zero_files_hint(&mode);
    }

    let any_error = output
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error);

    Ok(if any_error {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

enum Mode {
    Repo,
    Diff(DiffSpec),
    Paths,
}

fn pick_mode(args: &CheckArgs) -> Mode {
    if args.repo {
        Mode::Repo
    } else if !args.paths.is_empty() {
        Mode::Paths
    } else if args.staged {
        Mode::Diff(DiffSpec::Staged)
    } else {
        Mode::Diff(DiffSpec::WorkingTree)
    }
}

fn emit_zero_files_hint(mode: &Mode) {
    match mode {
        Mode::Diff(_) => {
            eprintln!(
                "cr: no changed files to check.\n     \
                 hint: working tree is clean — try `cr check --repo` to scan everything,\n     \
                 or `cr check PATH...` for explicit files."
            );
        }
        Mode::Repo => {
            eprintln!(
                "cr: no supported source files found.\n     \
                 hint: supported extensions are .rs, .py, .pyi, .ts, .mts, .cts, .tsx.\n     \
                 hint: check the path you passed exists and isn't excluded by .gitignore."
            );
        }
        Mode::Paths => {}
    }
}

fn run_publish(args: PublishArgs) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("getting current directory")?;

    let output: EngineOutput = if args.from_stdin {
        let mut json_input = String::new();
        std::io::stdin().read_to_string(&mut json_input)?;
        let diagnostics: Vec<casual_review::diagnostic::Diagnostic> = json_input
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();

        EngineOutput {
            diagnostics,
            sources: Default::default(),
            files_checked: 0,
        }
    } else {
        let check_mode = pick_mode(&CheckArgs {
            paths: vec![],
            repo: false,
            staged: false,
            all: false,
            verbose: false,
            format: FormatArg::Json,
        });

        match &check_mode {
            Mode::Repo => run_repo(std::slice::from_ref(&cwd))?,
            Mode::Diff(spec) => run_diff(&cwd, spec.clone(), false)?,
            Mode::Paths => EngineOutput {
                diagnostics: vec![],
                sources: Default::default(),
                files_checked: 0,
            },
        }
    };

    let payload = NotesPayload::new(args.commit.clone(), output.diagnostics.clone());
    git_notes::write_notes(&cwd, &args.commit, payload)?;

    eprintln!(
        "Published {} finding(s) to commit {}",
        output.diagnostics.len(),
        args.commit
    );
    Ok(())
}

fn run_show(args: ShowArgs) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("getting current directory")?;

    match git_notes::read_notes(&cwd, &args.commit)? {
        Some(payload) => {
            eprintln!("Findings for commit {}:", args.commit);
            eprintln!("  Tool: {} v{}", payload.tool, payload.tool_version);
            eprintln!("  Produced at: {}", payload.produced_at);
            eprintln!("  Findings: {}", payload.findings.len());

            for finding in &payload.findings {
                eprintln!(
                    "  [{:?}] {} ({}:{})",
                    finding.severity,
                    finding.rule,
                    finding.location.file.display(),
                    finding.location.line_range.0
                );
                eprintln!("    {}", finding.message);
            }
            Ok(())
        }
        None => {
            eprintln!("No findings stored for commit {}", args.commit);
            Ok(())
        }
    }
}

fn run_ack(args: AckArgs) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("getting current directory")?;

    match git_notes::read_notes(&cwd, &args.commit)? {
        Some(mut payload) => {
            let finding_exists = payload.findings.iter().any(|f| f.id == args.finding_id);
            if !finding_exists {
                return Err(anyhow::anyhow!(
                    "Finding {} not found in commit {}",
                    args.finding_id,
                    args.commit
                ));
            }

            let dismissal = Finding {
                id: format!("{}-dismissed", args.finding_id),
                rule: "dismissed".to_string(),
                severity: "note".to_string(),
                location: Location {
                    file: PathBuf::from(""),
                    byte_range: (0, 0),
                    line_range: (0, 0),
                    col_range: (0, 0),
                },
                message: args.message.clone(),
                labels: vec![],
                suggestions: vec![],
                parent: Some(args.finding_id.clone()),
            };

            payload.findings.push(dismissal);
            git_notes::write_notes(&cwd, &args.commit, payload)?;
            eprintln!(
                "Acknowledged finding {} on commit {}",
                args.finding_id, args.commit
            );
            Ok(())
        }
        None => Err(anyhow::anyhow!(
            "No findings found for commit {}",
            args.commit
        )),
    }
}

fn run_fetch(args: FetchArgs) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("getting current directory")?;
    git_notes::fetch(&cwd, &args.remote)?;
    git_comments::fetch(&cwd, &args.remote)?;
    eprintln!(
        "Fetched findings and comments from {} \
         (refs/notes/casual-review/{{findings,discuss}})",
        args.remote
    );
    Ok(())
}

fn run_push(args: PushArgs) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("getting current directory")?;
    git_notes::push(&cwd, &args.remote)?;
    git_comments::push(&cwd, &args.remote)?;
    eprintln!(
        "Pushed findings and comments to {} \
         (refs/notes/casual-review/{{findings,discuss}})",
        args.remote
    );
    Ok(())
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
    let author = author_from_git(&cwd)?;
    let commit_sha = resolve_commit(&cwd, &args.commit)?;

    let anchor = if args.commit_level {
        Anchor {
            file: None,
            line_range: (0, 0),
            byte_range: (0, 0),
            anchor_text_sha: String::new(),
        }
    } else {
        let file = args.file.clone().ok_or_else(|| {
            anyhow::anyhow!("file argument is required unless --commit-level is set")
        })?;
        let file = repo_relative(&cwd, &file)?;
        let content = read_file_at_commit(&cwd, &commit_sha, &file)?;

        if args.file_level {
            Anchor {
                file: Some(file),
                line_range: (0, 0),
                byte_range: (0, content.len()),
                anchor_text_sha: sha256_hex(&content),
            }
        } else {
            let lines = args
                .lines
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("--lines is required (or pass --file-level)"))?;
            let (line_start, line_end) = parse_line_range(lines)?;
            let (start, end) = line_range_to_byte_range(&content, line_start, line_end)?;
            Anchor {
                file: Some(file),
                line_range: (line_start, line_end),
                byte_range: (start, end),
                anchor_text_sha: sha256_hex(&content[start..end]),
            }
        }
    };

    let created_at = chrono::Utc::now().to_rfc3339();
    let id = comment_id(&author, &created_at, &anchor, &body);
    let comment = Comment {
        id: id.clone(),
        author,
        created_at,
        anchor,
        body,
        parent: None,
        resolved: false,
        origin_commit: None,
    };

    let mut payload = git_comments::read_comments(&cwd, &commit_sha)?
        .unwrap_or_else(|| CommentsPayload::new(commit_sha.clone()));
    payload.comments.push(comment);
    git_comments::write_comments(&cwd, &commit_sha, &payload)?;

    println!("{}", id);
    eprintln!(
        "Added comment {} on commit {}",
        id,
        &commit_sha[..12.min(commit_sha.len())]
    );
    Ok(())
}

fn run_comment_list(args: CommentListArgs) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("getting current directory")?;
    let commit_sha = resolve_commit(&cwd, &args.commit)?;

    let mut payload = git_comments::read_comments(&cwd, &commit_sha)?
        .unwrap_or_else(|| CommentsPayload::new(commit_sha.clone()));

    if args.include_ancestors {
        for sha in list_commented_commits(&cwd) {
            if sha == commit_sha {
                continue;
            }
            if !is_ancestor(&cwd, &sha, &commit_sha) {
                continue;
            }
            if let Some(ancestor) = git_comments::read_comments(&cwd, &sha)? {
                for mut c in ancestor.comments {
                    c.origin_commit = Some(sha.clone());
                    payload.comments.push(c);
                }
            }
        }
    }

    let resolved_roots: std::collections::HashSet<String> = payload
        .comments
        .iter()
        .filter(|c| c.resolved)
        .filter_map(|c| c.parent.clone())
        .collect();

    let visible: Vec<Comment> = payload
        .comments
        .iter()
        .filter(|c| {
            if let Some(file_filter) = &args.file {
                if c.anchor.file.as_deref() != Some(file_filter.as_path()) {
                    return false;
                }
            }
            if !args.include_resolved {
                let root = c.parent.clone().unwrap_or_else(|| c.id.clone());
                if resolved_roots.contains(&root) {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect();

    match args.format {
        FormatArg::Json => {
            let filtered = CommentsPayload {
                schema: payload.schema.clone(),
                tool: payload.tool.clone(),
                tool_version: payload.tool_version.clone(),
                commit: payload.commit.clone(),
                comments: visible,
            };
            println!("{}", filtered.to_json()?);
        }
        _ => print_comments_human(&cwd, &commit_sha, &visible)?,
    }

    Ok(())
}

/// All commits on which any comment has been written, regardless of ancestry.
/// Returns empty when the discuss ref does not yet exist.
fn list_commented_commits(repo: &Path) -> Vec<String> {
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

fn is_ancestor(repo: &Path, ancestor_sha: &str, descendant_sha: &str) -> bool {
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

fn print_comments_human(repo: &Path, target_sha: &str, comments: &[Comment]) -> anyhow::Result<()> {
    if comments.is_empty() {
        eprintln!("(no comments)");
        return Ok(());
    }
    for c in comments {
        let stale = is_stale(repo, &c.anchor)?;
        let stale_marker = if stale { " [stale]" } else { "" };
        let origin_marker = match c.origin_commit.as_deref() {
            Some(o) if o != target_sha => format!(" [from {}]", &o[..o.len().min(8)]),
            _ => String::new(),
        };
        let anchor_str = match &c.anchor.file {
            None => "<commit>".to_string(),
            Some(f) if c.anchor.line_range == (0, 0) => format!("{}", f.display()),
            Some(f) => format!(
                "{}:{}-{}",
                f.display(),
                c.anchor.line_range.0,
                c.anchor.line_range.1
            ),
        };
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

fn run_comment_reply(args: CommentReplyArgs) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("getting current directory")?;
    let body = read_body_or_editor(args.body.as_deref(), "reply")?;
    let author = author_from_git(&cwd)?;
    let commit_sha = resolve_commit(&cwd, &args.commit)?;

    let mut payload = git_comments::read_comments(&cwd, &commit_sha)?
        .ok_or_else(|| anyhow::anyhow!("No comments on commit {}", args.commit))?;
    let parent = payload
        .comments
        .iter()
        .find(|c| c.id == args.comment_id)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Comment {} not found on commit {}",
                args.comment_id,
                args.commit
            )
        })?
        .clone();

    let created_at = chrono::Utc::now().to_rfc3339();
    let anchor = parent.anchor.clone();
    let id = comment_id(&author, &created_at, &anchor, &body);
    let reply = Comment {
        id: id.clone(),
        author,
        created_at,
        anchor,
        body,
        parent: Some(parent.id.clone()),
        resolved: false,
        origin_commit: None,
    };
    payload.comments.push(reply);
    git_comments::write_comments(&cwd, &commit_sha, &payload)?;

    println!("{}", id);
    eprintln!("Replied to {} with {}", parent.id, id);
    Ok(())
}

fn run_comment_resolve(args: CommentResolveArgs) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("getting current directory")?;
    let author = author_from_git(&cwd)?;
    let commit_sha = resolve_commit(&cwd, &args.commit)?;

    let mut payload = git_comments::read_comments(&cwd, &commit_sha)?
        .ok_or_else(|| anyhow::anyhow!("No comments on commit {}", args.commit))?;
    let target = payload
        .comments
        .iter()
        .find(|c| c.id == args.comment_id)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Comment {} not found on commit {}",
                args.comment_id,
                args.commit
            )
        })?
        .clone();

    let created_at = chrono::Utc::now().to_rfc3339();
    let anchor = target.anchor.clone();
    let body = args.message.unwrap_or_default();
    let id = comment_id(&author, &created_at, &anchor, &body);
    let resolution = Comment {
        id: id.clone(),
        author,
        created_at,
        anchor,
        body,
        parent: Some(target.id.clone()),
        resolved: true,
        origin_commit: None,
    };
    payload.comments.push(resolution);
    git_comments::write_comments(&cwd, &commit_sha, &payload)?;

    println!("{}", id);
    eprintln!("Resolved {} with {}", target.id, id);
    Ok(())
}

fn run_comment_reanchor(args: CommentReanchorArgs) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("getting current directory")?;
    let author = author_from_git(&cwd)?;
    let commit_sha = resolve_commit(&cwd, &args.commit)?;

    let mut payload = git_comments::read_comments(&cwd, &commit_sha)?
        .ok_or_else(|| anyhow::anyhow!("No comments on commit {}", args.commit))?;
    let target_idx = payload
        .comments
        .iter()
        .position(|c| c.id == args.comment_id)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Comment {} not found on commit {}",
                args.comment_id,
                args.commit
            )
        })?;

    let file = payload.comments[target_idx]
        .anchor
        .file
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Cannot reanchor a commit-level comment"))?;
    let content = read_file_at_commit(&cwd, &commit_sha, &file)?;
    let (line_start, line_end) = parse_line_range(&args.lines)?;
    let (start, end) = line_range_to_byte_range(&content, line_start, line_end)?;
    let new_anchor = Anchor {
        file: Some(file),
        line_range: (line_start, line_end),
        byte_range: (start, end),
        anchor_text_sha: sha256_hex(&content[start..end]),
    };

    let created_at = chrono::Utc::now().to_rfc3339();
    let body = format!(
        "reanchored {} → lines {}:{}",
        args.comment_id, line_start, line_end
    );
    let target_id = payload.comments[target_idx].id.clone();
    let id = comment_id(&author, &created_at, &new_anchor, &body);
    let record = Comment {
        id: id.clone(),
        author,
        created_at,
        anchor: new_anchor,
        body,
        parent: Some(target_id),
        resolved: false,
        origin_commit: None,
    };
    payload.comments.push(record);
    git_comments::write_comments(&cwd, &commit_sha, &payload)?;

    println!("{}", id);
    Ok(())
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

fn resolve_commit(repo: &Path, commit: &str) -> anyhow::Result<String> {
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

fn repo_root(repo: &Path) -> anyhow::Result<PathBuf> {
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
        .with_context(|| format!("resolving path {}", path.display()))?;
    let root = repo_root(repo)?
        .canonicalize()
        .context("canonicalizing repo root")?;
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

fn parse_line_range(s: &str) -> anyhow::Result<(u32, u32)> {
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
            "line {line_end} is past end of file (file has {} line(s))",
            line
        ));
    }
    Ok((start, end))
}

fn is_stale(repo: &Path, anchor: &Anchor) -> anyhow::Result<bool> {
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
        // file-level
        sha256_hex(&bytes)
    } else if anchor.byte_range.1 <= bytes.len() {
        sha256_hex(&bytes[anchor.byte_range.0..anchor.byte_range.1])
    } else {
        return Ok(true);
    };
    Ok(cur != anchor.anchor_text_sha)
}
