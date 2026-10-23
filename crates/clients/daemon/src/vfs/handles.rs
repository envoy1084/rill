//! Open files and directories. Reads go to the cached object (or bytes
//! held in memory); writes go to a private scratch copy that becomes an
//! overlay entry when the file is flushed.

use std::fs::File;
use std::os::unix::fs::FileExt;
use std::path::PathBuf;

use bytes::Bytes;
use rill_types::{FileKind, RepoPathBuf};

use crate::error::Result;

/// An open file or directory.
#[derive(Debug)]
pub enum Handle {
    /// Read-only, from a cached object.
    ReadFile(File),
    /// Read-only, from bytes in memory (inline content, the marker).
    ReadBytes(Bytes),
    /// Writable: a scratch copy of the file.
    Write(Scratch),
    /// A directory listing taken at open (FUSE).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Dir(Vec<(u64, bool, String)>),
}

/// A writable copy of a file.
#[derive(Debug)]
pub struct Scratch {
    pub path: RepoPathBuf,
    pub kind: FileKind,
    pub file: File,
    pub location: PathBuf,
    /// Written since the last commit.
    pub dirty: bool,
}

impl Scratch {
    /// A scratch file in `dir` holding `initial`.
    pub fn create(
        dir: &std::path::Path,
        path: RepoPathBuf,
        kind: FileKind,
        initial: &[u8],
    ) -> Result<Scratch> {
        std::fs::create_dir_all(dir)?;
        let location = dir.join(uuid::Uuid::new_v4().simple().to_string());
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&location)?;
        file.write_all_at(initial, 0)?;
        Ok(Scratch {
            path,
            kind,
            file,
            location,
            dirty: false,
        })
    }

    /// The scratch file's bytes.
    pub fn contents(&self) -> Result<Bytes> {
        Ok(Bytes::from(std::fs::read(&self.location)?))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.location);
    }
}

/// Read `size` bytes at `offset` from a file.
pub fn read_at(file: &File, offset: u64, size: u32) -> Result<Vec<u8>> {
    let mut buf = vec![0; size as usize];
    let mut filled = 0;
    while filled < buf.len() {
        let n = file.read_at(&mut buf[filled..], offset + filled as u64)?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    buf.truncate(filled);
    Ok(buf)
}

/// A slice of `bytes` as a read at `offset` returns it.
pub fn slice(bytes: &Bytes, offset: u64, size: u32) -> Vec<u8> {
    let start = (offset as usize).min(bytes.len());
    let end = start.saturating_add(size as usize).min(bytes.len());
    bytes[start..end].to_vec()
}
