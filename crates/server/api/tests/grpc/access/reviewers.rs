//! The reviewer fleet: routing by `.rill/review.toml`, leasing as the
//! reviewer agent, the server's checks on results, independence of agent
//! approvals, calibration from outcomes, and resolver dispatch for
//! conflicts.

use rill_types::identity::{AccountStatus, AgentBudgets, AgentRecord, ReviewerProfile, Sponsor};
use rill_types::{Principal, Timestamp};

use super::super::*;
use super::fixture::*;
use super::writes::{change_of, edit, vote};

pub(super) async fn add_agent(
    server: &TestServer,
    name: &str,
    sponsor: &str,
    family: Option<&str>,
) {
    let now = Timestamp::now();
    let reviewer = family.map(|f| ReviewerProfile {
        model_family: f.into(),
        model: format!("{f}-model"),
        prompt: format!(".rill/reviewers/{name}/prompt.md"),
        max_vote: 1,
        runs_per_day: None,
    });
    server
        .state
        .identity
        .store()
        .create_agent(
            &AgentRecord {
                org: "default".into(),
                principal: Principal::agent(name),
                sponsor: Sponsor::User(sponsor.into()),
                purpose: "tests".into(),
                repos: vec!["*".into()],
                paths: vec![],
                models: vec![],
                budgets: AgentBudgets::default(),
                workloads: vec![],
                status: AccountStatus::Active,
                expires_at: None,
                created_by: Principal::human("admin"),
                created_at: now,
                updated_at: now,
                reviewer,
            },
            &Principal::human("admin"),
        )
        .await
        .unwrap();
}

async fn lease(c: &mut Clients) -> Result<pb::LeaseReviewJobResponse, Code> {
    c.reviewers
        .lease_review_job(pb::LeaseReviewJobRequest {
            parent: common::names::repo("acl"),
            wait_ms: 10_000,
        })
        .await
        .map(|r| r.into_owned())
        .map_err(|s| s.code)
}

async fn complete(
    c: &mut Clients,
    lease: &Bytes,
    findings: Vec<pb::ReviewFinding>,
    vote: i32,
) -> Result<pb::ReviewRun, Code> {
    c.reviewers
        .complete_review_job(pb::CompleteReviewJobRequest {
            parent: common::names::repo("acl"),
            lease: lease.clone(),
            findings,
            vote,
            summary: "reviewed".into(),
            model: "test-model".into(),
            ..Default::default()
        })
        .await
        .map(|r| r.into_owned().run.unwrap())
        .map_err(|s| s.code)
}

fn finding(path: &str) -> pb::ReviewFinding {
    pb::ReviewFinding {
        path: path.into(),
        start_line: 1,
        end_line: 1,
        category: pb::CommentCategory::Suggestion.into(),
        confidence: 80,
        body: "consider a doc comment".into(),
    }
}

async fn runs(c: &mut Clients, change: &Bytes) -> Vec<pb::ReviewRun> {
    c.reviewers
        .list_review_runs(pb::ListReviewRunsRequest {
            change: common::names::change("acl", change),
        })
        .await
        .unwrap()
        .into_owned()
        .runs
}

async fn land_config(acl: &mut Acl, files: &[(&str, &str)]) {
    let edits = files
        .iter()
        .map(|(p, t)| set_edit(p, t.as_bytes()))
        .collect();
    let v = edit(&mut acl.owner, None, edits).await.unwrap();
    let change = change_of(&mut acl.owner, &v).await;
    land_with(&mut acl.owner, "acl", change, &[("Owner", 1)]).await;
}

