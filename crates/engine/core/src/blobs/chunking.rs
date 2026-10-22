//! FastCDC chunk sizes for content split into a manifest.

/// FastCDC minimum chunk size.
pub const CHUNK_MIN: usize = 16 * 1024;
/// FastCDC average (target) chunk size.
pub const CHUNK_AVG: usize = 64 * 1024;
/// FastCDC maximum chunk size.
pub const CHUNK_MAX: usize = 256 * 1024;
