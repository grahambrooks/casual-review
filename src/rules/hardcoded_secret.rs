use super::{Rule, RuleCtx};
use crate::diagnostic::{Diagnostic, Severity, Span};
use once_cell::sync::Lazy;
use regex::Regex;

pub struct HardcodedSecretRule;

struct Pattern {
    label: &'static str,
    regex: Regex,
}

static PATTERNS: Lazy<Vec<Pattern>> = Lazy::new(|| {
    vec![
        Pattern {
            label: "AWS access key id",
            regex: Regex::new(r"AKIA[0-9A-Z]{16}").expect("valid regex"),
        },
        Pattern {
            label: "GitHub personal access token",
            regex: Regex::new(r"gh[pousr]_[A-Za-z0-9]{36,255}").expect("valid regex"),
        },
        Pattern {
            label: "Slack token",
            regex: Regex::new(r"xox[abprs]-[A-Za-z0-9-]{10,}").expect("valid regex"),
        },
        Pattern {
            label: "OpenAI API key",
            regex: Regex::new(r"sk-(?:proj-|svcacct-|admin-)?[A-Za-z0-9_\-]{20,}")
                .expect("valid regex"),
        },
        Pattern {
            label: "private key header",
            regex: Regex::new(r"-----BEGIN [A-Z ]*PRIVATE KEY-----").expect("valid regex"),
        },
        Pattern {
            label: "Google API key",
            regex: Regex::new(r"AIza[0-9A-Za-z_\-]{35}").expect("valid regex"),
        },
    ]
});

impl Rule for HardcodedSecretRule {
    fn id(&self) -> &'static str {
        "hardcoded-secret"
    }

    fn explain(&self) -> &'static str {
        "A pattern match for a known secret format: AWS access key, GitHub PAT, Slack token, \
         OpenAI/Google API key, or a PEM private-key header.\n\n\
         Severity is Error because committing a secret is hard to undo: the secret is in the \
         git history forever even after deletion, and any clone or fork retains it. Anyone \
         who reads the repo (now or future) can use the credential.\n\n\
         Fix:\n\
         1. Rotate the credential immediately — assume it is compromised.\n\
         2. Remove from the working tree and load from environment variables, a secret manager, \
            or a config file ignored by `.gitignore`.\n\
         3. If already pushed, follow your org's secret-leak runbook (BFG repo-cleaner or \
            `git-filter-repo` to scrub history; coordinate with security if it's a shared repo).\n\n\
         False positives are possible — if a string matching a secret pattern is part of a \
         test fixture or sample data, that's a known shape and path-based suppression will \
         eventually allow it. Don't suppress a real secret."
    }

    fn run(&self, ctx: &RuleCtx<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for pattern in PATTERNS.iter() {
            for m in pattern.regex.find_iter(ctx.source) {
                let span =
                    Span::from_byte_range(ctx.path.to_path_buf(), ctx.source, m.start()..m.end());
                if !ctx.line_in_changes(span.line_start) {
                    continue;
                }
                diagnostics.push(
                    Diagnostic::new(
                        "hardcoded-secret",
                        Severity::Error,
                        format!("possible {}", pattern.label),
                        span,
                    )
                    .with_note("commit history retains secrets even after deletion")
                    .with_help("rotate the credential and load it from env or a secret manager"),
                );
            }
        }
        diagnostics
    }
}
