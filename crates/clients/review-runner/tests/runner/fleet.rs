//! The runner's loops against a dev server: routed reviews are leased,
//! reviewed by the fake backend and posted as the reviewer; conflicts are
//! resolved and stored as the resolver.

use std::time::Duration;

use rill_api::{ServerConfig, StaticToken};
use rill_review_runner::backend::Fake;
use rill_review_runner::{resolve, review};
use rill_sdk::testing::DevServer;
use rill_sdk::{pb, ApplyOptions, Client, ClientConfig, Edit, EditBase, RepoClient, Root};
use rill_types::{ChangeId, PrincipalKind, RepoPathBuf};

fn path(p: &str) -> RepoPathBuf {
    RepoPathBuf::new(p).unwrap()
}

async fn start() -> (DevServer, Client) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("rill_review_runner=info")
        .with_test_writer()
        .try_init();
    let mut config: ServerConfig = DevServer::config();
    for name in ["sec", "fixer"] {
        config.auth.tokens.push(StaticToken {
            token: format!("{name}-token"),
            principal_name: name.into(),
            kind: PrincipalKind::Agent,
            teams: vec![],
        });
    }
    let server = DevServer::start_with(config).await.unwrap();
    let dev = server.client().await.unwrap();
    for (name, reviewer) in [("sec", true), ("fixer", false)] {
        dev.identity("default")
            .create_agent(pb::AgentSpec {
                org: "default".into(),
                name: name.into(),
                sponsor: "user:dev".into(),
                repos: vec!["*".into()],
                reviewer: reviewer
                    .then(|| pb::ReviewerProfile {
                        model_family: "fake".into(),
                        model: "fake".into(),
                        max_vote: 1,
                        ..Default::default()
                    })
                    .into(),
                ..Default::default()
            })
            .await
            .unwrap();
    }
    dev.create_repo("fleet", None).await.unwrap();
    (server, dev)
}

async fn as_agent(server: &DevServer, name: &str) -> RepoClient {
    let client =
        Client::connect(ClientConfig::new(server.endpoint()).with_token(format!("{name}-token")))
            .await
            .unwrap();
    client.repo("fleet")
}

/// Write `files` as a new change on `base` and submit it.
async fn change(
    repo: &RepoClient,
    base: Root,
    files: &[(&str, &str)],
) -> (ChangeId, rill_types::ChangeVersionHash) {
    let edits = files
        .iter()
        .map(|(p, t)| Edit::set(path(p), t.to_string()))
        .collect();
    let v = repo
        .apply_edits(EditBase::Root(base), edits, ApplyOptions::message("edit"))
        .await
        .unwrap();
    repo.reviews().submit(v.version.change_id).await.unwrap();
    (v.version.change_id, v.hash)
}

async fn land(repo: &RepoClient, files: &[(&str, &str)]) {
    let (id, _) = change(repo, Root::trunk(), files).await;
    let reviews = repo.reviews();
    for (label, value) in [("Verified", 1), ("Review", 2), ("Owner", 1)] {
        reviews.vote(id, label, value, None, None).await.unwrap();
    }
    let ticket = reviews.request_land(id).await.unwrap();
    repo.stack()
        .await_land(&ticket.id, Duration::from_secs(30))
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_review_loop_posts_the_fake_reviewers_verdicts() {
    let (server, dev) = start().await;
    let repo = dev.repo("fleet");
    land(
        &repo,
        &[(
            ".rill/review.toml",
            "[[reviewer]]\nagent = \"sec\"\npaths = [\"src/**\"]\n",
        )],
    )
    .await;
    let (clean, _) = change(&repo, Root::trunk(), &[("src/a.rs", "fn a() {}\n")]).await;
    let (seeded, _) = change(
        &repo,
        Root::trunk(),
        &[(
            "src/b.rs",
            "fn b() {}\nlet x = 1; // RILL-DEFECT: off by one\n",
        )],
    )
    .await;
    change(&repo, Root::trunk(), &[("docs/x.md", "x\n")]).await;

    let sec = as_agent(&server, "sec").await;
    let fake = Fake {
        detect_permille: 1000,
        false_positive_permille: 0,
    };
    let mut done = 0;
    for _ in 0..10 {
        done += review::run(&sec.reviewers(), &fake, 2_000, true)
            .await
            .unwrap()
            .done;
        if done >= 2 {
            break;
        }
    }
    assert_eq!(done, 2, "reviewed the two src/ changes, not docs/");
    let runs = repo.reviewers().runs(clean).await.unwrap();
    assert_eq!((runs.len(), runs[0].vote, runs[0].findings), (1, 1, 0));
    let runs = repo.reviewers().runs(seeded).await.unwrap();
    assert_eq!((runs[0].vote, runs[0].findings), (-1, 1));
    let review = repo.reviews().get(seeded).await.unwrap();
    let thread = &review.review.threads[0];
    assert_eq!(thread.comments[0].author.name, "sec");
    assert!(thread.comments[0].body.contains("off by one"));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_resolve_loop_stores_a_resolution_as_the_resolver() {
    let (server, dev) = start().await;
    let repo = dev.repo("fleet");
    land(
        &repo,
        &[
            (".rill/review.toml", "[resolver]\nagent = \"fixer\"\n"),
            ("src/a.rs", "base\n"),
        ],
    )
    .await;
    let (id, v1) = change(&repo, Root::trunk(), &[("src/a.rs", "ours\n")]).await;
    land(&repo, &[("src/a.rs", "theirs\n")]).await;
    let head = repo.resolve_root(&Root::trunk()).await.unwrap();
    let rebased = repo.rebase(v1, head).await.unwrap();
    assert!(rebased.version.has_conflicts(), "the rebase conflicts");
    assert_eq!(
        repo.reviews()
            .get(id)
            .await
            .unwrap()
            .review
            .current_version(),
        Some(rebased.hash)
    );

    let fixer = as_agent(&server, "fixer").await;

    let fake = Fake {
        detect_permille: 1000,
        false_positive_permille: 0,
    };
    let mut done = 0;
    for _ in 0..10 {
        done += resolve::run(&fixer, &fake, 2_000, true).await.unwrap().done;
        if done >= 1 {
            break;
        }
    }
    assert_eq!(done, 1, "{:?}", repo.reviewers().runs(id).await.unwrap());
    let current = repo.current_version(id).await.unwrap();
    let text = repo
        .read_file(&Root::Version(current.hash), &path("src/a.rs"))
        .await
        .unwrap();
    assert_eq!(&text[..], b"theirs\nours\n", "every side kept, trunk first");
    let runs = repo.reviewers().runs(id).await.unwrap();
    assert_eq!(runs[0].kind, "resolve");
    assert!(
        runs[0].detail.starts_with("resolved as"),
        "{}",
        runs[0].detail
    );
}
