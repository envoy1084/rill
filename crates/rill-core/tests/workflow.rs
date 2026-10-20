//! End-to-end: build history, derive trees, merge, rebase, resolve, blame.

use rill_core::derive::{ChangeSummary, Generation, PathHistory, RootTree};
use rill_core::merge::state::to_change;
use rill_core::{
    materialize, merge_changes, parse_materialized, read_content, rebase, ChangeBuilder,
    MergeChangeOptions,
};
use rill_types::{FileChange, Merge};

const BASE: &str = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\n";

mod common;

use common::{content_at, fixture, p, t};

#[tokio::test]
async fn full_workflow() {
    let f = fixture().await;

    // Root change with a few files.
    let (root, _) = ChangeBuilder::new(f.alice.clone())
        .message("init")
        .created_at(t(1))
        .file("src/main.rs", BASE)
        .unwrap()
        .file("src/lib.rs", "pub fn lib() {}\n")
        .unwrap()
        .file("README.md", "# demo\n")
        .unwrap()
        .commit(&f.repo)
        .await
        .unwrap();
    let root_tree = f.derive.get::<RootTree>(&root).await.unwrap();
    let listing = f
        .derive
        .trees()
        .list_dir(root_tree, &p("src"))
        .await
        .unwrap();
    assert_eq!(listing.len(), 2);
    assert_eq!(f.derive.get::<Generation>(&root).await.unwrap(), 1);
    let summary = f.derive.get::<ChangeSummary>(&root).await.unwrap();
    assert_eq!(summary.files_added, 3);
    assert_eq!(summary.languages.get("rs"), Some(&2));
    assert!(summary.top_level_dirs.contains("src"));

    // Two divergent children editing different regions of main.rs: a
    // clean merge.
    let (left, _) = ChangeBuilder::new(f.alice.clone())
        .parent(root)
        .message("left edits line 1")
        .created_at(t(2))
        .file("src/main.rs", BASE.replace("line1", "LEFT1"))
        .unwrap()
        .file("left.txt", "l\n")
        .unwrap()
        .commit(&f.repo)
        .await
        .unwrap();
    let (right, _) = ChangeBuilder::new(f.bob.clone())
        .parent(root)
        .message("right edits line 8")
        .created_at(t(3))
        .file("src/main.rs", BASE.replace("line8", "RIGHT8"))
        .unwrap()
        .delete("README.md")
        .unwrap()
        .commit(&f.repo)
        .await
        .unwrap();

    assert_eq!(f.graph.merge_base(&left, &right).await.unwrap(), Some(root));
    assert!(f.graph.is_ancestor(&root, &left).await.unwrap());
    assert!(!f.graph.is_ancestor(&left, &right).await.unwrap());

    let merged = merge_changes(
        &f.derive,
        &f.graph,
        &left,
        &right,
        MergeChangeOptions {
            created_at: t(4),
            ..MergeChangeOptions::new(f.alice.clone(), "merge right")
        },
    )
    .await
    .unwrap();
    assert!(!merged.has_conflicts());
    assert_eq!(merged.parents, vec![left, right]);
    assert_eq!(
        merged.file_changes.get(&p("README.md")),
        Some(&FileChange::Delete)
    );
    let merged_hash = f.repo.put_change_version(&merged).await.unwrap();
    let text = content_at(&f, &merged_hash, "src/main.rs").await.unwrap();
    assert_eq!(
        text,
        BASE.replace("line1", "LEFT1").replace("line8", "RIGHT8")
    );
    assert!(content_at(&f, &merged_hash, "left.txt").await.is_some());
    assert_eq!(f.derive.get::<Generation>(&merged_hash).await.unwrap(), 3);

    // A third child edits the same line as `left`: a conflict.
    let (clash, _) = ChangeBuilder::new(f.bob.clone())
        .parent(root)
        .message("clash on line 1")
        .created_at(t(5))
        .file("src/main.rs", BASE.replace("line1", "CLASH1"))
        .unwrap()
        .commit(&f.repo)
        .await
        .unwrap();
    let conflicted = merge_changes(
        &f.derive,
        &f.graph,
        &left,
        &clash,
        MergeChangeOptions {
            created_at: t(6),
            ..MergeChangeOptions::new(f.alice.clone(), "merge clash")
        },
    )
    .await
    .unwrap();
    assert!(conflicted.has_conflicts());
    let FileChange::Conflict(terms) = &conflicted.file_changes[&p("src/main.rs")] else {
        panic!("expected a conflict");
    };
    assert_eq!(terms.num_sides(), 2);
    assert!(terms.is_present_everywhere());
    let base_text = read_content(
        f.repo.objects().as_ref(),
        &terms.removes().next().unwrap().as_ref().unwrap().0,
    )
    .await
    .unwrap();
    assert_eq!(base_text, BASE);
    let conflicted_hash = f.repo.put_change_version(&conflicted).await.unwrap();
    // The tree carries the first add (left's content).
    assert_eq!(
        content_at(&f, &conflicted_hash, "src/main.rs")
            .await
            .unwrap(),
        BASE.replace("line1", "LEFT1")
    );

    // Materialize the conflict with markers and parse it back.
    let mut texts = Vec::new();
    for term in terms.iter() {
        let (content, _) = term.as_ref().unwrap();
        texts.push(Some(
            read_content(f.repo.objects().as_ref(), content)
                .await
                .unwrap(),
        ));
    }
    let text_terms = Merge::from_vec(texts).unwrap();
    let rendered = materialize(&text_terms);
    let rendered_str = String::from_utf8(rendered.clone()).unwrap();
    assert!(rendered_str.contains("<<<<<<< Conflict 1 of 1 (2 sides)"));
    assert!(rendered_str.contains("-line1\n+LEFT1\n"));
    assert!(rendered_str.contains("CLASH1\n"));
    assert_eq!(parse_materialized(&rendered).unwrap(), text_terms);

    // Rebase `clash` onto `right`: no overlap, so it applies cleanly and
    // keeps right's line 8.
    let rebased = rebase(&f.derive, &clash, &right, t(7)).await.unwrap();
    assert_eq!(rebased.parents, vec![right]);
    assert_eq!(rebased.predecessor, Some(clash));
    assert!(!rebased.has_conflicts());
    let rebased_hash = f.repo.put_change_version(&rebased).await.unwrap();
    assert_eq!(
        content_at(&f, &rebased_hash, "src/main.rs").await.unwrap(),
        BASE.replace("line1", "CLASH1").replace("line8", "RIGHT8")
    );
    assert!(content_at(&f, &rebased_hash, "README.md").await.is_none());

    // Rebase `clash` onto `left`: the same line differs, so the rebased
    // version carries a conflict whose terms are (left, base, clash).
    let rebased_conflict = rebase(&f.derive, &clash, &left, t(8)).await.unwrap();
    let FileChange::Conflict(rc) = &rebased_conflict.file_changes[&p("src/main.rs")] else {
        panic!("expected conflict after rebase");
    };
    assert_eq!(rc.num_sides(), 2);
    let rebased_conflict_hash = f.repo.put_change_version(&rebased_conflict).await.unwrap();

    // Resolve by producing a new version of the same change with a Set.
    let resolved_text = BASE.replace("line1", "LEFT1+CLASH1");
    let (resolved, resolved_cv) = ChangeBuilder::new(f.bob.clone())
        .change_id(rebased_conflict.change_id)
        .parent(left)
        .predecessor(rebased_conflict_hash)
        .message("clash on line 1 (resolved)")
        .created_at(t(9))
        .file("src/main.rs", resolved_text.clone())
        .unwrap()
        .commit(&f.repo)
        .await
        .unwrap();
    assert!(!resolved_cv.has_conflicts());
    assert_eq!(
        content_at(&f, &resolved, "src/main.rs").await.unwrap(),
        resolved_text
    );
    let versions = f
        .repo
        .change_versions(&rebased_conflict.change_id)
        .await
        .unwrap();
    assert_eq!(
        versions,
        vec![clash, rebased_hash, rebased_conflict_hash, resolved]
    );

    // Merging a tip that still records a conflict folds its terms in.
    let remerge = merge_changes(
        &f.derive,
        &f.graph,
        &rebased_conflict_hash,
        &right,
        MergeChangeOptions {
            created_at: t(10),
            ..MergeChangeOptions::new(f.alice.clone(), "merge right into conflict")
        },
    )
    .await
    .unwrap();
    let FileChange::Conflict(folded) = &remerge.file_changes[&p("src/main.rs")] else {
        panic!("expected conflict to survive merge");
    };
    // left + (clash - base) + (right - base): three sides, two bases.
    assert_eq!(folded.num_sides(), 3);
    let mut texts = Vec::new();
    for term in folded.iter() {
        let (content, _) = term.as_ref().unwrap();
        texts.push(Some(
            read_content(f.repo.objects().as_ref(), content)
                .await
                .unwrap(),
        ));
    }
    let rendered = String::from_utf8(materialize(&Merge::from_vec(texts).unwrap())).unwrap();
    // Line 8 merges cleanly outside the markers; line 1 stays in conflict.
    assert!(rendered.ends_with("line7\nRIGHT8\n"));
    assert!(rendered.contains("Conflict 1 of 1 (3 sides)"));
    assert_eq!(rendered.matches("<<<<<<<").count(), 1);

    // Blame: line 1 belongs to the resolution, the rest to the root.
    let blame = f.derive.blame(&resolved, &p("src/main.rs")).await.unwrap();
    assert_eq!(blame.len(), 8);
    assert_eq!(blame[0].version, resolved);
    assert_eq!(blame[0].author, f.bob);
    assert_eq!(blame[0].line_no, 1);
    assert_eq!(blame[1].version, root);
    assert_eq!(blame[7].line_no, 8);
    // Blame on an unchanged file follows the chain back to the root.
    let lib_blame = f.derive.blame(&resolved, &p("src/lib.rs")).await.unwrap();
    assert_eq!(lib_blame.len(), 1);
    assert_eq!(lib_blame[0].version, root);

    // Path history along the first-parent line.
    let history = f
        .derive
        .history(&p("src/main.rs"), &resolved, 10)
        .await
        .unwrap();
    assert_eq!(history, vec![resolved, left, root]);
    let dir_history = f.derive.history(&p("src"), &resolved, 10).await.unwrap();
    assert_eq!(dir_history, vec![resolved, left, root]);
    let lib_history = f
        .derive
        .history(&p("src/lib.rs"), &resolved, 10)
        .await
        .unwrap();
    assert_eq!(lib_history, vec![root]);
    assert!(f
        .derive
        .history(&p("nope"), &resolved, 10)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        f.derive.get::<PathHistory>(&root).await.unwrap(),
        f.derive.get::<PathHistory>(&root).await.unwrap()
    );

    // Log order: newest first, each version before its parents.
    let log = f.graph.ancestors(&merged_hash, 10).await.unwrap();
    let hashes: Vec<_> = log.iter().map(|e| e.info.hash).collect();
    assert_eq!(hashes, vec![merged_hash, right, left, root]);
    let only_new = f
        .graph
        .ancestors_excluding(&merged_hash, &left, 10)
        .await
        .unwrap();
    let hashes: Vec<_> = only_new.iter().map(|e| e.info.hash).collect();
    assert_eq!(hashes, vec![merged_hash, right]);

    // Trivial algebra: an explicit conflict whose sides agree resolves
    // without touching content.
    let state = Some((
        rill_types::ContentRef::inline(b"x".to_vec()),
        rill_types::FileKind::Regular,
    ));
    assert_eq!(
        to_change(
            Merge::three_way(None, state.clone(), state.clone())
                .into_trivially_resolved()
                .unwrap()
        ),
        FileChange::set(
            rill_types::ContentRef::inline(b"x".to_vec()),
            rill_types::FileKind::Regular
        )
    );
}

