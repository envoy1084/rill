//! Directories large enough to be split into nodes.

use std::collections::BTreeMap;
use std::sync::Arc;

use rill_core::diff_trees;
use rill_core::tree::{Edit, TreeBuilder, TreeOps};
use rill_store::MemoryObjectStore;
use rill_types::{ContentRef, EntryKind, FileKind, RepoPathBuf, Tree, TreeHash, SHARD_ENTRIES};

fn ops() -> TreeOps {
    TreeOps::new(Arc::new(MemoryObjectStore::new()), 1 << 16)
}

fn p(s: &str) -> RepoPathBuf {
    RepoPathBuf::new(s).unwrap()
}

fn file(body: &str) -> Edit {
    Edit::Set(
        ContentRef::inline(body.as_bytes().to_vec()),
        FileKind::Regular,
    )
}

/// A small deterministic generator, so failures replay.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

fn name(i: u64) -> String {
    format!("item-{i:06}.rs")
}

async fn build(
    ops: &TreeOps,
    base: Option<TreeHash>,
    edits: &[(String, Option<String>)],
) -> TreeHash {
    let mut b = TreeBuilder::new();
    for (path, body) in edits {
        b.edit(
            &p(path),
            match body {
                Some(body) => file(body),
                None => Edit::Delete,
            },
        );
    }
    b.build(ops, base).await.unwrap()
}

async fn dir_of(ops: &TreeOps, root: TreeHash) -> TreeHash {
    ops.dir_hash(root, &p("big")).await.unwrap().unwrap()
}

#[tokio::test]
async fn equal_contents_give_equal_hashes_however_they_were_built() {
    let ops = ops();
    let mut rng = Rng(7);
    let mut model: BTreeMap<String, String> = BTreeMap::new();
    let mut root = None;
    // Grow past the split threshold, shrink below it and grow again, in
    // random batches of adds, rewrites and deletes.
    for round in 0..40u64 {
        let mut batch = Vec::new();
        let target = if (10..25).contains(&round) {
            2_000
        } else {
            9_000
        };
        for _ in 0..rng.below(1_500) + 1 {
            let key = name(rng.below(12_000));
            let delete = model.len() > target || rng.below(4) == 0;
            let body = (!delete).then(|| format!("{key}@{round}"));
            match &body {
                Some(b) => model.insert(key.clone(), b.clone()),
                None => model.remove(&key),
            };
            batch.push((format!("big/{key}"), body));
        }
        root = Some(build(&ops, root, &batch).await);

        let fresh = build(
            &self::ops(),
            None,
            &model
                .iter()
                .map(|(k, v)| (format!("big/{k}"), Some(v.clone())))
                .collect::<Vec<_>>(),
        )
        .await;
        assert_eq!(root, Some(fresh), "round {round}: {} entries", model.len());
    }
    let dir = dir_of(&ops, root.unwrap()).await;
    let listed = ops.get_tree(&dir).await.unwrap();
    assert_eq!(listed.len(), model.len());
    for (k, v) in &model {
        let Some(EntryKind::File { content, .. }) =
            ops.entry(&dir, k).await.unwrap().map(|e| e.kind)
        else {
            panic!("{k} missing");
        };
        assert_eq!(content, ContentRef::inline(v.as_bytes().to_vec()));
    }
    assert_eq!(ops.summary(&dir).await.unwrap(), listed.summary());
}

async fn big_dir(entries: u64) -> (TreeOps, TreeHash) {
    let ops = ops();
    let edits: Vec<_> = (0..entries)
        .map(|i| (format!("big/{}", name(i)), Some(format!("v{i}"))))
        .collect();
    let root = build(&ops, None, &edits).await;
    (ops, root)
}

#[tokio::test]
async fn an_edit_in_a_200k_entry_directory_writes_a_few_nodes() {
    let (ops, root) = big_dir(200_000).await;
    let before = ops.nodes_written();
    let root2 = build(
        &ops,
        Some(root),
        &[(format!("big/{}", name(123_456)), Some("new".into()))],
    )
    .await;
    let rewrite = ops.nodes_written() - before;

    let before = ops.nodes_written();
    build(
        &ops,
        Some(root2),
        &[("big/item-123456a.rs".into(), Some("added".into()))],
    )
    .await;
    let insert = ops.nodes_written() - before;

    // Leaf, inner levels and the repository root; an added name also
    // rewrites a path through the name index.
    assert!(rewrite <= 5, "a rewrite wrote {rewrite} nodes");
    assert!(insert <= 10, "an insert wrote {insert} nodes");
}

#[tokio::test]
async fn case_collisions_are_found_in_split_directories() {
    let (ops, root) = big_dir(SHARD_ENTRIES as u64 + 2_000).await;
    let err = TreeBuilder::new();
    let mut b = err;
    b.edit(&p(&format!("big/{}", name(10).to_uppercase())), file("x"));
    let err = b.build(&ops, Some(root)).await.unwrap_err();
    assert!(err.to_string().contains("differ only in case"), "{err}");

    // Renaming to a case variant is fine.
    let mut b = TreeBuilder::new();
    b.edit(&p(&format!("big/{}", name(10))), Edit::Delete);
    b.edit(&p(&format!("big/{}", name(10).to_uppercase())), file("x"));
    let renamed = b.build(&ops, Some(root)).await.unwrap();
    let dir = dir_of(&ops, renamed).await;
    assert!(ops
        .entry(&dir, &name(10).to_uppercase())
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn diffs_between_split_directories_report_exactly_the_changes() {
    let (ops, root) = big_dir(30_000).await;
    let edits = vec![
        (format!("big/{}", name(5)), Some("changed".to_string())),
        (format!("big/{}", name(29_999)), None),
        ("big/zzz-new.rs".to_string(), Some("new".to_string())),
    ];
    let root2 = build(&ops, Some(root), &edits).await;
    let diff = diff_trees(&ops, Some(root), Some(root2)).await.unwrap();
    let paths: Vec<_> = diff.iter().map(|d| d.path.as_str()).collect();
    assert_eq!(
        paths,
        vec!["big/item-000005.rs", "big/item-029999.rs", "big/zzz-new.rs"]
    );
    assert!(diff[1].is_deleted() && diff[2].is_added());
}

#[tokio::test]
async fn a_directory_shrinking_below_the_threshold_is_one_node_again() {
    let (ops, root) = big_dir(SHARD_ENTRIES as u64 + 10).await;
    let deletes: Vec<_> = (0..20)
        .map(|i| (format!("big/{}", name(i)), None))
        .collect();
    let root2 = build(&ops, Some(root), &deletes).await;
    let dir = dir_of(&ops, root2).await;
    let listing = ops.get_tree(&dir).await.unwrap();
    assert_eq!(listing.len(), SHARD_ENTRIES - 10);
    assert_eq!(dir, ops.put_tree(&Tree::clone(&listing)).await.unwrap());
    assert!(matches!(
        ops.get_node(&dir).await.unwrap(),
        rill_core::tree::NodeRef::Leaf(_)
    ));
}
