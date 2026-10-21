//! A one-file commit writes one tree node per directory on its path.

use rill_bench::{Corpus, Shape};
use rill_core::derive::RootTree;

#[tokio::test]
async fn a_one_file_commit_writes_one_node_per_directory_level() {
    let corpus = Corpus::build(Shape::with_files(40_000)).await;
    let trees = corpus.derive.trees();
    corpus.derive.get::<RootTree>(&corpus.root).await.unwrap();
    // Revisions pick files by `7 * revision`, so these reach the deepest
    // directories.
    for revision in 5_000..5_005 {
        let before = trees.nodes_written();
        let version = corpus.change(corpus.root, 1, revision).await;
        corpus.derive.get::<RootTree>(&version).await.unwrap();
        let path = corpus.shape.path(corpus.spread(0, 1, revision));
        let depth = path.split('/').count() as u64;
        assert!(depth >= 3, "{path}");
        assert_eq!(trees.nodes_written() - before, depth, "{path}");
    }
}
