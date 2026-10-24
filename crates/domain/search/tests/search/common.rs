//! A repository with a few files and an index over it.

use std::sync::Arc;

use rill_core::{ChangeBuilder, DeriveManager, TreeOps};
use rill_search::{search, IndexConfig, Scope, SearchHit, SearchIndex, SearchQuery, SearchResult};
use rill_store::{RepoStore, Stores};
use rill_types::{ChangeVersionHash, Principal, RepoPath, TreeHash};

pub struct Fixture {
    pub repo: RepoStore,
    pub derive: Arc<DeriveManager>,
    pub index: Arc<SearchIndex>,
    pub version: ChangeVersionHash,
    pub root: TreeHash,
}

pub const MAIN_RS: &str = "use std::fmt;\n\n/// Entry point.\nfn main() {\n    println!(\"Hello, World\");\n    helper();\n}\n\nfn helper() {}\n";
pub const LIB_PY: &str = "class Greeter:\n    def hello(self):\n        return 'hello world'\n";
pub const NOTES: &str = "hello from the notes\nnothing else here\n";

pub fn config() -> IndexConfig {
    IndexConfig {
        shards: 4,
        merge_fanout: 3,
        ..IndexConfig::default()
    }
}

pub async fn fixture() -> Fixture {
    let stores = Stores::memory();
    let repo_config = stores.create_repo("r", "").await.unwrap();
    let repo = stores.repo(repo_config.id);
    let trees = TreeOps::new(repo.objects().clone(), 64);
    let derive = DeriveManager::new(repo.clone(), trees, 64);
    let (version, _) = ChangeBuilder::new(Principal::human("alice"))
        .message("init")
        .file("src/main.rs", MAIN_RS)
        .unwrap()
        .file("py/lib.py", LIB_PY)
        .unwrap()
        .file("docs/notes.txt", NOTES)
        .unwrap()
        .file("assets/blob.bin", b"\0\x01\x02hello".to_vec())
        .unwrap()
        .commit(&repo)
        .await
        .unwrap();
    let index = SearchIndex::new(repo.clone(), derive.clone(), config());
    let (root, stats) = index.index_version(&version, None).await.unwrap();
    assert_eq!(stats.files, 4);
    assert_eq!((stats.blobs_indexed, stats.blobs_skipped), (3, 1));
    Fixture {
        repo,
        derive,
        index,
        version,
        root,
    }
}

impl Fixture {
    /// A child version of `parent` setting `files`.
    pub async fn child(
        &self,
        parent: ChangeVersionHash,
        files: &[(&str, &str)],
    ) -> ChangeVersionHash {
        let mut b = ChangeBuilder::new(Principal::human("alice"))
            .parent(parent)
            .message("edit");
        for (p, c) in files {
            b = b.file(p, c.to_string()).unwrap();
        }
        b.commit(&self.repo).await.unwrap().0
    }
}

/// Search `root` of `index` with every path allowed, unless `allow` says.
pub async fn run(
    index: &SearchIndex,
    root: TreeHash,
    query: SearchQuery,
    allow: impl Fn(&RepoPath) -> bool + Send + Sync + Clone + 'static,
) -> SearchResult {
    let view = index.view(root).await.unwrap().expect("indexed root");
    search(index, &Scope::indexed(view), &query, allow)
        .await
        .unwrap()
}

/// `(path, line)` of each hit, sorted.
pub fn lines(hits: &[SearchHit]) -> Vec<(String, u32)> {
    let mut out: Vec<_> = hits
        .iter()
        .map(|h| (h.path.to_string(), h.line_no))
        .collect();
    out.sort();
    out
}

pub fn all(_: &RepoPath) -> bool {
    true
}
