use anyhow::Context;
use casual_review::cli::{
    AckArgs, CheckArgs, Cli, Command, ExplainArgs, FetchArgs, FormatArg, PublishArgs, PushArgs,
    ShowArgs,
};
use casual_review::diagnostic::Severity;
use casual_review::engine::{run_diff, run_paths, run_repo, EngineOutput};
use casual_review::git::DiffSpec;
use casual_review::git_notes;
use casual_review::notes::{Finding, Location, NotesPayload};
use casual_review::render::{self, Format};
use casual_review::rules::default_rules;
use clap::Parser;
use std::io::{Read, Write};
use std::process::ExitCode;

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
        Command::Publish(args) => match run_publish(args) {
            Ok(_) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                ExitCode::from(2)
            }
        },
        Command::Show(args) => match run_show(args) {
            Ok(_) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                ExitCode::from(2)
            }
        },
        Command::Ack(args) => match run_ack(args) {
            Ok(_) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                ExitCode::from(2)
            }
        },
        Command::Fetch(args) => match run_fetch(args) {
            Ok(_) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                ExitCode::from(2)
            }
        },
        Command::Push(args) => match run_push(args) {
            Ok(_) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                ExitCode::from(2)
            }
        },
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
        // Read JSON diagnostics from stdin
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
        // Run check to get diagnostics
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

    // Create payload with findings
    let payload = NotesPayload::new(args.commit.clone(), output.diagnostics.clone());

    // Write to git notes/findings storage
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

    // Read findings from git notes
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

    // Read existing findings
    match git_notes::read_notes(&cwd, &args.commit)? {
        Some(mut payload) => {
            // Check if finding exists
            let finding_exists = payload.findings.iter().any(|f| f.id == args.finding_id);
            if !finding_exists {
                return Err(anyhow::anyhow!(
                    "Finding {} not found in commit {}",
                    args.finding_id,
                    args.commit
                ));
            }

            // Create dismissal entry
            let dismissal = Finding {
                id: format!("{}-dismissed", args.finding_id),
                rule: "dismissed".to_string(),
                severity: "note".to_string(),
                location: Location {
                    file: std::path::PathBuf::from(""),
                    byte_range: (0, 0),
                    line_range: (0, 0),
                    col_range: (0, 0),
                },
                message: args.message.clone(),
                labels: vec![],
                suggestions: vec![],
                parent: Some(args.finding_id.clone()),
            };

            // Append dismissal to findings
            payload.findings.push(dismissal);

            // Write updated payload
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

    // Run git fetch for casual-review notes ref
    let output = std::process::Command::new("git")
        .current_dir(&cwd)
        .arg("fetch")
        .arg(&args.remote)
        .arg("refs/notes/casual-review:refs/notes/casual-review")
        .output()
        .context("running git fetch")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("fatal:") || !stderr.is_empty() {
            return Err(anyhow::anyhow!("git fetch failed: {}", stderr));
        }
    }

    eprintln!(
        "Fetched findings from {} (refs/notes/casual-review)",
        args.remote
    );
    Ok(())
}

fn run_push(args: PushArgs) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("getting current directory")?;

    // Run git push for casual-review notes ref
    let output = std::process::Command::new("git")
        .current_dir(&cwd)
        .arg("push")
        .arg(&args.remote)
        .arg("refs/notes/casual-review:refs/notes/casual-review")
        .output()
        .context("running git push")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("fatal:") || !stderr.is_empty() {
            return Err(anyhow::anyhow!("git push failed: {}", stderr));
        }
    }

    eprintln!(
        "Pushed findings to {} (refs/notes/casual-review)",
        args.remote
    );
    Ok(())
}
