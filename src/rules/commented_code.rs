use super::{Rule, RuleCtx};
use crate::diagnostic::{Diagnostic, Severity, Span};
use crate::parse::Language;
use tree_sitter::{Query, QueryCursor};

pub struct CommentedCodeRule;

// Query to extract all comments from each language
const RUST_QUERY: &str = "(line_comment) @c (block_comment) @c";
const PYTHON_QUERY: &str = "(comment) @c";
const TS_QUERY: &str = "(comment) @c";
const JAVA_QUERY: &str = "(line_comment) @c (block_comment) @c";

impl Rule for CommentedCodeRule {
    fn id(&self) -> &'static str {
        "commented-code"
    }

    fn explain(&self) -> &'static str {
        "A block of commented-out source code. These are often debug artifacts left behind \
         during development or incomplete refactors.\n\n\
         Commented-out code clutters readability and creates doubt about whether it's safe to \
         delete. If the code is needed later, git history will retrieve it. If it's obsolete, \
         delete it. Real comments explaining *why* code exists are welcome — those look like \
         natural language, not syntax.\n\n\
         Detection is heuristic: we flag comments that look like valid code (balanced braces, \
         semicolons, operators, language keywords, etc.). False positives are possible in \
         comments that happen to include code snippets as examples; surface those and let the \
         reviewer decide."
    }

    fn run(&self, ctx: &RuleCtx<'_>) -> Vec<Diagnostic> {
        let (Some(tree), Some(language)) = (ctx.tree, ctx.language) else {
            return Vec::new();
        };
        let query_src = match language {
            Language::Rust => RUST_QUERY,
            Language::Python => PYTHON_QUERY,
            Language::TypeScript | Language::Tsx => TS_QUERY,
            Language::Java => JAVA_QUERY,
        };
        let ts_lang = language.ts_language();
        let Ok(query) = Query::new(&ts_lang, query_src) else {
            return Vec::new();
        };

        let mut cursor = QueryCursor::new();
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();

        for m in cursor.matches(&query, tree.root_node(), source_bytes) {
            for cap in m.captures {
                let comment_text = cap.node.utf8_text(source_bytes).unwrap_or("");
                if is_likely_commented_code(comment_text, language) {
                    let span = Span::from_byte_range(
                        ctx.path.to_path_buf(),
                        ctx.source,
                        cap.node.start_byte()..cap.node.end_byte(),
                    );
                    if !ctx.line_in_changes(span.line_start) {
                        continue;
                    }
                    diagnostics.push(
                        Diagnostic::new(
                            "commented-code",
                            Severity::Warning,
                            "commented-out code",
                            span,
                        )
                        .with_help("delete or restore to active code; git history preserves it"),
                    );
                }
            }
        }

        diagnostics
    }
}

/// Heuristic to detect whether a comment looks like source code.
/// Returns true if the comment contains indicators of actual code.
fn is_likely_commented_code(comment: &str, language: Language) -> bool {
    // Strip the comment markers (// or # or /* */)
    let code_text = strip_comment_markers(comment, language);

    // Filter out obviously non-code comments (very short, all lowercase words, etc.)
    if is_obviously_natural_language(&code_text) {
        return false;
    }

    // Check for code-like patterns
    has_code_indicators(&code_text, language)
}

fn strip_comment_markers(comment: &str, language: Language) -> String {
    let s = comment.trim();
    match language {
        Language::Rust | Language::TypeScript | Language::Tsx => {
            if let Some(stripped) = s.strip_prefix("//") {
                stripped.trim()
            } else if s.starts_with("/*") && s.ends_with("*/") {
                let inner = &s[2..s.len() - 2];
                inner.trim()
            } else {
                s
            }
        }
        Language::Python => {
            if let Some(stripped) = s.strip_prefix("#") {
                stripped.trim()
            } else {
                s
            }
        }
        Language::Java => {
            if let Some(stripped) = s.strip_prefix("//") {
                stripped.trim()
            } else if s.starts_with("/*") && s.ends_with("*/") {
                let inner = &s[2..s.len() - 2];
                inner.trim()
            } else {
                s
            }
        }
    }
    .to_string()
}