#[tokio::test]
async fn routed_reviewers_post_checked_results_and_count_when_independent() {
    let mut acl = Acl::start().await;
    for user in ["ann", "cara"] {
        add_user(&acl.server, user, vec![], &[]).await;
    }
    add_agent(&acl.server, "sec", "ann", Some("claude")).await;
    add_agent(&acl.server, "perf", "ann", Some("gpt")).await;
    add_agent(&acl.server, "twin", "cara", Some("gemini")).await;
    add_agent(&acl.server, "coder", "cara", None).await;
    for agent in ["sec", "perf", "twin"] {
        acl.grant("", "reviewer", &Principal::agent(agent)).await;
    }
    acl.grant("pub", "writer", &Principal::agent("coder")).await;
    assert_eq!(
        edit(
            &mut acl.owner,
            None,
            vec![set_edit(
                ".rill/review.toml",
                b"[[reviewer]]\nagent = \"user:x\"\n"
            )]
        )
        .await,
        Err(Code::InvalidArgument),
        "a bad routing file is refused when stored"
    );
    land_config(
        &mut acl,
        &[
            (
                ".rill/review.toml",
                "[[reviewer]]\nagent = \"sec\"\npaths = [\"pub/**\"]\n[[reviewer]]\nagent = \"perf\"\npaths = [\"pub/**\"]\n",
            ),
            (".rill/reviewers/sec/prompt.md", "Look for security bugs.\n"),
        ],
    )
    .await;

    let mut coder = acl.as_principal(&Principal::agent("coder")).await;
    let version = edit(&mut coder, None, vec![set_edit("pub/b.rs", b"fn b() {}\n")])
        .await
        .unwrap();
    let change = change_of(&mut coder, &version).await;
    coder
        .review
        .submit_change(pb::SubmitChangeRequest {
            request_id: String::new(),
            change: common::names::change("acl", &change),
        })
        .await
        .unwrap();

    assert_eq!(
        lease(&mut acl.owner).await.unwrap_err(),
        Code::FailedPrecondition,
        "a human leases nothing"
    );
    let mut sec = acl.as_principal(&Principal::agent("sec")).await;
    let job = lease(&mut sec).await.unwrap();
    let held = job.lease.clone();
    let job = job.job.unwrap();
    assert_eq!(job.prompt, "Look for security bugs.\n");
    assert_eq!(job.reason, "submitted");
    let packet = job.packet.unwrap();
    assert_eq!(packet.files.len(), 1);
    assert!(
        packet.files[0].unified.contains("+fn b() {}"),
        "{:?}",
        packet.files[0]
    );

    let mut perf = acl.as_principal(&Principal::agent("perf")).await;
    assert_eq!(
        complete(&mut perf, &held, vec![], 1).await.unwrap_err(),
        Code::PermissionDenied,
        "another agent's lease"
    );
    assert_eq!(
        complete(&mut sec, &held, vec![finding("secret/s.txt")], 1)
            .await
            .unwrap_err(),
        Code::InvalidArgument,
        "a finding on a path the change does not touch"
    );
    let run = complete(&mut sec, &held, vec![], 1).await;
    assert!(run.is_err(), "the refused result ended the job");

    let perf_job = lease(&mut perf).await.unwrap();
    assert_eq!(
        complete(&mut perf, &perf_job.lease, vec![], 2)
            .await
            .unwrap_err(),
        Code::InvalidArgument,
        "a vote over the reviewer's maximum"
    );

    // Ask both again: the refused runs failed, so new jobs are queued.
    let requested = acl
        .owner
        .reviewers
        .request_review(pb::RequestReviewRequest {
            change: common::names::change("acl", &change),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .runs;
    assert_eq!(requested.len(), 2);
    let again = lease(&mut sec).await.unwrap();
    let run = complete(&mut sec, &again.lease, vec![finding("pub/b.rs")], 1)
        .await
        .unwrap();
    assert_eq!((run.findings, run.vote), (1, 1));
    let decision = |c: &mut Clients| {
        let review = c.review.clone();
        let change = change.clone();
        async move {
            review
                .evaluate_landing(pb::EvaluateLandingRequest {
                    change: common::names::change("acl", &change),
                })
                .await
                .unwrap()
                .into_owned()
                .decision
                .unwrap()
        }
    };
    vote(&mut acl.owner, &change, "Owner", 1).await.unwrap();
    vote(&mut acl.owner, &change, "Verified", 1).await.unwrap();
    // The twin shares the author's sponsor: its approval does not count.
    let mut twin = acl.as_principal(&Principal::agent("twin")).await;
    vote(&mut twin, &change, "Review", 1).await.unwrap();
    let d = decision(&mut acl.owner).await;
    assert!(
        !d.allowed,
        "one independent approval: {:?}",
        d.matched_policies
    );

    let again = lease(&mut perf).await.unwrap();
    complete(&mut perf, &again.lease, vec![], 1).await.unwrap();
    let d = decision(&mut acl.owner).await;
    assert!(d.allowed, "{:?}", d.reasons);
    assert!(d
        .matched_policies
        .contains(&"land_normal_agent_review".to_string()));

    let all = runs(&mut acl.owner, &change).await;
    let count = |status: pb::ReviewRunStatus| {
        all.iter()
            .filter(|r| r.status == buffa::EnumValue::from(status))
            .count()
    };
    assert_eq!(
        (
            count(pb::ReviewRunStatus::Done),
            count(pb::ReviewRunStatus::Failed)
        ),
        (2, 2),
        "{all:?}"
    );

    // Landing credits both approvers; reverting the landing makes it an
    // escape for each.
    let seq = common::request_land(&mut acl.owner, "acl", change.clone())
        .await
        .unwrap();
    acl.owner
        .trunk
        .revert_landing(pb::RevertLandingRequest {
            landing: common::names::position("acl", seq),
            reason: "broke prod".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let stats = acl
            .owner
            .reviewers
            .list_reviewer_stats(pb::ListReviewerStatsRequest {
                parent: common::names::repo("acl"),
            })
            .await
            .unwrap()
            .into_owned()
            .reviewers;
        let sec = stats.iter().find(|s| s.reviewer == "agent:sec");
        if sec.is_some_and(|s| s.approvals == 1 && s.escapes == 1) {
            assert!(sec.unwrap().trust < 500, "{sec:?}");
            break;
        }
        assert!(std::time::Instant::now() < deadline, "{stats:?}");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(
        acl.owner
            .reviewers
            .reinstate_reviewer(pb::ReinstateReviewerRequest {
                parent: common::names::repo("acl"),
                reviewer: "sec".into(),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_owned()
            .stats
            .unwrap()
            .outcomes,
        0
    );
    acl.server.stop().await;
}

#[tokio::test]
async fn a_conflicted_version_goes_to_the_resolver() {
    let mut acl = Acl::start().await;
    add_user(&acl.server, "ann", vec![], &[]).await;
    add_agent(&acl.server, "coder", "ann", None).await;
    add_agent(&acl.server, "fixer", "ann", None).await;
    for agent in ["coder", "fixer"] {
        acl.grant("pub", "writer", &Principal::agent(agent)).await;
    }
    land_config(
        &mut acl,
        &[(
            ".rill/review.toml",
            "[resolver]\nagent = \"fixer\"\npaths = [\"pub/**\"]\n",
        )],
    )
    .await;
    let base = acl.at_version();
    let mut coder = acl.as_principal(&Principal::agent("coder")).await;
    let v1 = edit(
        &mut coder,
        Some(base),
        vec![set_edit("pub/a.txt", b"ours\n")],
    )
    .await
    .unwrap();
    let change = change_of(&mut coder, &v1).await;
    coder
        .review
        .submit_change(pb::SubmitChangeRequest {
            request_id: String::new(),
            change: common::names::change("acl", &change),
        })
        .await
        .unwrap();
    land_config(&mut acl, &[("pub/a.txt", "theirs\n")]).await;
    let v2 = coder
        .changes
        .rebase_version(pb::RebaseVersionRequest {
            name: common::names::version("acl", &v1),
            onto: common::trunk(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .info
        .unwrap()
        .hash;

    let fixer = acl.as_principal(&Principal::agent("fixer")).await;
    let leased = fixer
        .reviewers
        .lease_resolve_job(pb::LeaseResolveJobRequest {
            parent: common::names::repo("acl"),
            wait_ms: 10_000,
        })
        .await
        .unwrap()
        .into_owned();
    let job = leased.job.unwrap();
    assert_eq!(job.paths, ["pub/a.txt"]);
    assert_eq!(job.version, common::names::version("acl", &v2));
    let Some(pb::edit::Op::SetContent(content)) = set_edit("pub/a.txt", b"both\n").op else {
        unreachable!()
    };
    let v3 = fixer
        .changes
        .resolve_conflicts(pb::ResolveConflictsRequest {
            name: job.version.clone(),
            resolutions: vec![pb::Resolution {
                path: "pub/a.txt".into(),
                resolution: Some(pb::resolution::Resolution::Set(content)),
            }],
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .info
        .unwrap()
        .hash;
    let run = fixer
        .reviewers
        .complete_resolve_job(pb::CompleteResolveJobRequest {
            parent: common::names::repo("acl"),
            lease: leased.lease,
            resolved_version: common::names::version("acl", &v3),
            rationale: "kept both".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .run
        .unwrap();
    assert_eq!(run.kind, "resolve");
    assert!(run.detail.starts_with("resolved as"), "{}", run.detail);
    acl.server.stop().await;
}
