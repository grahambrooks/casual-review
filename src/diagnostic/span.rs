use serde::{Deserialize, Serialize};
use std::ops::Range;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Span {
    pub file: PathBuf,
    pub byte_range: Range<usize>,
    pub line_start: u32,
    pub col_start: u32,
    pub line_end: u32,
    pub col_end: u32,
}

impl Span {
    pub fn from_byte_range(file: PathBuf, source: &str, byte_range: Range<usize>) -> Self {
        let (line_start, col_start) = line_col(source, byte_range.start);
        let (line_end, col_end) = line_col(source, byte_range.end);
        Self {
            file,
            byte_range,
            line_start,
            col_start,
            line_end,
            col_end,
        }
    }
}

fn line_col(source: &str, byte_offset: usize) -> (u32, u32) {
    let clamped = byte_offset.min(source.len());
    let prefix = &source.as_bytes()[..clamped];
    let line = prefix.iter().filter(|&&b| b == b'\n').count() as u32 + 1;
    let last_newline = prefix.iter().rposition(|&b| b == b'\n');
    let col_bytes = match last_newline {
        Some(idx) => clamped - idx - 1,
        None => clamped,
    };
    (line, col_bytes as u32 + 1)
}
