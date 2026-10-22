use std::sync::Arc;

use rill_core::{ChangeBuilder, DeriveManager, TreeOps};
use rill_search::{
    search, IndexConfig, SearchIndex, SearchQuery, SymbolIndex, SymbolKind, TrigramQuery,
};
use rill_store::{RepoStore, Stores};
use rill_types::{ChangeVersionHash, Principal, RepoPathBuf, TreeHash};

struct Fixture {
    repo: RepoStore,
    derive: Arc<DeriveManager>,
    index: Arc<SearchIndex>,
    version: ChangeVersionHash,
    root: TreeHash,
}

const MAIN_RS: &str = "use std::fmt;\n\n/// Entry point.\nfn main() {\n    println!(\"Hello, World\");\n    helper();\n}\n\nfn helper() {}\n";
const LIB_PY: &str = "class Greeter:\n    def hello(self):\n        return 'hello world'\n";
const NOTES: &str = "hello from the notes\nnothing else here\n";

async fn fixture() -> Fixture {
    let stores = Stores::memory();
    let config = stores.create_repo("r", "").await.unwrap();
    let repo = stores.repo(config.id);
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
    let index = SearchIndex::new(repo.clone(), derive.clone(), IndexConfig::default());
    let (root, stats) = index.index_version(&version, None).await.unwrap();
    assert_eq!(stats.files, 4);
    assert_eq!(stats.blobs_indexed, 3);
    assert_eq!(stats.blobs_skipped, 1);
    Fixture {
        repo,
        derive,
        index,
        version,
        root,
    }
}

fn paths(hits: &[rill_search::SearchHit]) -> Vec<(&str, u32)> {
    hits.iter().map(|h| (h.path.as_str(), h.line_no)).collect()
}

#[tokio::test]
async fn searches_with_case_and_glob() {
    let f = fixture().await;
    let hits = search(&f.index, f.root, &SearchQuery::new("hello"), |_| true)
        .await
        .unwrap();
    assert_eq!(
        paths(&hits),
        vec![("docs/notes.txt", 1), ("py/lib.py", 2), ("py/lib.py", 3)]
    );
    assert_eq!(hits[2].line, "        return 'hello world'");
    assert_eq!(&LIB_PY.as_bytes()[hits[2].byte_range.clone()], b"hello");

    let hits = search(
        &f.index,
        f.root,
        &SearchQuery::new("hello").case_insensitive(true),
        |_| true,
    )
    .await
    .unwrap();
    assert_eq!(
        paths(&hits),
        vec![
            ("docs/notes.txt", 1),
            ("py/lib.py", 2),
            ("py/lib.py", 3),
            ("src/main.rs", 5)
        ]
    );

    let hits = search(
        &f.index,
        f.root,
        &SearchQuery::new("hello")
            .case_insensitive(true)
            .path_glob("**/*.rs")
            .unwrap()
            .context_lines(1),
        |_| true,
    )
    .await
    .unwrap();
    assert_eq!(paths(&hits), vec![("src/main.rs", 5)]);
    assert_eq!(hits[0].before, vec!["fn main() {"]);
    assert_eq!(hits[0].after, vec!["    helper();"]);
}

#[tokio::test]
async fn regex_without_literals_and_path_filter() {
    let f = fixture().await;
    let q = SearchQuery::new(r"fn\s+main");
    assert_eq!(q.trigram_query().unwrap(), TrigramQuery::literal(b"main"));
    let hits = search(&f.index, f.root, &q, |_| true).await.unwrap();
    assert_eq!(paths(&hits), vec![("src/main.rs", 4)]);

    // No required literal (two-byte literal): full scan still finds
    // things, binaries excluded.
    let q = SearchQuery::new(r"\bhe\w+");
    assert!(q.trigram_query().unwrap().is_all());
    let hits = search(&f.index, f.root, &q, |_| true).await.unwrap();
    assert!(hits.iter().all(|h| h.path.as_str() != "assets/blob.bin"));
    assert_eq!(
        paths(&hits),
        vec![
            ("docs/notes.txt", 1),
            ("docs/notes.txt", 2),
            ("py/lib.py", 2),
            ("py/lib.py", 3),
            ("src/main.rs", 6),
            ("src/main.rs", 9)
        ]
    );

    // Permission filter hides a path entirely.
    let hits = search(
        &f.index,
        f.root,
        &SearchQuery::new("hello").case_insensitive(true),
        |p| !p.starts_with(&RepoPathBuf::new("docs").unwrap()),
    )
    .await
    .unwrap();
    assert_eq!(
        paths(&hits),
        vec![("py/lib.py", 2), ("py/lib.py", 3), ("src/main.rs", 5)]
    );

    let hits = search(
        &f.index,
        f.root,
        &SearchQuery::new("hello")
            .case_insensitive(true)
            .max_results(1),
        |_| true,
    )
    .await
    .unwrap();
    assert_eq!(hits.len(), 1);

    assert!(search(&f.index, f.root, &SearchQuery::new("("), |_| true)
        .await
        .is_err());
    assert!(matches!(
        search(&f.index, TreeHash::ZERO, &SearchQuery::new("x"), |_| true).await,
        Err(rill_search::Error::RootNotIndexed(_))
    ));
}