#[tokio::test]
async fn delete_versus_modify_conflicts_and_is_data() {
    let f = fixture().await;
    let (root, _) = ChangeBuilder::new(f.alice.clone())
        .message("init")
        .created_at(t(1))
        .file("a.txt", "one\ntwo\n")
        .unwrap()
        .commit(&f.repo)
        .await
        .unwrap();
    let (del, _) = ChangeBuilder::new(f.alice.clone())
        .parent(root)
        .created_at(t(2))
        .delete("a.txt")
        .unwrap()
        .commit(&f.repo)
        .await
        .unwrap();
    let (edit, _) = ChangeBuilder::new(f.bob.clone())
        .parent(root)
        .created_at(t(3))
        .file("a.txt", "one\nTWO\n")
        .unwrap()
        .commit(&f.repo)
        .await
        .unwrap();
    let merged = merge_changes(
        &f.derive,
        &f.graph,
        &del,
        &edit,
        MergeChangeOptions {
            created_at: t(4),
            ..MergeChangeOptions::new(f.alice.clone(), "m")
        },
    )
    .await
    .unwrap();
    let FileChange::Conflict(terms) = &merged.file_changes[&p("a.txt")] else {
        panic!("expected conflict");
    };
    assert!(terms.first().is_none());
    let hash = f.repo.put_change_version(&merged).await.unwrap();
    // First add is absent, so the tree has no file.
    assert!(content_at(&f, &hash, "a.txt").await.is_none());
    let summary = f.derive.get::<ChangeSummary>(&hash).await.unwrap();
    assert_eq!(summary.files_conflicted, 1);
}
