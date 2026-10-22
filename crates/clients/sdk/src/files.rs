//! Replacing a local file atomically and durably.

use std::io::Write;
use std::path::Path;

/// Replace `path` with `bytes`: written to a uniquely named temporary file
/// beside it, flushed to disk, then renamed over it, so a reader or a
/// concurrent writer never sees a partial file and a crash keeps one whole
/// version. `mode` sets Unix permissions on the new file.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8], mode: Option<u32>) -> std::io::Result<()> {
    let dir = match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };
    let mut builder = tempfile::Builder::new();
    builder.prefix(".rill-tmp-");
    #[cfg(unix)]
    if let Some(mode) = mode {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(mode));
    }
    #[cfg(not(unix))]
    let _ = mode;
    let mut file = builder.tempfile_in(dir)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_writers_never_leave_a_partial_or_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.toml");
        let payloads: Vec<Vec<u8>> = (0..8u8).map(|i| vec![b'a' + i; 64 * 1024]).collect();
        std::thread::scope(|scope| {
            for payload in &payloads {
                let path = path.clone();
                scope.spawn(move || {
                    for _ in 0..25 {
                        write_atomic(&path, payload, Some(0o600)).unwrap();
                    }
                });
            }
        });
        let written = std::fs::read(&path).unwrap();
        assert!(payloads.contains(&written), "a whole payload, not a mix");
        let leftovers = std::fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(leftovers, 1, "no temporary files left behind");
    }
}
