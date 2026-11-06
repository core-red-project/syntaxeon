//! Byte offsets of the compiler against line/character positions of the protocol, where a
//! character is a UTF-16 code unit.

use lsp_types::{Position, Range};
use syntaxeon_core::Span;

pub struct LineIndex<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    #[must_use]
    pub fn new(text: &'a str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        LineIndex { text, starts }
    }

    #[must_use]
    pub fn position(&self, offset: usize) -> Position {
        let offset = offset.min(self.text.len());
        let line = self.starts.partition_point(|&s| s <= offset) - 1;
        let units = self.text[self.starts[line]..offset].encode_utf16().count();
        Position::new(to_u32(line), to_u32(units))
    }

    #[must_use]
    pub fn offset(&self, p: Position) -> usize {
        let Some(&start) = self.starts.get(p.line as usize) else {
            return self.text.len();
        };
        let mut units = 0;
        for (i, c) in self.text[start..].char_indices() {
            if units >= p.character as usize || c == '\n' {
                return start + i;
            }
            units += c.len_utf16();
        }
        self.text.len()
    }

    #[must_use]
    pub fn range(&self, span: Span) -> Range {
        Range::new(self.position(span.start), self.position(span.end))
    }

    /// The whole of a 1-based source line, for errors that only know the line.
    #[must_use]
    pub fn line_range(&self, line: usize) -> Range {
        let i = line.saturating_sub(1).min(self.starts.len() - 1);
        let start = self.starts[i];
        let end = self.text[start..]
            .find('\n')
            .map_or(self.text.len(), |n| start + n);
        self.range(Span { start, end })
    }
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_round_trip_through_positions() {
        let text = "ab\nc é 😀 d\n\nlast";
        let ix = LineIndex::new(text);
        for (i, _) in text.char_indices() {
            assert_eq!(ix.offset(ix.position(i)), i, "offset {i}");
        }
        assert_eq!(ix.position(text.len()), Position::new(3, 4));
    }

    #[test]
    fn characters_count_utf16_units() {
        let text = "x😀y";
        let ix = LineIndex::new(text);
        // The emoji is two UTF-16 units, so `y` is at character 3, not 2.
        assert_eq!(ix.position(text.find('y').unwrap()), Position::new(0, 3));
        assert_eq!(ix.offset(Position::new(0, 3)), text.find('y').unwrap());
        // A position inside a surrogate pair rounds to the next character boundary.
        assert_eq!(ix.offset(Position::new(0, 2)), text.find('y').unwrap());
    }

    #[test]
    fn ranges_and_line_ranges() {
        let ix = LineIndex::new("one\ntwo three\nfour");
        assert_eq!(
            ix.range(Span { start: 8, end: 13 }),
            Range::new(Position::new(1, 4), Position::new(1, 9))
        );
        assert_eq!(
            ix.line_range(2),
            Range::new(Position::new(1, 0), Position::new(1, 9))
        );
        assert_eq!(
            ix.line_range(9).start.line,
            2,
            "past the end clamps to the last line"
        );
        assert_eq!(ix.offset(Position::new(9, 0)), 18);
    }
}
