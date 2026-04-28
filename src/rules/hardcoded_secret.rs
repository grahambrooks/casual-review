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
            regex: Regex::new(r"sk-(?:proj-|svcacct-|admin-)?[A-Za-z0-9_\-]{20,}").expect("valid regex"),
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

    fn run(&self, ctx: &RuleCtx<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for pattern in PATTERNS.iter() {
            for m in pattern.regex.find_iter(ctx.source) {
                let span = Span::from_byte_range(
                    ctx.path.to_path_buf(),
                    ctx.source,
                    m.start()..m.end(),
                );
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
