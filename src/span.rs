//! Source positions: `FileId` plus byte-offset `Span`, threaded
//! through every IR node so diagnostics and IDE features point at real
//! file/line/column coordinates. Ingest stamps spans and registers
//! each file as a `SourceFile` in `App::sources` (1-based `FileId`
//! indexing, with `FileId(0)` the synthetic sentinel); lowerers
//! backfill synthesized nodes via `Expr::inherit_span`; diagnostic
//! printers resolve offsets with `SourceFile::line_col`. For
//! `.html.erb` views the stored text is the raw template, not the
//! compiled Ruby — view ingest translates compiled-Ruby offsets back
//! to template coordinates, so a span there lands in the ERB the
//! author actually wrote.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn synthetic() -> Self {
        Span { file: FileId(0), start: 0, end: 0 }
    }

    pub fn is_synthetic(&self) -> bool {
        self.file.0 == 0 && self.start == 0 && self.end == 0
    }

    pub fn len(&self) -> u32 {
        self.end - self.start
    }
}

impl Default for Span {
    fn default() -> Self {
        Self::synthetic()
    }
}

/// One source file captured at ingest. `FileId(n)` (1-based) indexes
/// entry `n - 1` of `App::sources`; `FileId(0)` is the synthetic
/// sentinel and maps to no file. `text` is the text spans index —
/// the text prism parsed for plain Ruby, and the raw template for
/// `.html.erb` views (view ingest translates compiled-Ruby spans
/// back to template offsets via `erb::translate_spans`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: String,
    pub text: String,
}

impl SourceFile {
    /// 1-based (line, column) of a byte offset, by newline count.
    /// Offsets past the end clamp to the last position, and an offset
    /// that lands mid-character (a mistranslated span — roundhouse#1048)
    /// snaps back to that character's start — a bad offset shouldn't
    /// panic a diagnostics printer. (Not `str::floor_char_boundary`:
    /// MSRV is 1.89, which predates its stabilization.)
    pub fn line_col(&self, offset: u32) -> (u32, u32) {
        let offset = self.char_start(offset);
        let before = &self.text.as_bytes()[..offset];
        let line = before.iter().filter(|&&b| b == b'\n').count() as u32 + 1;
        let line_start = before
            .iter()
            .rposition(|&b| b == b'\n')
            .map(|p| p + 1)
            .unwrap_or(0);
        (line, self.column(line_start, offset))
    }

    /// This file with where each of its lines starts, for many
    /// `line_col` lookups: [`LineIndex::line_col`] answers the same by a
    /// binary search, where `line_col` counts the newlines before the
    /// offset on every call (a source map asks once per mapped node,
    /// which on a large file is quadratic).
    pub fn line_index(&self) -> LineIndex<'_> {
        let after_newlines =
            self.text.bytes().enumerate().filter(|&(_, b)| b == b'\n').map(|(i, _)| i + 1);
        LineIndex { file: self, starts: std::iter::once(0).chain(after_newlines).collect() }
    }

    /// The offset clamped to the text and snapped back to the start of
    /// the character it lands in.
    fn char_start(&self, offset: u32) -> usize {
        let mut offset = (offset as usize).min(self.text.len());
        while offset > 0 && !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }

    /// 1-based column of `offset` on the line starting at `line_start`.
    /// Column counts characters, not bytes, so editors that treat col as
    /// a character offset land on the right spot in UTF-8.
    fn column(&self, line_start: usize, offset: usize) -> u32 {
        self.text[line_start..offset].chars().count() as u32 + 1
    }
}

/// A [`SourceFile`] with the byte offset where each of its lines starts
/// (the first is 0, then one after each newline), from
/// [`SourceFile::line_index`]. It borrows its file, so an index is never
/// read against another file's text.
#[derive(Clone, Debug)]
pub struct LineIndex<'a> {
    file: &'a SourceFile,
    starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    /// The file this index is of.
    pub fn file(&self) -> &'a SourceFile {
        self.file
    }

    /// [`SourceFile::line_col`] of the same offset, by a binary search.
    pub fn line_col(&self, offset: u32) -> (u32, u32) {
        let offset = self.file.char_start(offset);
        let line = self.starts.partition_point(|&start| start <= offset);
        (line as u32, self.file.column(self.starts[line - 1], offset))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_col_is_one_based_and_newline_aware() {
        let f = SourceFile { path: "a.rb".into(), text: "ab\ncd\nef".into() };
        assert_eq!(f.line_col(0), (1, 1));
        assert_eq!(f.line_col(1), (1, 2));
        assert_eq!(f.line_col(3), (2, 1));
        assert_eq!(f.line_col(7), (3, 2));
    }

    #[test]
    fn line_col_clamps_past_end() {
        let f = SourceFile { path: "a.rb".into(), text: "ab\n".into() };
        assert_eq!(f.line_col(99), (2, 1));
    }

    #[test]
    fn line_col_counts_chars_not_bytes() {
        let f = SourceFile { path: "a.rb".into(), text: "é = 1".into() };
        // "é" is two bytes; the `=` at byte offset 3 is character column 3.
        assert_eq!(f.line_col(3), (1, 3));
    }

    /// roundhouse#1048: a diagnostic span can carry an offset that lands
    /// mid-character (e.g. a mistranslated template offset from a HAML
    /// continuation). One bad offset must snap back to its char's start
    /// rather than panicking the whole run on `text[..offset]`.
    #[test]
    fn line_col_snaps_a_mid_character_offset_back_to_its_char_boundary() {
        // "é" is a 2-byte char at bytes 0..2; byte 1 lands inside it.
        let f = SourceFile { path: "a.rb".into(), text: "é = 1".into() };
        assert!(!f.text.is_char_boundary(1), "test setup: byte 1 must be mid-character");
        assert_eq!(f.line_col(1), f.line_col(0));
    }

    /// The line index answers `line_col` for every offset: at line
    /// starts and ends, inside multi-byte characters (four bytes too),
    /// on empty lines, with `\r\n` and a lone `\r`, without a final
    /// newline, and past the end.
    #[test]
    fn line_index_answers_as_line_col_for_every_offset() {
        let texts = ["", "\n", "\n\n", "ab\ncd\nef", "ab\n", "é\nàx\n\nü", "a\r\nb\r\n", "日本\n語x", "x", "😀a\n\r😀\nb"];
        for text in texts {
            let f = SourceFile { path: "a.rb".into(), text: text.into() };
            let index = f.line_index();
            for offset in 0..text.len() as u32 + 3 {
                assert_eq!(index.line_col(offset), f.line_col(offset), "{text:?} at {offset}");
            }
        }
    }
}