fn is_obviously_natural_language(text: &str) -> bool {
    if text.len() < 5 {
        return true;
    }

    // If it's all lowercase words separated by spaces, likely natural language
    let words: Vec<&str> = text.split_whitespace().collect();
    if !words.is_empty() && words.len() < 4 {
        // Short comment, likely natural language
        if words.iter().all(|w| is_lowercase_word(w)) {
            return true;
        }
    }

    // Check for typical comment sentence starts
    let text_lower = text.to_lowercase();
    if text_lower.starts_with("todo")
        || text_lower.starts_with("fixme")
        || text_lower.starts_with("xxx")
        || text_lower.starts_with("hack")
        || text_lower.starts_with("note")
        || text_lower.starts_with("see")
        || text_lower.starts_with("returns")
        || text_lower.starts_with("throws")
        || text_lower.starts_with("raises")
        || text_lower.starts_with("param")
        || (text_lower.starts_with("test") && !text.contains("assert"))
    {
        return true;
    }

    false
}

fn is_lowercase_word(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_lowercase() || !c.is_alphabetic()) && s.len() > 1
}

fn has_code_indicators(text: &str, language: Language) -> bool {
    // Check for syntactic indicators
    let code_chars = count_code_indicators(text);
    let mut has_code = code_chars >= 2;

    // Check for common keywords
    if has_language_keywords(text, language) {
        has_code = true;
    }

    // Check for function/method calls (word followed by parenthesis)
    if text.contains('(') && text.contains(')') {
        // Look for a word followed by (
        if regex_like_pattern_match(text, r"\w+\s*\(") {
            has_code = true;
        }
    }

    // Check for assignments (= without ==)
    if text.contains('=')
        && !text.contains("==")
        && !text.contains("!=")
        && !text.contains("=>")
        && regex_like_pattern_match(text, r"\w+\s*=")
    {
        has_code = true;
    }

    // Check for semicolons at end or in middle (rare in comments)
    if text.contains(';') {
        has_code = true;
    }

    // Check for common code patterns: variable declarations, control structures
    if regex_like_pattern_match(
        text,
        r"\blet\s+\w+|var\s+\w+|const\s+\w+|fn\s+\w+|def\s+\w+",
    ) {
        has_code = true;
    }

    has_code
}

fn count_code_indicators(text: &str) -> usize {
    let mut count = 0;
    for c in text.chars() {
        match c {
            '{' | '}' | '[' | ']' | '(' | ')' | ';' => count += 1,
            _ => {}
        }
    }
    count
}

fn has_language_keywords(text: &str, language: Language) -> bool {
    let keywords = match language {
        Language::Rust => vec![
            "fn", "let", "mut", "const", "if", "else", "match", "for", "while", "loop", "return",
            "pub", "struct", "enum", "impl", "trait", "type", "use", "mod",
        ],
        Language::Python => vec![
            "def", "class", "if", "else", "elif", "for", "while", "try", "except", "finally",
            "return", "import", "from", "as", "with", "pass", "break", "continue", "lambda",
        ],
        Language::TypeScript | Language::Tsx => vec![
            "function",
            "const",
            "let",
            "var",
            "if",
            "else",
            "for",
            "while",
            "do",
            "switch",
            "case",
            "return",
            "class",
            "interface",
            "type",
            "enum",
            "import",
            "export",
            "async",
            "await",
            "new",
            "this",
        ],
        Language::Java => vec![
            "class",
            "interface",
            "public",
            "private",
            "static",
            "void",
            "int",
            "String",
            "if",
            "else",
            "for",
            "while",
            "do",
            "switch",
            "case",
            "try",
            "catch",
            "finally",
            "return",
            "new",
            "import",
            "extends",
            "implements",
        ],
    };

    for keyword in keywords {
        if regex_like_pattern_match(text, &format!(r"\b{}\b", keyword)) {
            return true;
        }
    }
    false
}

/// Simple regex-like pattern matching without full regex engine
fn regex_like_pattern_match(text: &str, pattern: &str) -> bool {
    // For now, use simple substring matching with word boundaries
    // This is a simplified implementation suitable for our use case
    if pattern.starts_with(r"\b") && pattern.ends_with(r"\b") {
        let keyword = &pattern[2..pattern.len() - 2];
        if text.contains(keyword) {
            // Check word boundaries
            return text
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .any(|word| word == keyword);
        }
        false
    } else if pattern.contains(r"\s+\(") {
        // Function call pattern: \w+\s*\(
        let text_lower = text.to_lowercase();
        text_lower.contains('(')
            && text
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .any(|word| {
                    if word.is_empty() {
                        return false;
                    }
                    let rest = &text[text.find(word).unwrap_or(0) + word.len()..];
                    rest.trim_start().starts_with('(')
                })
    } else if pattern.contains(r"\s*=") {
        // Assignment pattern
        text.contains('=')
            && !text.contains("==")
            && !text.contains("!=")
            && (text
                .split('=')
                .next()
                .map(|s| s.contains(|c: char| c.is_alphanumeric()))
                .unwrap_or(false))
    } else {
        // Substring match
        text.contains(pattern)
    }
}
