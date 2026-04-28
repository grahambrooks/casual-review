use anyhow::Context;
use casual_review::cli::{CheckArgs, Cli, Command, ExplainArgs, FormatArg};
use casual_review::diagnostic::Severity;
use casual_review::engine::{run_diff, run_paths, run_repo, EngineOutput};
use casual_review::git::DiffSpec;
use casual_review::render::{self, Format};
use casual_review::rules::default_rules;
use clap::Parser;
use std::io::Write;
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
    };

    let mut stderr = std::io::stderr().lock();
    let mut stdout = std::io::stdout().lock();
    let writer: &mut dyn Write = match format {
        Format::Human => &mut stderr,
        Format::Json => &mut stdout,
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
