use std::cell::RefCell;
use std::path::Path;
use thiserror::Error;
use tree_sitter::{Parser, Tree};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Python,
    TypeScript,
    Tsx,
    Java,
}

impl Language {
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?;
        match ext {
            "rs" => Some(Language::Rust),
            "py" | "pyi" => Some(Language::Python),
            "ts" | "mts" | "cts" => Some(Language::TypeScript),
            "tsx" => Some(Language::Tsx),
            "java" => Some(Language::Java),
            _ => None,
        }
    }

    pub fn ts_language(self) -> tree_sitter::Language {
        match self {
            Language::Rust => tree_sitter_rust::language(),
            Language::Python => tree_sitter_python::language(),
            Language::TypeScript => tree_sitter_typescript::language_typescript(),
            Language::Tsx => tree_sitter_typescript::language_tsx(),
            Language::Java => tree_sitter_java::language(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("tree-sitter could not produce a tree (input too large or grammar mismatch)")]
    NoTree,
    #[error("failed to set tree-sitter language: {0}")]
    SetLanguage(#[from] tree_sitter::LanguageError),
}

thread_local! {
    static RUST_PARSER: RefCell<Option<Parser>> = const { RefCell::new(None) };
    static PYTHON_PARSER: RefCell<Option<Parser>> = const { RefCell::new(None) };
    static TS_PARSER: RefCell<Option<Parser>> = const { RefCell::new(None) };
    static TSX_PARSER: RefCell<Option<Parser>> = const { RefCell::new(None) };
    static JAVA_PARSER: RefCell<Option<Parser>> = const { RefCell::new(None) };
}

pub fn parse(language: Language, source: &[u8]) -> Result<Tree, ParseError> {
    let cell = match language {
        Language::Rust => &RUST_PARSER,
        Language::Python => &PYTHON_PARSER,
        Language::TypeScript => &TS_PARSER,
        Language::Tsx => &TSX_PARSER,
        Language::Java => &JAVA_PARSER,
    };
    with_parser(cell, language, |p| {
        p.parse(source, None).ok_or(ParseError::NoTree)
    })
}

fn with_parser<R>(
    cell: &'static std::thread::LocalKey<RefCell<Option<Parser>>>,
    lang: Language,
    f: impl FnOnce(&mut Parser) -> Result<R, ParseError>,
) -> Result<R, ParseError> {
    cell.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            let mut p = Parser::new();
            p.set_language(&lang.ts_language())?;
            *slot = Some(p);
        }
        let parser = slot.as_mut().expect("initialized above");
        f(parser)
    })
}
