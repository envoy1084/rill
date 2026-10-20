/// Number of leading bytes inspected by [`is_binary`].
pub const BINARY_SNIFF_LEN: usize = 8 * 1024;

/// Heuristic binary detection: a NUL byte within the first 8 KiB.
///
/// This matches what git does and is cheap enough to run on every merge or
/// diff request; text files with embedded NULs are vanishingly rare.
pub fn is_binary(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(BINARY_SNIFF_LEN)];
    head.contains(&0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_nul_in_head() {
        assert!(is_binary(b"abc\0def"));
        assert!(!is_binary(b"plain text\n"));
        assert!(!is_binary(b""));
    }

    #[test]
    fn ignores_nul_after_sniff_window() {
        let mut data = vec![b'a'; BINARY_SNIFF_LEN];
        data.push(0);
        assert!(!is_binary(&data));
        data[BINARY_SNIFF_LEN - 1] = 0;
        assert!(is_binary(&data));
    }
}
