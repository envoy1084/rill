//! Line tokenization shared by the diff and merge algorithms.
//!
//! Every token is one line *including* its terminator (`\n` or `\r\n`), so
//! a change of line endings is a real change and the concatenation of the
//! tokens reproduces the input byte for byte. The last line may have no
//! terminator; [`LineTokens::missing_final_newline`] reports that so
//! renderers can emit the usual `\ No newline at end of file` marker.

use imara_diff::TokenSource;

/// The lines of a byte buffer, each with its terminator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineTokens<'a> {
    lines: Vec<&'a [u8]>,
}

impl<'a> LineTokens<'a> {
    /// Tokenize `data` into lines.
    pub fn new(data: &'a [u8]) -> Self {
        LineTokens {
            lines: split_lines(data).collect(),
        }
    }

    /// The lines, terminators included.
    pub fn lines(&self) -> &[&'a [u8]] {
        &self.lines
    }

    /// Number of lines.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// True when the input was empty.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// True when the last line has no `\n` terminator.
    pub fn missing_final_newline(&self) -> bool {
        self.lines.last().is_some_and(|l| !l.ends_with(b"\n"))
    }
}

/// Split `data` into lines, each including its terminator.
pub fn split_lines(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut rest = data;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let end = rest
            .iter()
            .position(|&b| b == b'\n')
            .map_or(rest.len(), |i| i + 1);
        let (line, tail) = rest.split_at(end);
        rest = tail;
        Some(line)
    })
}

/// Strip the terminator (`\n` or `\r\n`) from a line token.
pub fn line_content(line: &[u8]) -> &[u8] {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    line.strip_suffix(b"\r").unwrap_or(line)
}

/// A line token source for `imara-diff`, yielding borrowed lines.
#[derive(Clone, Copy)]
pub(crate) struct LineSource<'a>(pub(crate) &'a [&'a [u8]]);

impl<'a> TokenSource for LineSource<'a> {
    type Token = &'a [u8];
    type Tokenizer = std::iter::Copied<std::slice::Iter<'a, &'a [u8]>>;

    fn tokenize(&self) -> Self::Tokenizer {
        self.0.iter().copied()
    }

    fn estimate_tokens(&self) -> u32 {
        self.0.len() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_bytes() {
        for input in [
            &b"a\nb\n"[..],
            b"a\r\nb",
            b"",
            b"\n",
            b"no newline",
            b"\r\n\r\n",
        ] {
            let joined: Vec<u8> = split_lines(input).flat_map(|l| l.to_vec()).collect();
            assert_eq!(joined, input);
        }
    }

    #[test]
    fn detects_missing_newline() {
        assert!(LineTokens::new(b"a\nb").missing_final_newline());
        assert!(!LineTokens::new(b"a\nb\n").missing_final_newline());
        assert!(!LineTokens::new(b"").missing_final_newline());
    }

    #[test]
    fn strips_crlf() {
        assert_eq!(line_content(b"x\r\n"), b"x");
        assert_eq!(line_content(b"x\n"), b"x");
        assert_eq!(line_content(b"x"), b"x");
        assert_eq!(LineTokens::new(b"a\r\nb\r\n").len(), 2);
    }
}