#[tokio::test]
async fn reindexing_is_a_noop_and_shares_blobs() {
    let f = fixture().await;
    assert_eq!(f.index.blob_index_calls(), 4);
    let stats = f.index.index_root(f.root, None).await.unwrap();
    assert!(stats.already_indexed);
    assert_eq!(f.index.blob_index_calls(), 4);

    // A second version that changes one file tokenizes only that blob.
    let (v2, _) = ChangeBuilder::new(Principal::human("alice"))
        .parent(f.version)
        .message("edit")
        .file("src/main.rs", "fn main() { goodbye(); }\n")
        .unwrap()
        .commit(&f.repo)
        .await
        .unwrap();
    let (root2, stats) = f.index.index_version(&v2, None).await.unwrap();
    assert_ne!(root2, f.root);
    assert_eq!(stats.blobs_indexed, 1);
    assert_eq!(f.index.blob_index_calls(), 5);
    let hits = search(&f.index, root2, &SearchQuery::new("goodbye"), |_| true)
        .await
        .unwrap();
    assert_eq!(paths(&hits), vec![("src/main.rs", 1)]);
    let hits = search(&f.index, f.root, &SearchQuery::new("goodbye"), |_| true)
        .await
        .unwrap();
    assert!(hits.is_empty());
}

#[tokio::test]
async fn indexing_persists_postings_before_the_manifest() {
    let f = fixture().await;
    // Indexing flushed already: nothing is left for an explicit flush.
    assert_eq!(f.index.flush(None).await.unwrap(), Default::default());

    let reopened = SearchIndex::open(f.repo.clone(), f.derive.clone(), IndexConfig::default())
        .await
        .unwrap();
    assert_eq!(reopened.doc_count(), 3);
    let stats = reopened.index_root(f.root, None).await.unwrap();
    assert!(stats.already_indexed);
    assert_eq!(reopened.blob_index_calls(), 0);
    let hits = search(
        &reopened,
        f.root,
        &SearchQuery::new("World").case_insensitive(true),
        |_| true,
    )
    .await
    .unwrap();
    assert_eq!(paths(&hits), vec![("py/lib.py", 3), ("src/main.rs", 5)]);

    // A fresh index over the same store re-walks a new root but skips
    // every blob it already has markers for.
    let (v2, _) = ChangeBuilder::new(Principal::human("alice"))
        .parent(f.version)
        .message("add")
        .file("extra.txt", "brand new content\n")
        .unwrap()
        .commit(&f.repo)
        .await
        .unwrap();
    let (_, stats) = reopened.index_version(&v2, None).await.unwrap();
    assert_eq!(stats.blobs_indexed, 1);
    assert_eq!(reopened.blob_index_calls(), 1);
}

#[tokio::test]
async fn symbol_lookup() {
    let f = fixture().await;
    let symbols = SymbolIndex::new(f.repo.clone(), f.derive.trees().clone());
    let defs = symbols.find_definition(f.root, "main").await.unwrap();
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].path.as_str(), "src/main.rs");
    assert_eq!(defs[0].symbol.kind, SymbolKind::Function);
    assert_eq!(defs[0].symbol.line, 4);

    let listed = symbols
        .list_symbols(f.root, &RepoPathBuf::new("py/lib.py").unwrap())
        .await
        .unwrap();
    let names: Vec<_> = listed.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["Greeter", "hello"]);
    assert!(symbols
        .find_definition(f.root, "nothing")
        .await
        .unwrap()
        .is_empty());

    // Persisted table is reused by a fresh instance.
    let again = SymbolIndex::new(f.repo.clone(), f.derive.trees().clone());
    assert_eq!(again.for_root(f.root).await.unwrap().file_count(), 2);
}

#[tokio::test]
async fn a_reader_finds_what_another_process_indexed() {
    let f = fixture().await;
    let reader = SearchIndex::open(f.repo.clone(), f.derive.clone(), IndexConfig::default())
        .await
        .unwrap();
    let (v2, _) = ChangeBuilder::new(Principal::human("alice"))
        .parent(f.version)
        .message("add")
        .file("src/new.rs", "fn freshly_indexed() {}\n")
        .unwrap()
        .commit(&f.repo)
        .await
        .unwrap();
    let (root2, _) = f.index.index_version(&v2, None).await.unwrap();

    let hits = search(&reader, root2, &SearchQuery::new("freshly_indexed"), |_| {
        true
    })
    .await
    .unwrap();
    assert_eq!(paths(&hits), vec![("src/new.rs", 1)]);
    assert!(!reader.refresh().await.unwrap(), "already current");
}

#[tokio::test]
async fn a_replaced_writer_cannot_change_the_index() {
    let f = fixture().await;
    let stale = f.repo.try_acquire("search", "a").await.unwrap().unwrap();
    f.repo.release(&stale).await.unwrap();
    f.repo.try_acquire("search", "b").await.unwrap().unwrap();
    let (v2, _) = ChangeBuilder::new(Principal::human("alice"))
        .parent(f.version)
        .message("add")
        .file("src/late.rs", "fn too_late() {}\n")
        .unwrap()
        .commit(&f.repo)
        .await
        .unwrap();
    assert!(f.index.index_version(&v2, Some(&stale)).await.is_err());
    let reader = SearchIndex::open(f.repo.clone(), f.derive.clone(), IndexConfig::default())
        .await
        .unwrap();
    assert!(reader
        .manifest(f.derive.get::<rill_core::RootTree>(&v2).await.unwrap())
        .await
        .is_err());
}
