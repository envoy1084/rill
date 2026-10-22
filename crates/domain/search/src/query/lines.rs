//! Mapping byte offsets to lines.

/// Byte offsets of every line start in a buffer, for offset-to-line
/// lookups in logarithmic time.
pub struct LineIndex<'a> {
    bytes: &'a [u8],
    starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    /// Build the index for `bytes`.
    pub fn new(bytes: &'a [u8]) -> Self {
        let mut starts = vec![0];
        starts.extend(
            bytes
                .iter()
                .enumerate()
                .filter(|(_, &b)| b == b'\n')
                .map(|(i, _)| i + 1),
        );
        // A trailing newline produces a phantom empty final line; drop it
        // so line counts match what editors show.
        if starts.len() > 1 && starts.last() == Some(&bytes.len()) {
            starts.pop();
        }
        LineIndex { bytes, starts }
    }

    /// Number of lines.
    pub fn line_count(&self) -> usize {
        self.starts.len()
    }

    /// 0-based line containing byte `offset`.
    pub fn line_of(&self, offset: usize) -> usize {
        match self.starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
    }

    /// Text of 0-based line `line`, without its terminator.
    pub fn line(&self, line: usize) -> &'a [u8] {
        let start = self.starts[line];
        let end = self
            .starts
            .get(line + 1)
            .map_or(self.bytes.len(), |&next| next);
        let raw = &self.bytes[start..end];
        let raw = raw.strip_suffix(b"\n").unwrap_or(raw);
        raw.strip_suffix(b"\r").unwrap_or(raw)
    }

    /// Lossily decoded text of 0-based line `line`.
    pub fn line_string(&self, line: usize) -> String {
        String::from_utf8_lossy(self.line(line)).into_owned()
    }

    /// Up to `n` lines before `line`, in order.
    pub fn before(&self, line: usize, n: usize) -> Vec<String> {
        (line.saturating_sub(n)..line)
            .map(|l| self.line_string(l))
            .collect()
    }

    /// Up to `n` lines after `line`, in order.
    pub fn after(&self, line: usize, n: usize) -> Vec<String> {
        (line + 1..(line + 1 + n).min(self.line_count()))
            .map(|l| self.line_string(l))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_offsets_to_lines() {
        let idx = LineIndex::new(b"one\r\ntwo\nthree\n");
        assert_eq!(idx.line_count(), 3);
        assert_eq!(idx.line_of(0), 0);
        assert_eq!(idx.line_of(4), 0);
        assert_eq!(idx.line_of(5), 1);
        assert_eq!(idx.line_of(9), 2);
        assert_eq!(idx.line(0), b"one");
        assert_eq!(idx.line_string(2), "three");
        assert_eq!(idx.before(2, 5), vec!["one", "two"]);
        assert_eq!(idx.after(0, 1), vec!["two"]);
        assert!(idx.after(2, 3).is_empty());
    }

    #[test]
    fn handles_no_trailing_newline_and_empty() {
        let idx = LineIndex::new(b"a\nb");
        assert_eq!(idx.line_count(), 2);
        assert_eq!(idx.line(1), b"b");
        let empty = LineIndex::new(b"");
        assert_eq!(empty.line_count(), 1);
        assert_eq!(empty.line(0), b"");
    }
}
