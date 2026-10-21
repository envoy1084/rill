use std::time::Duration;

use rill_store::Stores;
use rill_types::{Principal, PrincipalKind, RepoPathBuf};

use super::*;

async fn authorizer() -> Authorizer {
    let stores = Stores::memory();
    let cfg = stores.create_repo("r", "").await.unwrap();
    Authorizer::with_ttl(stores.repo(cfg.id), Duration::from_secs(60))
}

fn p(s: &str) -> RepoPathBuf {
    RepoPathBuf::new(s).unwrap()
}

#[tokio::test]
async fn rewrites_owner_writer_reader() {
    let auth = authorizer().await;
    let alice = Principal::human("alice");
    let bob = Principal::human("bob");
    let carol = Principal::human("carol");
    auth.grant(
        "repo",
        Relation::Owner,
        &Subject::principal(&alice),
        &admin(),
    )
    .await
    .unwrap();
    auth.grant(
        "repo",
        Relation::Writer,
        &Subject::principal(&bob),
        &admin(),
    )
    .await
    .unwrap();
    auth.grant(
        "repo",
        Relation::Reader,
        &Subject::principal(&carol),
        &admin(),
    )
    .await
    .unwrap();

    for r in Relation::ALL {
        assert!(auth.check(&alice, &[], r, "repo").await, "owner has {r}");
    }
    assert!(auth.check(&bob, &[], Relation::Writer, "repo").await);
    assert!(auth.check(&bob, &[], Relation::Reader, "repo").await);
    assert!(auth.check(&bob, &[], Relation::Contributor, "repo").await);
    assert!(!auth.check(&bob, &[], Relation::Owner, "repo").await);
    assert!(!auth.check(&bob, &[], Relation::Reviewer, "repo").await);
    assert!(auth.check(&carol, &[], Relation::Reader, "repo").await);
    assert!(!auth.check(&carol, &[], Relation::Writer, "repo").await);
}

#[tokio::test]
async fn directories_inherit_from_ancestors() {
    let auth = authorizer().await;
    let alice = Principal::human("alice");
    let bob = Principal::human("bob");
    auth.grant(
        "dir:/src",
        Relation::Writer,
        &Subject::principal(&alice),
        &admin(),
    )
    .await
    .unwrap();
    auth.grant(
        "repo",
        Relation::Reader,
        &Subject::principal(&bob),
        &admin(),
    )
    .await
    .unwrap();

    assert!(
        auth.check(&alice, &[], Relation::Writer, "dir:/src/billing/deep")
            .await
    );
    assert!(
        auth.check_path(&alice, &[], Relation::Writer, &p("src/billing/x.rs"))
            .await
    );
    assert!(!auth.check(&alice, &[], Relation::Writer, "dir:/docs").await);
    assert!(!auth.check(&alice, &[], Relation::Reader, "repo").await);
    assert!(auth.check(&bob, &[], Relation::Reader, "dir:/src/x").await);
    assert!(!auth.check(&bob, &[], Relation::Writer, "dir:/src/x").await);
}

#[tokio::test]
async fn teams_kinds_and_wildcards() {
    let auth = authorizer().await;
    let alice = Principal::human("alice");
    let bot = Principal::agent("bot");
    auth.grant(
        "repo",
        Relation::Writer,
        &Subject::Team("core".into()),
        &admin(),
    )
    .await
    .unwrap();
    auth.grant(
        "repo",
        Relation::Reviewer,
        &Subject::Kind(PrincipalKind::Agent),
        &admin(),
    )
    .await
    .unwrap();
    auth.grant("dir:/docs", Relation::Reader, &Subject::Any, &admin())
        .await
        .unwrap();

    let core = vec!["core".to_string()];
    assert!(auth.check(&alice, &core, Relation::Writer, "repo").await);
    assert!(!auth.check(&alice, &[], Relation::Writer, "repo").await);
    assert!(auth.check(&bot, &[], Relation::Reviewer, "repo").await);
    assert!(!auth.check(&alice, &[], Relation::Reviewer, "repo").await);
    assert!(
        auth.check(&alice, &[], Relation::Reader, "dir:/docs/x")
            .await
    );
    assert!(auth.check(&bot, &[], Relation::Reader, "dir:/docs").await);
    assert!(!auth.check(&bot, &[], Relation::Reader, "repo").await);
}

#[tokio::test]
async fn revoke_and_cache_invalidation() {
    let auth = authorizer().await;
    let alice = Principal::human("alice");
    let subj = Subject::principal(&alice);
    assert!(!auth.check(&alice, &[], Relation::Reader, "repo").await);
    auth.grant("repo", Relation::Reader, &subj, &admin())
        .await
        .unwrap();
    assert!(auth.check(&alice, &[], Relation::Reader, "repo").await);
    auth.revoke("repo", Relation::Reader, &subj, &admin())
        .await
        .unwrap();
    assert!(!auth.check(&alice, &[], Relation::Reader, "repo").await);

    // A write that bypasses the authorizer is invisible until invalidated.
    auth.repo()
        .add_relation("repo", "reader", &subj.to_string(), &admin())
        .await
        .unwrap();
    assert!(!auth.check(&alice, &[], Relation::Reader, "repo").await);
    auth.invalidate();
    assert!(auth.check(&alice, &[], Relation::Reader, "repo").await);
}

#[tokio::test]
async fn allowed_paths_filter() {
    let auth = authorizer().await;
    let alice = Principal::human("alice");
    let bob = Principal::human("bob");
    let nobody = Principal::human("nobody");
    auth.grant(
        "dir:/src/billing",
        Relation::Reader,
        &Subject::principal(&alice),
        &admin(),
    )
    .await
    .unwrap();
    auth.grant(
        "dir:/docs",
        Relation::Writer,
        &Subject::principal(&alice),
        &admin(),
    )
    .await
    .unwrap();
    auth.grant("repo", Relation::Owner, &Subject::principal(&bob), &admin())
        .await
        .unwrap();

    let f = auth.allowed_paths(&alice, &[]).await;
    assert!(f.allows(&p("src/billing/invoice.rs")));
    assert!(f.allows(&p("src/billing")));
    assert!(f.allows(&p("docs/a/b.md")));
    assert!(!f.allows(&p("src/auth/x.rs")));
    assert!(!f.allows(&p("src")));
    assert!(!f.allows(&RepoPathBuf::root()));

    let mut paths = vec![p("src/billing/a"), p("src/auth/b"), p("docs/c")];
    f.retain(&mut paths);
    assert_eq!(paths, vec![p("src/billing/a"), p("docs/c")]);

    let f = auth.allowed_paths(&bob, &[]).await;
    assert!(f.allows(&p("anything/at/all")));
    assert!(f.allows(&RepoPathBuf::root()));

    let f = auth.allowed_paths(&nobody, &[]).await;
    assert!(!f.allows(&p("docs/c")));

    // Writer filter excludes the read-only grant.
    let f = auth.allowed_paths_for(&alice, &[], Relation::Writer).await;
    assert!(f.allows(&p("docs/c")));
    assert!(!f.allows(&p("src/billing/a")));
}

fn admin() -> Principal {
    Principal::human("admin")
}
