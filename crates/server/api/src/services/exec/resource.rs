//! ByteStream resource names for CAS blobs:
//! `{instance}/blobs/[blake3/]{hash}/{size}` to read and
//! `{instance}/uploads/{uuid}/blobs/[blake3/]{hash}/{size}[/...]` to write.

use rill_exec::remote::BlobDigest;
use rill_runtime::error::{Error, Result};

/// A parsed resource name.
#[derive(Debug, PartialEq, Eq)]
pub struct BlobResource {
    pub instance: String,
    pub digest: BlobDigest,
}

/// Parse a read resource name.
pub fn parse_read(name: &str) -> Result<BlobResource> {
    let parts: Vec<&str> = name.split('/').collect();
    let at = parts
        .iter()
        .position(|p| *p == "blobs")
        .ok_or_else(|| invalid(name))?;
    parse_blob(name, &parts[..at], &parts[at + 1..])
}

/// Parse a write resource name.
pub fn parse_write(name: &str) -> Result<BlobResource> {
    let parts: Vec<&str> = name.split('/').collect();
    let at = parts
        .iter()
        .position(|p| *p == "uploads")
        .ok_or_else(|| invalid(name))?;
    if parts.get(at + 2) != Some(&"blobs") {
        return Err(invalid(name));
    }
    parse_blob(name, &parts[..at], &parts[at + 3..])
}

fn parse_blob(name: &str, instance: &[&str], rest: &[&str]) -> Result<BlobResource> {
    let rest = match rest.first() {
        Some(&"blake3") => &rest[1..],
        Some(f) if f.len() != 64 => {
            return Err(Error::invalid(format!(
                "digest function {f:?} is not supported; use blake3"
            )))
        }
        _ => rest,
    };
    let (hash, size) = match rest {
        [hash, size, ..] => (*hash, size.parse::<i64>().map_err(|_| invalid(name))?),
        _ => return Err(invalid(name)),
    };
    Ok(BlobResource {
        instance: instance.join("/"),
        digest: BlobDigest::from_parts(hash, size).map_err(Error::from)?,
    })
}

fn invalid(name: &str) -> Error {
    Error::invalid(format!("not a blob resource name: {name:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262";

    #[test]
    fn resource_names_from_bazel_parse_with_or_without_the_function() {
        let read = parse_read(&format!("acme/team/blobs/blake3/{HASH}/42")).unwrap();
        assert_eq!(read.instance, "acme/team");
        assert_eq!(read.digest.size, 42);
        assert_eq!(
            parse_read(&format!("blobs/{HASH}/42")).unwrap().instance,
            ""
        );
        let write = parse_write(&format!(
            "acme/uploads/3f2a-uuid/blobs/blake3/{HASH}/7/meta"
        ))
        .unwrap();
        assert_eq!(write.instance, "acme");
        assert_eq!(write.digest.size, 7);
        assert!(parse_read(&format!("acme/blobs/sha256/{HASH}/42")).is_err());
        assert!(parse_write(&format!("acme/blobs/{HASH}/42")).is_err());
        assert!(parse_read("acme/blobs/nothex/1").is_err());
    }
}
