//! Storing content from a stream holds a bounded amount of it in memory,
//! however large it is. Its own test binary, so peak memory is not shared
//! with other tests.

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream;
use rill_core::store_content_stream;
use rill_store::ObjectStore;
use rill_types::{ContentRef, ObjectHash, ObjectKind};

/// Accepts every write and keeps nothing, so memory measures the
/// streaming path alone.
struct Discard;

#[async_trait]
impl ObjectStore for Discard {
    async fn get(&self, _: ObjectKind, _: &ObjectHash) -> rill_store::Result<Option<Bytes>> {
        Ok(None)
    }
    async fn has(&self, _: ObjectKind, _: &ObjectHash) -> rill_store::Result<bool> {
        Ok(false)
    }
    async fn put(&self, _: ObjectKind, _: &ObjectHash, _: Bytes) -> rill_store::Result<()> {
        Ok(())
    }
    fn namespaced(&self, _: &str) -> std::sync::Arc<dyn ObjectStore> {
        std::sync::Arc::new(Discard)
    }
}

fn peak_rss_bytes() -> u64 {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: getrusage fills the struct for RUSAGE_SELF and cannot fail.
    let usage = unsafe {
        libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr());
        usage.assume_init()
    };
    let raw = usage.ru_maxrss as u64;
    if cfg!(target_os = "macos") {
        raw
    } else {
        raw * 1024
    }
}

#[tokio::test]
async fn a_gibibyte_streams_through_bounded_memory() {
    const PART: usize = 256 * 1024;
    const TOTAL: usize = 1 << 30;
    let before = peak_rss_bytes();
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let parts = stream::iter((0..TOTAL / PART).map(move |_| {
        let mut part = Vec::with_capacity(PART);
        for _ in 0..PART / 8 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            part.extend_from_slice(&state.to_le_bytes());
        }
        Ok(Bytes::from(part))
    }));
    let stored = store_content_stream(&Discard, parts).await.unwrap();
    assert_eq!(stored.size, TOTAL as u64);
    assert!(matches!(stored.content, ContentRef::Chunked { .. }));
    let grown = peak_rss_bytes().saturating_sub(before);
    assert!(grown < 64 << 20, "peak memory grew by {} MiB", grown >> 20);
}
