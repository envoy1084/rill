//! Stored contents are lz4 blocks. A candidate's content is decompressed
//! on every query that verifies it, and source files are small: zstd
//! rebuilt its entropy tables for each one and spent most of a query's
//! time doing so, where lz4 decodes with no setup.

use crate::error::{Error, Result};

/// Compress one document's content.
pub fn compress(content: &[u8]) -> Result<Vec<u8>> {
    Ok(lz4_flex::block::compress(content))
}

/// Decompress one document's content of `len` bytes into `out`.
pub fn decompress_into(raw: &[u8], len: usize, out: &mut Vec<u8>) -> Result<()> {
    out.resize(len, 0);
    let n = lz4_flex::block::decompress_into(raw, out)
        .map_err(|e| Error::Codec(format!("content: {e}")))?;
    out.truncate(n);
    Ok(())
}

/// Decompress one document's content of `len` bytes.
pub fn decompress(raw: &[u8], len: usize) -> Result<Vec<u8>> {
    lz4_flex::block::decompress(raw, len).map_err(|e| Error::Codec(format!("content: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contents_round_trip() {
        for text in ["", "a", "hello world hello world", &"x".repeat(100_000)] {
            let packed = compress(text.as_bytes()).unwrap();
            assert_eq!(decompress(&packed, text.len()).unwrap(), text.as_bytes());
        }
        assert!(decompress(b"\xff\xff\xff", 10).is_err());
    }
}
