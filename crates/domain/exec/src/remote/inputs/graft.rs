//! Adding directories to an input root that its filter left out.

use std::future::Future;
use std::pin::Pin;

use bytes::Bytes;
use prost::Message;
use rill_reapi_proto::reapi::{Directory, DirectoryNode};
use rill_types::RepoPath;

use crate::error::{Error, Result};
use crate::remote::digest::BlobDigest;
use crate::remote::space::ExecSpace;

impl ExecSpace {
    /// `root` with the directory `path` present, created empty along with
    /// its parents when missing. A step's working directory must exist
    /// even when none of its inputs live there.
    pub async fn with_directory(&self, root: BlobDigest, path: &RepoPath) -> Result<BlobDigest> {
        let names: Vec<String> = path.components().map(str::to_string).collect();
        self.graft(root, names).await
    }

    fn graft(
        &self,
        root: BlobDigest,
        mut names: Vec<String>,
    ) -> Pin<Box<dyn Future<Output = Result<BlobDigest>> + Send + '_>> {
        Box::pin(async move {
            if names.is_empty() {
                return Ok(root);
            }
            let name = names.remove(0);
            let bytes = self
                .read(&root)
                .await?
                .ok_or_else(|| Error::MissingBlob(root.to_string()))?;
            let mut dir = Directory::decode(bytes)
                .map_err(|e| Error::Codec(format!("directory {root}: {e}")))?;
            if dir.files.iter().any(|f| f.name == name)
                || dir.symlinks.iter().any(|s| s.name == name)
            {
                return Err(Error::InvalidAction(format!(
                    "{name:?} is a file, not a directory"
                )));
            }
            let position = dir
                .directories
                .binary_search_by(|d| d.name.as_str().cmp(&name));
            let child = match position {
                Ok(at) => BlobDigest::parse(
                    dir.directories[at]
                        .digest
                        .as_ref()
                        .ok_or_else(|| Error::InvalidDigest(name.clone()))?,
                )?,
                Err(_) => BlobDigest::of(&[]),
            };
            let node = DirectoryNode {
                name,
                digest: Some(self.graft(child, names).await?.to_proto()),
            };
            match position {
                Ok(at) => dir.directories[at] = node,
                Err(at) => dir.directories.insert(at, node),
            }
            let bytes = Bytes::from(dir.encode_to_vec());
            let digest = BlobDigest::of(&bytes);
            self.write(&digest, bytes).await?;
            Ok(digest)
        })
    }
}
