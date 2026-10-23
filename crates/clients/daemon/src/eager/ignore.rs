//! `.rillignore` rules (gitignore syntax, at any directory level) and the
//! daemon's own files, which are never recorded. Ignore rules only hide
//! untracked files: a tracked path is always reconciled.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::Match;
use parking_lot::Mutex;
use rill_types::RepoPath;

use super::disk::to_fs_path;

/// Name of the ignore file.
pub const IGNORE_FILE: &str = ".rillignore";
/// Prefix of the daemon's temporary files (cookies, atomic writes).
pub const TEMP_PREFIX: &str = ".rill-tmp-";
/// The marker naming the mount, written at its root.
pub const MARKER: &str = ".rill/mount.json";

/// Ignore rules of one mount, loaded per directory on demand.
#[derive(Debug)]
pub struct IgnoreRules {
    root: PathBuf,
    dirs: Mutex<HashMap<PathBuf, Option<Arc<Gitignore>>>>,
}

impl IgnoreRules {
    /// Rules of the mount at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        IgnoreRules {
            root: root.into(),
            dirs: Mutex::new(HashMap::new()),
        }
    }

    /// True for the daemon's own files, whatever the rules say.
    pub fn is_internal(path: &RepoPath) -> bool {
        path.as_str() == MARKER
            || rill_types::limits::is_reserved(path)
            || path.file_name().is_some_and(|n| n.starts_with(TEMP_PREFIX))
    }

    /// Whether an untracked `path` is ignored.
    pub fn ignored(&self, path: &RepoPath, is_dir: bool) -> bool {
        if Self::is_internal(path) || path.file_name() == Some(IGNORE_FILE) {
            return true;
        }
        let fs_path = to_fs_path(&self.root, path);
        let mut ignored = false;
        let mut dir = self.root.clone();
        let mut components: Vec<&str> = path.components().collect();
        components.pop();
        for i in 0..=components.len() {
            if let Some(rules) = self.rules(&dir) {
                match rules.matched_path_or_any_parents(&fs_path, is_dir) {
                    Match::Ignore(_) => ignored = true,
                    Match::Whitelist(_) => ignored = false,
                    Match::None => {}
                }
            }
            if let Some(c) = components.get(i) {
                dir.push(c);
            }
        }
        ignored
    }

    /// Forget the rules of `dir` (its ignore file changed).
    pub fn invalidate(&self, dir: &Path) {
        self.dirs.lock().remove(dir);
    }

    fn rules(&self, dir: &Path) -> Option<Arc<Gitignore>> {
        if let Some(found) = self.dirs.lock().get(dir) {
            return found.clone();
        }
        let file = dir.join(IGNORE_FILE);
        let rules = file.is_file().then(|| {
            let mut builder = GitignoreBuilder::new(dir);
            if let Some(e) = builder.add(&file) {
                tracing::warn!(file = %file.display(), error = %e, "bad ignore rules");
            }
            builder.build().map(Arc::new).ok()
        });
        let rules = rules.flatten();
        self.dirs.lock().insert(dir.to_path_buf(), rules.clone());
        rules
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_types::RepoPathBuf;

    #[test]
    fn nested_rules_apply_below_their_directory_and_can_re_include() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(IGNORE_FILE), "*.log\nbuild/\n").unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub").join(IGNORE_FILE), "!keep.log\n").unwrap();
        let rules = IgnoreRules::new(dir.path());
        let p = |s: &str| RepoPathBuf::new(s).unwrap();
        assert!(rules.ignored(&p("a.log"), false));
        assert!(rules.ignored(&p("build/out.o"), false));
        assert!(!rules.ignored(&p("src/a.rs"), false));
        assert!(rules.ignored(&p("sub/x.log"), false));
        assert!(!rules.ignored(&p("sub/keep.log"), false));
        assert!(rules.ignored(&p(".rill/mount.json"), false));
    }
}
