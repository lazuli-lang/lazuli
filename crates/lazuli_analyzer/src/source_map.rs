//! SourceMap resolver — maps `SpanRef` byte offsets to (file, line, column).
//!
//! Companion to `lazuli_ir::SourceMap`. Built from raw source during
//! parse + analyzer pipeline; consumed by codegen for `//line`
//! directives (D4) and `WithSource` context injection (D5).

use lazuli_ir::{FileId, SourceFile, SourceMap, SpanRef};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLoc {
    pub file: String,
    /// 1-based line number.
    pub line: u32,
    /// 1-based byte column.
    pub column: u32,
}

pub trait SourceMapResolver {
    /// Build a SourceFile entry from raw source text.
    /// Caller assigns a FileId.
    fn build_source_file(id: FileId, path: impl Into<String>, source: &str) -> SourceFile;

    /// Resolve a span's start position within a given file to (line, column).
    /// Returns None if file_id is unknown or span.start exceeds file bytes.
    fn resolve(&self, file_id: FileId, span: SpanRef) -> Option<ResolvedLoc>;
}

impl SourceMapResolver for SourceMap {
    fn build_source_file(id: FileId, path: impl Into<String>, source: &str) -> SourceFile {
        let mut offsets = vec![0u32];
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                offsets.push((i + 1) as u32);
            }
        }

        let source_len = u32::try_from(source.len()).expect("source file length exceeds u32::MAX");
        if offsets.last().copied() != Some(source_len) {
            offsets.push(source_len);
        }

        SourceFile {
            id,
            path: path.into(),
            line_offsets: offsets,
        }
    }

    fn resolve(&self, file_id: FileId, span: SpanRef) -> Option<ResolvedLoc> {
        let file = self.files.iter().find(|f| f.id == file_id)?;
        let source_len = file.line_offsets.last().copied()?;
        let start = u32::try_from(span.start).ok()?;
        if start > source_len {
            return None;
        }

        let searchable_offsets = if file.line_offsets.len() > 1 {
            &file.line_offsets[..file.line_offsets.len() - 1]
        } else {
            &file.line_offsets[..]
        };

        let line_idx = match searchable_offsets.binary_search(&start) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let line_start = searchable_offsets.get(line_idx)?;
        let column = start - line_start + 1;
        Some(ResolvedLoc {
            file: file.path.clone(),
            line: (line_idx + 1) as u32,
            column,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(source: &str) -> SourceMap {
        SourceMap {
            files: vec![SourceMap::build_source_file(
                7,
                "features/customer.lzi",
                source,
            )],
        }
    }

    #[test]
    fn build_source_file_handles_empty_source() {
        let file = SourceMap::build_source_file(1, "features/empty.lzi", "");

        assert_eq!(file.id, 1);
        assert_eq!(file.path, "features/empty.lzi");
        assert_eq!(file.line_offsets, vec![0]);
    }

    #[test]
    fn build_source_file_records_line_offsets() {
        let file = SourceMap::build_source_file(1, "features/customer.lzi", "one\ntwo\nthree");

        assert_eq!(file.line_offsets, vec![0, 4, 8, 13]);
    }

    #[test]
    fn resolve_first_line_returns_line_1_column_1() {
        let loc = map("one\ntwo")
            .resolve(7, SpanRef { start: 0, end: 3 })
            .unwrap();

        assert_eq!(
            loc,
            ResolvedLoc {
                file: "features/customer.lzi".to_string(),
                line: 1,
                column: 1
            }
        );
    }

    #[test]
    fn resolve_middle_of_line_returns_correct_column() {
        let loc = map("abcdef")
            .resolve(7, SpanRef { start: 3, end: 4 })
            .unwrap();

        assert_eq!(loc.line, 1);
        assert_eq!(loc.column, 4);
    }

    #[test]
    fn resolve_start_of_subsequent_line_returns_new_line_column_1() {
        let loc = map("first\nsecond")
            .resolve(7, SpanRef { start: 6, end: 12 })
            .unwrap();

        assert_eq!(loc.line, 2);
        assert_eq!(loc.column, 1);
    }

    #[test]
    fn resolve_returns_none_for_unknown_file() {
        assert_eq!(map("one").resolve(99, SpanRef { start: 0, end: 1 }), None);
    }

    #[test]
    fn resolve_returns_none_for_span_past_end() {
        assert_eq!(map("one").resolve(7, SpanRef { start: 4, end: 5 }), None);
    }

    #[test]
    fn resolve_handles_multi_byte_utf8() {
        let source = "field nome = \"ação\"\ncommand criar";
        let loc = map(source)
            .resolve(7, SpanRef { start: 16, end: 21 })
            .unwrap();

        assert_eq!(loc.line, 1);
        assert_eq!(loc.column, 17);
    }
}
