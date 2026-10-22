//! On-disk / on-wire framing of object bytes shared by the persistent
//! object stores.
//!
//! Every stored object is wrapped in a small frame:
//!
//! ```text
//! +------+---------+--------+-------+-----------------+
//! | RILZ | version | method | level | payload         |
//! +------+---------+--------+-------+-----------------+
//!   4 B     1 B       1 B     1 B     rest of the file
//! ```
//!
//! `method` is [`Method::Raw`] (payload is the object bytes verbatim) or
//! [`Method::Zstd`] (payload is a zstd frame). `level` records the zstd level
//! used so operators can tell old and new encodings apart; decoding does not
//! need it. Objects smaller than [`RAW_THRESHOLD`] bytes and objects that do
//! not shrink under compression are stored raw.

use crate::error::{Error, Result};

/// Magic bytes at the start of every frame.
pub const MAGIC: [u8; 4] = *b"RILZ";
/// Frame layout version. Bump when the header changes shape.
pub const VERSION: u8 = 1;
/// Total header length in bytes.
pub const HEADER_LEN: usize = 7;
/// Objects shorter than this are never compressed: the zstd frame overhead
/// would exceed any saving.
pub const RAW_THRESHOLD: usize = 64;
/// Default zstd compression level. Level 3 is zstd's own default and is the
/// best speed/ratio trade-off for source text and small structured objects.
pub const DEFAULT_LEVEL: i32 = 3;

/// How the payload of a frame is encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Method {
    /// Payload is the object bytes unchanged.
    Raw = 0,
    /// Payload is a single zstd frame.
    Zstd = 1,
}

impl Method {
    fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Method::Raw),
            1 => Some(Method::Zstd),
            _ => None,
        }
    }
}

/// Frame `bytes` with the default compression level.
pub fn compress(bytes: &[u8]) -> Vec<u8> {
    compress_with_level(bytes, DEFAULT_LEVEL)
}

/// Frame `bytes`, compressing with the given zstd level when it pays off.
pub fn compress_with_level(bytes: &[u8], level: i32) -> Vec<u8> {
    if bytes.len() >= RAW_THRESHOLD {
        // zstd only fails on allocation or an invalid level; fall back to raw
        // rather than surfacing an error for a pure optimisation.
        if let Ok(compressed) = zstd::bulk::compress(bytes, level) {
            if compressed.len() < bytes.len() {
                return frame(Method::Zstd, level, &compressed);
            }
        }
    }
    frame(Method::Raw, 0, bytes)
}

/// Decode a frame produced by [`compress`] back into the object bytes.
pub fn decompress(frame: &[u8]) -> Result<Vec<u8>> {
    let (method, payload) = parse_header(frame)?;
    match method {
        Method::Raw => Ok(payload.to_vec()),
        Method::Zstd => {
            zstd::stream::decode_all(payload).map_err(|e| Error::Codec(format!("zstd: {e}")))
        }
    }
}

/// Inspect a frame header without decoding the payload.
pub fn parse_header(frame: &[u8]) -> Result<(Method, &[u8])> {
    if frame.len() < HEADER_LEN {
        return Err(Error::Codec("object frame too short".into()));
    }
    if frame[..4] != MAGIC {
        return Err(Error::Codec("bad object frame magic".into()));
    }
    if frame[4] != VERSION {
        return Err(Error::Codec(format!(
            "unsupported object frame version {}",
            frame[4]
        )));
    }
    let method = Method::from_u8(frame[5])
        .ok_or_else(|| Error::Codec(format!("unknown object frame method {}", frame[5])))?;
    Ok((method, &frame[HEADER_LEN..]))
}

fn frame(method: Method, level: i32, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(&MAGIC);
    out.push(VERSION);
    out.push(method as u8);
    // Levels are small positive integers (zstd caps at 22); clamp defensively
    // so a negative "fast" level still fits the byte.
    out.push(level.clamp(0, u8::MAX as i32) as u8);
    out.extend_from_slice(payload);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_objects_stay_raw() {
        let data = b"tiny";
        let framed = compress(data);
        let (method, payload) = parse_header(&framed).unwrap();
        assert_eq!(method, Method::Raw);
        assert_eq!(payload, data);
        assert_eq!(decompress(&framed).unwrap(), data);
    }

    #[test]
    fn compressible_objects_are_zstd() {
        let data = vec![b'a'; 4096];
        let framed = compress(&data);
        let (method, _) = parse_header(&framed).unwrap();
        assert_eq!(method, Method::Zstd);
        assert!(framed.len() < data.len());
        assert_eq!(framed[6], DEFAULT_LEVEL as u8);
        assert_eq!(decompress(&framed).unwrap(), data);
    }

    #[test]
    fn incompressible_objects_fall_back_to_raw() {
        // A simple LCG gives bytes zstd cannot shrink.
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        let data: Vec<u8> = (0..1024)
            .map(|_| {
                x = x
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                (x >> 56) as u8
            })
            .collect();
        let framed = compress(&data);
        assert_eq!(parse_header(&framed).unwrap().0, Method::Raw);
        assert_eq!(decompress(&framed).unwrap(), data);
    }

    #[test]
    fn empty_roundtrips() {
        assert_eq!(decompress(&compress(b"")).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn rejects_bad_frames() {
        assert!(matches!(decompress(b"RIL"), Err(Error::Codec(_))));
        assert!(matches!(
            decompress(b"NOPE\x01\x00\x00"),
            Err(Error::Codec(_))
        ));
        assert!(matches!(
            decompress(b"RILZ\x02\x00\x00"),
            Err(Error::Codec(_))
        ));
        assert!(matches!(
            decompress(b"RILZ\x01\x09\x00"),
            Err(Error::Codec(_))
        ));
        assert!(matches!(
            decompress(b"RILZ\x01\x01\x03not zstd"),
            Err(Error::Codec(_))
        ));
    }
}
