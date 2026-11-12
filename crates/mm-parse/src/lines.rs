//! Byte offsets to (line, character column) and back.
//!
//! Lines start after each `\n`, `\r\n` or lone `\r`, as CPython's
//! positions count them (it translates line endings before tokenizing);
//! columns count characters, not bytes.

use alloc::vec::Vec;
use mm_compile::ast::Pos;

/// Line starts and which lines are pure ASCII (also used by the `ast`
/// module to report byte columns).
pub struct Lines<'s> {
    src: &'s str,
    starts: Vec<u32>,
    ascii: Vec<bool>,
    /// For a source with a non-ASCII line: the chars before each multiple
    /// of 256 bytes, so a column is a count over at most 256 bytes, not
    /// over the line (quadratic in `ast`/`tokenize` for a long line).
    marks: Vec<u32>,
}

/// Chars starting in `b` (bytes that are not `10xxxxxx`), 8 at a time.
fn chars_in(b: &[u8]) -> usize {
    let (words, rest) = b.as_chunks::<8>();
    let mut n = 0;
    for w in words {
        let x = u64::from_le_bytes(*w);
        n += 8 - (x & !(x << 1) & 0x8080_8080_8080_8080).count_ones() as usize;
    }
    n + rest.iter().filter(|&&c| c & 0xC0 != 0x80).count()
}

impl<'s> Lines<'s> {
    /// Indexes the lines of `src`.
    #[must_use]
    pub fn new(src: &'s str) -> Lines<'s> {
        let b = src.as_bytes();
        let mut starts = Vec::with_capacity(b.len() / 32 + 1);
        let mut ascii = Vec::with_capacity(b.len() / 32 + 1);
        starts.push(0);
        let mut line_ascii = true;
        for (i, &c) in b.iter().enumerate() {
            if c == b'\n' || (c == b'\r' && b.get(i + 1) != Some(&b'\n')) {
                ascii.push(line_ascii);
                line_ascii = true;
                starts.push(u32::try_from(i + 1).unwrap_or(u32::MAX));
            } else if c >= 0x80 {
                line_ascii = false;
            }
        }
        ascii.push(line_ascii);
        let mut marks = Vec::new();
        if ascii.contains(&false) {
            let mut n = 0u32;
            for chunk in b.chunks(256) {
                marks.push(n);
                n += u32::try_from(chars_in(chunk)).unwrap_or(0);
            }
            marks.push(n);
        }
        Lines {
            src,
            starts,
            ascii,
            marks,
        }
    }

    /// Chars before byte `x` (a char boundary) of a non-ASCII source.
    fn chars_to(&self, x: usize) -> usize {
        let k = x >> 8;
        let base = self.marks.get(k).map_or(0, |&m| m as usize);
        let tail = self.src.as_bytes().get(k << 8..x).unwrap_or(&[]);
        base + chars_in(tail)
    }

    /// Byte offset of char `n` of a non-ASCII source (its length if none).
    fn char_at(&self, n: usize) -> usize {
        let k = self
            .marks
            .partition_point(|&m| m as usize <= n)
            .saturating_sub(1);
        let mut seen = self.marks.get(k).map_or(0, |&m| m as usize);
        let from = (k << 8).min(self.src.len());
        for (i, &c) in self.src.as_bytes()[from..].iter().enumerate() {
            if c & 0xC0 != 0x80 {
                if seen == n {
                    return from + i;
                }
                seen += 1;
            }
        }
        self.src.len()
    }

    /// The byte offset where 1-based line `line` starts.
    #[must_use]
    pub fn start(&self, line: u32) -> usize {
        self.starts
            .get((line as usize).wrapping_sub(1))
            .map_or(self.src.len(), |&s| s as usize)
    }

    /// The 1-based line containing byte offset `off`.
    pub(crate) fn line_of(&self, off: u32) -> usize {
        self.starts.partition_point(|&s| s <= off).max(1)
    }

    /// (line, character column) of byte offset `off`.
    #[must_use]
    #[inline(never)]
    pub fn loc(&self, off: u32) -> (u32, u32) {
        let line = self.line_of(off);
        let start = self.starts[line - 1] as usize;
        let off = (off as usize).min(self.src.len());
        let col = if self.ascii[line - 1] {
            off - start
        } else if self.src.is_char_boundary(off) {
            self.chars_to(off) - self.chars_to(start)
        } else {
            0
        };
        (
            u32::try_from(line).unwrap_or(u32::MAX),
            u32::try_from(col).unwrap_or(u32::MAX),
        )
    }

    /// The range `start..end` as a [`Pos`].
    #[inline(never)]
    pub(crate) fn pos(&self, start: u32, end: u32) -> Pos {
        let (line, col) = self.loc(start);
        let (end_line, end_col) = self.loc(end);
        Pos {
            line,
            col,
            end_line,
            end_col,
        }
    }

    /// Byte offset of (line, character column).
    #[must_use]
    #[inline(never)]
    pub fn offset(&self, line: u32, col: u32) -> usize {
        let Some(&start) = self.starts.get((line as usize).wrapping_sub(1)) else {
            return self.src.len();
        };
        let start = start as usize;
        if self.ascii[line as usize - 1] {
            return (start + col as usize).min(self.src.len());
        }
        self.char_at(self.chars_to(start).saturating_add(col as usize))
    }

    /// The text of 1-based line `line`, without its line break (one
    /// byte, or `\r\n`).
    #[must_use]
    pub fn text(&self, line: usize) -> &'s str {
        let Some(&start) = self.starts.get(line.wrapping_sub(1)) else {
            return "";
        };
        let end = self
            .starts
            .get(line)
            .map_or(self.src.len(), |&e| e as usize - 1);
        let t = self.src.get(start as usize..end).unwrap_or("");
        t.strip_suffix('\r').unwrap_or(t)
    }
}
