//! Adding to an input root what its filter left out: the working
//! directory, and files Rill writes for a step.

use std::future::Future;
use std::pin::Pin;

use bytes::Bytes;
use prost::Message;
use rill_reapi_proto::reapi::{Directory, DirectoryNode, FileNode};
use rill_types::RepoPath;

use crate::error::{Error, Result};
use crate::remote::digest::BlobDigest;
use crate::remote::space::ExecSpace;

impl ExecSpace {
    /// `root` with the directory `path` present, created empty along with
    /// its parents when missing. A step's working directory must exist
    /// even when none of its inputs live there.
    pub async fn with_directory(&self, root: BlobDigest, path: &RepoPath) -> Result<BlobDigest> {
        self.graft(root, components(path), None).await
    }

    /// `root` with a regular file of `bytes` at `path`, replacing any file
    /// there.
    pub async fn with_file(
        &self,
        root: BlobDigest,
        path: &RepoPath,
        bytes: Bytes,
    ) -> Result<BlobDigest> {
        let mut names = components(path);
        let name = names
            .pop()
            .ok_or_else(|| Error::InvalidAction("a file at the root path".into()))?;
        let file = FileNode {
            name,
            digest: Some(self.put(bytes).await?.to_proto()),
            is_executable: false,
            node_properties: None,
        };
        self.graft(root, names, Some(file)).await
    }

    /// `root` with directories `names` present and `file` placed in the
    /// last of them.
    fn graft(
        &self,
        root: BlobDigest,
        mut names: Vec<String>,
        file: Option<FileNode>,
    ) -> Pin<Box<dyn Future<Output = Result<BlobDigest>> + Send + '_>> {
        Box::pin(async move {
            if names.is_empty() && file.is_none() {
                return Ok(root);
            }
            let bytes = self
                .read(&root)
                .await?
                .ok_or_else(|| Error::MissingBlob(root.to_string()))?;
            let mut dir = Directory::decode(bytes)
                .map_err(|e| Error::Codec(format!("directory {root}: {e}")))?;
            if names.is_empty() {
                let file = file.unwrap_or_default();
                if dir.directories.iter().any(|d| d.name == file.name)
                    || dir.symlinks.iter().any(|s| s.name == file.name)
                {
                    return Err(Error::InvalidAction(format!(
                        "{:?} is not a regular file",
                        file.name
                    )));
                }
                match dir.files.binary_search_by(|f| f.name.cmp(&file.name)) {
                    Ok(at) => dir.files[at] = file,
                    Err(at) => dir.files.insert(at, file),
                }
            } else {
                let name = names.remove(0);
                if dir.files.iter().any(|f| f.name == name)
                    || dir.symlinks.iter().any(|s| s.name == name)
                {
                    return Err(Error::InvalidAction(format!(
                        "{name:?} is a file, not a directory"
                    )));
                }
                let position = dir.directories.binary_search_by(|d| d.name.cmp(&name));
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
                    digest: Some(self.graft(child, names, file).await?.to_proto()),
                };
                match position {
                    Ok(at) => dir.directories[at] = node,
                    Err(at) => dir.directories.insert(at, node),
                }
            }
            self.put(Bytes::from(dir.encode_to_vec())).await
        })
    }
}

fn components(path: &RepoPath) -> Vec<String> {
    path.components().map(str::to_string).collect()
}
