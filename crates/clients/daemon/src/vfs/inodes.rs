//! Inode numbers for paths. A path keeps its inode for the life of the
//! mount, and a rename moves the inodes of everything below it. Pure.

use std::collections::HashMap;

use rill_types::{RepoPath, RepoPathBuf};

/// The root directory's inode.
pub const ROOT: u64 = 1;

/// Paths by inode and inodes by path.
#[derive(Debug)]
pub struct Inodes {
    paths: HashMap<u64, RepoPathBuf>,
    inos: HashMap<RepoPathBuf, u64>,
    next: u64,
}

impl Default for Inodes {
    fn default() -> Self {
        let root = RepoPathBuf::root();
        Inodes {
            paths: HashMap::from([(ROOT, root.clone())]),
            inos: HashMap::from([(root, ROOT)]),
            next: ROOT + 1,
        }
    }
}

impl Inodes {
    /// The inode of `path`, assigned on first use.
    pub fn ino(&mut self, path: &RepoPath) -> u64 {
        if let Some(ino) = self.inos.get(path) {
            return *ino;
        }
        let ino = self.next;
        self.next += 1;
        self.paths.insert(ino, path.to_owned_path());
        self.inos.insert(path.to_owned_path(), ino);
        ino
    }

    /// The path of `ino`.
    pub fn path(&self, ino: u64) -> Option<RepoPathBuf> {
        self.paths.get(&ino).cloned()
    }

    /// Move `from` and everything below it to `to`. Whatever had `to`'s
    /// paths loses them.
    pub fn rename(&mut self, from: &RepoPath, to: &RepoPath) {
        let moved: Vec<(RepoPathBuf, u64)> = self
            .inos
            .iter()
            .filter(|(p, _)| p.starts_with(from))
            .map(|(p, i)| (p.clone(), *i))
            .collect();
        let replaced: Vec<RepoPathBuf> = self
            .inos
            .keys()
            .filter(|p| p.starts_with(to))
            .cloned()
            .collect();
        for path in replaced {
            if let Some(ino) = self.inos.remove(&path) {
                self.paths.remove(&ino);
            }
        }
        for (path, ino) in moved {
            self.inos.remove(&path);
            let Some(rest) = path.strip_prefix(from) else {
                continue;
            };
            let mut new = to.to_owned_path();
            if rest.components().try_for_each(|c| new.push(c)).is_err() {
                self.paths.remove(&ino);
                continue;
            }
            self.paths.insert(ino, new.clone());
            self.inos.insert(new, ino);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> RepoPathBuf {
        RepoPathBuf::new(s).unwrap()
    }

    #[test]
    fn a_renamed_directory_keeps_its_and_its_childrens_inodes() {
        let mut inodes = Inodes::default();
        let dir = inodes.ino(&p("a"));
        let file = inodes.ino(&p("a/f"));
        let sibling = inodes.ino(&p("ab"));
        let target = inodes.ino(&p("b/f"));
        inodes.rename(&p("a"), &p("b"));
        assert_eq!(inodes.path(dir), Some(p("b")));
        assert_eq!(inodes.path(file), Some(p("b/f")));
        assert_eq!(inodes.path(sibling), Some(p("ab")));
        assert_eq!(
            inodes.path(target),
            None,
            "the replaced file's inode is gone"
        );
        assert_eq!(inodes.ino(&p("b/f")), file);
        assert_eq!(inodes.path(ROOT), Some(RepoPathBuf::root()));
    }
}
