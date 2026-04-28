pub mod any_type;
pub mod api_surface_change;
pub mod bare_except;
pub mod debug_print;
pub mod disabled_test;
pub mod empty_catch;
pub mod hardcoded_secret;
pub mod large_function;
pub mod parse_error;
pub mod todo_marker;
pub mod trailing_whitespace;
pub mod ts_escape_hatch;
pub mod unwrap_used;

use crate::diagnostic::Diagnostic;
use crate::parse::Language;
use std::ops::Range;
use std::path::Path;
use tree_sitter::Tree;

pub struct RuleCtx<'a> {
    pub path: &'a Path,
    pub source: &'a str,
    pub tree: Option<&'a Tree>,
    pub language: Option<Language>,
    pub changed_lines: Option<&'a [Range<u32>]>,
    pub old_source: Option<&'a str>,
    pub old_tree: Option<&'a Tree>,
}

impl<'a> RuleCtx<'a> {
    pub fn line_in_changes(&self, line_1based: u32) -> bool {
        match self.changed_lines {
            None => true,
            Some(ranges) => ranges.iter().any(|r| line_1based >= r.start && line_1based < r.end),
        }
    }
}

pub trait Rule: Send + Sync {
    fn id(&self) -> &'static str;
    fn run(&self, ctx: &RuleCtx<'_>) -> Vec<Diagnostic>;
}

pub fn default_rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(parse_error::ParseErrorRule),
        Box::new(todo_marker::TodoMarkerRule),
        Box::new(trailing_whitespace::TrailingWhitespaceRule),
        Box::new(large_function::LargeFunctionRule),
        Box::new(debug_print::DebugPrintRule),
        Box::new(unwrap_used::UnwrapUsedRule),
        Box::new(any_type::AnyTypeRule),
        Box::new(bare_except::BareExceptRule),
        Box::new(empty_catch::EmptyCatchRule),
        Box::new(disabled_test::DisabledTestRule),
        Box::new(ts_escape_hatch::TsEscapeHatchRule),
        Box::new(hardcoded_secret::HardcodedSecretRule),
        Box::new(api_surface_change::ApiSurfaceChangeRule),
    ]
}
