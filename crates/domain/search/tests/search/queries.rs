//! Queries: case, globs, languages, regexes without literals, permission
//! filtering, ranking and facets, live files.

use rill_search::{search, LiveFile, Scope, SearchQuery};
use rill_types::{ContentRef, FileKind, ObjectHash, ObjectKind, RepoPathBuf};

use crate::common::*;

#[tokio::test]
async fn case_globs_and_languages() {
    let f = fixture().await;
    let r = run(&f.index, f.root, SearchQuery::new("hello"), all).await;
    assert_eq!(
        lines(&r.hits),
        [
            ("docs/notes.txt".into(), 1),
            ("py/lib.py".into(), 2),
            ("py/lib.py".into(), 3)
        ]
    );
    let h = r.hits.iter().find(|h| h.line_no == 3).unwrap();
    assert_eq!(&LIB_PY.as_bytes()[h.byte_range.clone()], b"hello");

    let r = run(
        &f.index,
        f.root,
        SearchQuery::new("hello").case_insensitive(true),
        all,
    )
    .await;
    assert_eq!(r.files, 3);
    assert_eq!(r.facets.languages.get("Rust"), Some(&1));
    assert_eq!(r.facets.directories.get("py"), Some(&1));

    let q = SearchQuery::new("hello")
        .case_insensitive(true)
        .path_glob("**/*.rs")
        .unwrap()
        .context_lines(1);
    let r = run(&f.index, f.root, q, all).await;
    assert_eq!(lines(&r.hits), [("src/main.rs".into(), 5)]);
    assert_eq!(r.hits[0].before, ["fn main() {"]);
    let q = SearchQuery::new("hello").languages(vec!["python".into()]);
    assert_eq!(lines(&run(&f.index, f.root, q, all).await.hits).len(), 2);
    let q = SearchQuery::new("println!(").literal(true);
    assert_eq!(
        lines(&run(&f.index, f.root, q, all).await.hits),
        [("src/main.rs".into(), 5)]
    );
}

#[tokio::test]
async fn unnarrowable_patterns_permissions_and_limits() {
    let f = fixture().await;
    let q = SearchQuery::new(r"\bhe\w+");
    assert!(q.trigram_query().unwrap().is_all());
    let r = run(&f.index, f.root, q, all).await;
    assert!(r.hits.iter().all(|h| h.path.as_str() != "assets/blob.bin"));
    assert_eq!(r.hits.len(), 6);

    let docs = RepoPathBuf::new("docs").unwrap();
    let r = run(
        &f.index,
        f.root,
        SearchQuery::new("hello").case_insensitive(true),
        move |p| !p.starts_with(&docs),
    )
    .await;
    assert!(r.hits.iter().all(|h| !h.path.as_str().starts_with("docs")));

    let r = run(
        &f.index,
        f.root,
        SearchQuery::new("hello")
            .case_insensitive(true)
            .max_results(1),
        all,
    )
    .await;
    assert_eq!(r.hits.len(), 1);
    assert!(r.truncated);
    let view = f.index.view(f.root).await.unwrap().unwrap();
    assert!(
        search(&f.index, &Scope::indexed(view), &SearchQuery::new("("), all)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn definitions_rank_first() {
    let f = fixture().await;
    let v = f
        .child(
            f.version,
            &[("tests/uses.rs", "fn t() { helper(); helper(); }\n")],
        )
        .await;
    let (root, _) = f.index.index_version(&v, None).await.unwrap();
    let r = run(&f.index, root, SearchQuery::new("helper"), all).await;
    assert_eq!(r.hits[0].path.as_str(), "src/main.rs", "{:?}", r.hits);
}

#[tokio::test]
async fn live_files_replace_their_indexed_paths() {
    let f = fixture().await;
    let view = f.index.view(f.root).await.unwrap().unwrap();
    let edited = b"fn main() { only_in_the_workspace(); }\n".to_vec();
    let hash = ObjectHash::digest(ObjectKind::Blob, &edited);
    let content = ContentRef::Inline {
        bytes: edited.into(),
    };
    let path = RepoPathBuf::new("src/main.rs").unwrap();
    let scope = Scope {
        view: std::sync::Arc::new(view.apply(&[(path.clone(), Some((hash, FileKind::Regular)))])),
        live: vec![LiveFile { path, content }],
    };
    let r = search(
        &f.index,
        &scope,
        &SearchQuery::new("only_in_the_workspace"),
        all,
    )
    .await
    .unwrap();
    assert_eq!(lines(&r.hits), [("src/main.rs".into(), 1)]);
    let r = search(&f.index, &scope, &SearchQuery::new("Hello, World"), all)
        .await
        .unwrap();
    assert!(r.hits.is_empty(), "the indexed copy is shadowed");
}

#[tokio::test]
async fn structural_patterns_match_syntax_not_text() {
    let f = fixture().await;
    let text = SearchQuery::new("helper()").literal(true);
    assert_eq!(
        lines(&run(&f.index, f.root, text, all).await.hits),
        [("src/main.rs".into(), 6), ("src/main.rs".into(), 9)]
    );
    let call = SearchQuery::new("helper($$$ARGS)")
        .structural(true)
        .languages(vec!["rust".into()]);
    assert!(!call.matcher().unwrap().trigram_query().unwrap().is_all());
    assert_eq!(
        lines(&run(&f.index, f.root, call, all).await.hits),
        [("src/main.rs".into(), 6)],
        "the definition is not a call"
    );
    let unscoped = SearchQuery::new("helper()").structural(true);
    assert!(unscoped.matcher().is_err());
}

#[tokio::test]
async fn symbols_by_name_substring_and_file() {
    use rill_search::{file_symbols, find_symbols, SymbolKind, SymbolName, SymbolQuery};
    let f = fixture().await;
    let view = f.index.view(f.root).await.unwrap().unwrap();
    let scope = Scope::indexed(view);
    let query = |name, kinds| SymbolQuery {
        name,
        kinds,
        limit: 10,
    };
    let defs = find_symbols(
        &f.index,
        &scope,
        &query(SymbolName::Exact("helper".into()), vec![]),
        all,
    )
    .await
    .unwrap();
    assert_eq!(defs.len(), 1);
    assert_eq!(
        (defs[0].path.as_str(), defs[0].symbol.line),
        ("src/main.rs", 9)
    );
    let classes = find_symbols(
        &f.index,
        &scope,
        &query(
            SymbolName::Contains("GREET".into()),
            vec![SymbolKind::Class],
        ),
        all,
    )
    .await
    .unwrap();
    assert_eq!(classes.len(), 1);
    assert_eq!(classes[0].symbol.name, "Greeter");
    let hidden = find_symbols(
        &f.index,
        &scope,
        &query(SymbolName::Exact("helper".into()), vec![]),
        |p: &rill_types::RepoPath| !p.as_str().starts_with("src/"),
    )
    .await
    .unwrap();
    assert!(hidden.is_empty());

    let main = RepoPathBuf::new("src/main.rs").unwrap();
    let names: Vec<_> = file_symbols(&f.index, &scope, &main)
        .await
        .unwrap()
        .unwrap()
        .into_iter()
        .map(|s| s.name)
        .collect();
    assert_eq!(names, ["main", "helper"]);
    let missing = RepoPathBuf::new("nope.rs").unwrap();
    assert!(file_symbols(&f.index, &scope, &missing)
        .await
        .unwrap()
        .is_none());
}
