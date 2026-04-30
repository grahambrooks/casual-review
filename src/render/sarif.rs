use crate::diagnostic::{Diagnostic, Severity};
use serde_json::{json, Value};
use std::io::Write;

/// Render diagnostics as SARIF 2.1.0 (Static Analysis Results Format).
///
/// SARIF is the standard format for code analysis tools and enables:
/// - GitHub code scanning native integration
/// - IDE plugins and dashboards
/// - Cross-tool result aggregation
pub fn render(diagnostics: &[Diagnostic], out: &mut dyn Write) -> std::io::Result<()> {
    let sarif = build_sarif(diagnostics);
    let json = serde_json::to_string(&sarif).map_err(std::io::Error::other)?;
    writeln!(out, "{}", json)?;
    Ok(())
}

fn build_sarif(diagnostics: &[Diagnostic]) -> Value {
    let results: Vec<Value> = diagnostics.iter().map(build_result).collect();

    json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [
            {
                "tool": {
                    "driver": {
                        "name": "casual-review",
                        "version": env!("CARGO_PKG_VERSION"),
                        "informationUri": "https://github.com/grahambrooks/casual-review",
                        "rules": build_rule_definitions(diagnostics),
                    }
                },
                "results": results,
            }
        ]
    })
}

fn build_result(diag: &Diagnostic) -> Value {
    let level = match diag.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Note => "note",
        Severity::Help => "note",
    };

    json!({
        "ruleId": diag.code,
        "level": level,
        "message": {
            "text": diag.message.clone(),
        },
        "locations": [
            {
                "physicalLocation": {
                    "artifactLocation": {
                        "uri": diag.primary.file.to_string_lossy().to_string().replace('\\', "/"),
                    },
                    "region": {
                        "startLine": diag.primary.line_start,
                        "startColumn": diag.primary.col_start,
                        "endLine": diag.primary.line_end,
                        "endColumn": diag.primary.col_end,
                    }
                }
            }
        ],
        "suppressions": [],
    })
}

fn build_rule_definitions(diagnostics: &[Diagnostic]) -> Vec<Value> {
    let mut rules = std::collections::HashMap::new();

    for diag in diagnostics {
        rules.entry(diag.code.clone()).or_insert_with(|| {
            let level = match diag.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
                Severity::Note => "note",
                Severity::Help => "note",
            };

            json!({
                "id": diag.code.clone(),
                "shortDescription": {
                    "text": diag.message.clone(),
                },
                "defaultConfiguration": {
                    "level": level,
                },
                "help": {
                    "text": diag.helps.join("\n"),
                }
            })
        });
    }

    rules.into_values().collect()
}
