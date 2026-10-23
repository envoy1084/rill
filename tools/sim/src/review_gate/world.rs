//! The gate's repository: people, agents, a reviewer fleet, approved
//! intents over its directories, and the changes the agents write.

use std::sync::Arc;

use rill_core::{ChangeBuilder, CommitGraph, DeriveManager, TreeOps};
use rill_policy::{Person, RiskConfig, StaticDirectory, SubmitPolicy};
use rill_review::{LabelSet, ReviewService};
use rill_store::{RepoStore, Stores};
use rill_types::identity::ReviewerProfile;
use rill_types::{
    ChangeId, ChangeVersionHash, Intent, IntentApproval, IntentKind, IntentScope, Principal,
    Timestamp,
};

use super::GateConfig;
use crate::rng::SimRng;

/// Directories changes land in; intents cover them.
pub(super) const DIRS: usize = 100;
/// Approved intents.
const INTENTS: usize = 200;
/// The marker the fleet's fake reviewers look for.
pub(super) use rill_review_runner::backend::DEFECT_MARKER;

/// One change the gate wrote.
pub struct GateChange {
    pub id: ChangeId,
    pub version: ChangeVersionHash,
    pub author: Principal,
    pub sponsor: Principal,
    pub defect: bool,
    pub sensitive: bool,
}

/// The repository and the people in it.
pub struct GateWorld {
    pub repo: RepoStore,
    pub service: ReviewService,
    root: ChangeVersionHash,
    intents: Vec<Intent>,
    /// The fleet: general reviewers, and one more for sensitive changes.
    pub(super) reviewers: Vec<Principal>,
    pub(super) security: Principal,
    /// Agent-only landings audited, per thousand.
    pub(super) audit_permille: u32,
}

fn reviewer(family: &str, prompt: &str) -> Person {
    Person {
        sponsor: Some("team:review-fleet".into()),
        reviewer: Some(ReviewerProfile {
            model_family: family.into(),
            model: format!("{family}-model"),
            prompt: prompt.into(),
            max_vote: 1,
            runs_per_day: None,
        }),
        ..Person::default()
    }
}

impl GateWorld {
    pub async fn new(config: &GateConfig, rng: &mut SimRng) -> Self {
        let stores = Stores::memory();
        let repo_config = stores.create_repo("gate", "").await.expect("memory store");
        let repo = stores.repo(repo_config.id);
        let trees = TreeOps::new(repo.objects().clone(), 4096);
        let derive = DeriveManager::new(repo.clone(), trees, 4096);
        let graph = Arc::new(CommitGraph::new(repo.clone(), derive.clone()));

        let reviewers = vec![Principal::agent("rev-claude"), Principal::agent("rev-gpt")];
        let security = Principal::agent("rev-security");
        let mut directory = StaticDirectory::default()
            .with(
                &reviewers[0],
                reviewer("claude", ".rill/reviewers/general.md"),
            )
            .with(&reviewers[1], reviewer("gpt", ".rill/reviewers/general.md"))
            .with(&security, reviewer("claude", ".rill/reviewers/security.md"));
        for a in 0..config.agents {
            directory = directory.with(
                &Principal::agent(format!("a{a}")),
                Person {
                    sponsor: Some(format!("user:h{}", a % config.humans)),
                    ..Person::default()
                },
            );
        }
        let service = ReviewService::new(
            repo.clone(),
            derive,
            graph,
            Arc::new(SubmitPolicy::default_rules()),
            Arc::new(
                RiskConfig::default()
                    .compile()
                    .expect("default risk config"),
            ),
            Arc::new(LabelSet::default()),
        )
        .with_directory(Arc::new(directory));

        let mut root = ChangeBuilder::new(Principal::human("h0")).message("root");
        for d in 0..DIRS {
            root = root
                .file(&format!("svc{d}/lib.rs"), "pub fn f() {}\n".to_string())
                .expect("valid path");
        }
        let (root, _) = root.commit(&repo).await.expect("memory store");

        let mut intents = Vec::with_capacity(INTENTS);
        for i in 0..INTENTS {
            let owner = Principal::human(format!("h{}", rng.below(config.humans)));
            let dir = format!("svc{}", i % DIRS);
            let mut intent = Intent::new(
                format!("migrate {dir}"),
                IntentKind::Refactor,
                owner.clone(),
            );
            intent.scope = Some(IntentScope {
                paths: vec![format!("{dir}/**")],
                human_sample_permille: Some(config.audit_permille),
                ..IntentScope::default()
            });
            intent.revision = 1;
            intent.approvals.push(IntentApproval {
                by: owner.clone(),
                at: Timestamp::now(),
                revision: 1,
                owns: vec![dir],
            });
            repo.put_intent(&intent, &owner)
                .await
                .expect("memory store");
            intents.push(intent);
        }
        GateWorld {
            repo,
            service,
            root,
            intents,
            reviewers,
            security,
            audit_permille: config.audit_permille,
        }
    }

    /// Change `n`: an agent edits a file under an intent (or outside every
    /// scope, or the repository's configuration), maybe with a defect.
    pub async fn write_change(
        &self,
        config: &GateConfig,
        rng: &mut SimRng,
        n: usize,
    ) -> GateChange {
        let a = rng.below(config.agents);
        let author = Principal::agent(format!("a{a}"));
        let sponsor = Principal::human(format!("h{}", a % config.humans));
        let intent = &self.intents[rng.below(self.intents.len())];
        let roll = rng.below(1000) as u32;
        let sensitive = roll < config.sensitive_permille;
        let out_of_scope =
            !sensitive && roll < config.sensitive_permille + config.out_of_scope_permille;
        let dir = &intent
            .scope
            .as_ref()
            .expect("gate intents have scopes")
            .paths[0];
        let path = if sensitive {
            format!(".rill/settings/c{n}.toml")
        } else if out_of_scope {
            format!("misc/m{n}.rs")
        } else {
            format!("{}/f{n}.rs", dir.trim_end_matches("/**"))
        };
        let defect = rng.below(1000) < config.defect_permille as usize;
        let mut text = format!("pub fn g{n}() -> u32 {{\n    {n}\n}}\n");
        if defect {
            text.push_str(&format!("let x = {n}; // {DEFECT_MARKER}: off by one\n"));
        }
        let (version, cv) = ChangeBuilder::new(author.clone())
            .parent(self.root)
            .message(format!("change {n}"))
            .intent(intent.id)
            .file(&path, text)
            .expect("valid path")
            .commit(&self.repo)
            .await
            .expect("memory store");
        self.service
            .submit(&cv.change_id, &author)
            .await
            .expect("a fresh change submits");
        GateChange {
            id: cv.change_id,
            version,
            author,
            sponsor,
            defect,
            sensitive,
        }
    }
}
