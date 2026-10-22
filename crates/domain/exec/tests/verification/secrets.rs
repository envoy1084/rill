//! Org secrets reach the steps that name them, as environment variables,
//! and nowhere else: not the action, not the cache, not the logs.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use prost::Message;
use rill_exec::remote::secrets::SecretScope;
use rill_exec::remote::{BlobDigest, ExecSpace};
use rill_exec::verify::StepStatus;
use rill_reapi_proto::reapi::Action;
use rill_store::{DataKey, DataKeySource, KeyRange, OrgSealer, Sealer, DEFAULT_ORG};
use rill_types::{Principal, Timestamp};

use crate::fixture::Fixture;

const VALUE: &str = "tok-5ecret-value";

const PLAN: &str = r#"
[[step]]
name = "publish"
command = ["sh", "-c", 'echo "using $API_TOKEN"; test ${#API_TOKEN} -eq 16']
secrets = ["API_TOKEN"]
"#;

async fn read(space: &ExecSpace, digest: BlobDigest) -> String {
    String::from_utf8(space.read(&digest).await.unwrap().unwrap().to_vec()).unwrap()
}

#[tokio::test]
async fn a_step_gets_its_secret_but_its_action_and_log_never_hold_it() {
    let fx = Fixture::new().await;
    fx.scheduler
        .put_secret(
            DEFAULT_ORG,
            "API_TOKEN",
            VALUE.as_bytes(),
            SecretScope::default(),
            &Principal::human("admin"),
            vec![],
        )
        .await
        .unwrap();
    let root = fx.tree(None, &[("PLAN", PLAN)]).await;

    let outcome = fx.verify(root, None).await;
    assert!(outcome.green, "{} {:?}", outcome.summary(), outcome.excerpt);
    let step = &outcome.steps[0];
    let space = fx.scheduler.space(DEFAULT_ORG);
    assert_eq!(
        read(&space, step.stdout.unwrap()).await,
        "using [secret API_TOKEN]\n"
    );

    let action = Action::decode(space.read(&step.action.unwrap()).await.unwrap().unwrap()).unwrap();
    assert!(action.do_not_cache);
    let command = read(
        &space,
        BlobDigest::parse(action.command_digest.as_ref().unwrap()).unwrap(),
    )
    .await;
    assert!(command.contains("API_TOKEN") && !command.contains(VALUE));
    let leftovers = space
        .records
        .meta()
        .scan(KeyRange::prefix(
            space.records.keys().exec_secret_commands(),
        ))
        .await
        .unwrap();
    assert!(
        leftovers.is_empty(),
        "the command with values outlived its action"
    );

    let again = fx.verify(root, None).await;
    assert!(
        again.green && !again.steps[0].cached,
        "a step with secrets is never cached"
    );
}

#[tokio::test]
async fn a_failing_steps_excerpt_is_masked() {
    let fx = Fixture::new().await;
    fx.scheduler
        .put_secret(
            DEFAULT_ORG,
            "API_TOKEN",
            VALUE.as_bytes(),
            SecretScope::default(),
            &Principal::human("admin"),
            vec![],
        )
        .await
        .unwrap();
    let plan = r#"
[[step]]
command = ["sh", "-c", 'echo "bad token $API_TOKEN" >&2; exit 1']
secrets = ["API_TOKEN"]
"#;
    let outcome = fx
        .verify(fx.tree(None, &[("PLAN", plan)]).await, None)
        .await;
    assert!(!outcome.green);
    assert_eq!(
        outcome.excerpt.as_deref(),
        Some("bad token [secret API_TOKEN]\n")
    );
}

#[tokio::test]
async fn a_secret_reaches_only_the_repositories_and_stages_it_allows() {
    let fx = Fixture::new().await;
    let root = fx.tree(None, &[("PLAN", PLAN)]).await;
    let error = |outcome: rill_exec::VerificationOutcome| match &outcome.steps[0].status {
        StepStatus::Error(reason) => reason.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        error(fx.verify(root, None).await),
        "secret API_TOKEN is not set"
    );

    let put = |scope| {
        let scheduler = fx.scheduler.clone();
        async move {
            scheduler
                .put_secret(
                    DEFAULT_ORG,
                    "API_TOKEN",
                    VALUE.as_bytes(),
                    scope,
                    &Principal::human("admin"),
                    vec![],
                )
                .await
                .unwrap();
        }
    };
    put(SecretScope {
        repos: vec!["other".into()],
        trunk_only: false,
    })
    .await;
    assert_eq!(
        error(fx.verify(root, None).await),
        "secret API_TOKEN is not available to app"
    );

    put(SecretScope {
        repos: vec!["app".into()],
        trunk_only: true,
    })
    .await;
    assert_eq!(
        error(fx.verify(root, None).await),
        "secret API_TOKEN is only available after landing"
    );
    assert!(fx.verify_at(root, None, true).await.green);
}

struct Fixed(HashMap<String, Vec<DataKey>>);

#[async_trait]
impl DataKeySource for Fixed {
    async fn data_keys(&self, org: &str) -> rill_store::Result<Vec<DataKey>> {
        Ok(self.0.get(org).cloned().unwrap_or_default())
    }
}

/// A fixture sealing with real org data keys.
pub async fn sealed_fixture() -> Fixture {
    let sealer = Arc::new(OrgSealer::new());
    sealer.set_source(Arc::new(Fixed(HashMap::from([(
        DEFAULT_ORG.to_string(),
        vec![DataKey {
            id: 1,
            secret: [7; 32],
            active_from: Timestamp::from_millis(0),
        }],
    )]))));
    Fixture::sealed_by(sealer as Arc<dyn Sealer>).await
}

#[tokio::test]
async fn secret_values_are_stored_sealed_under_the_org_keys() {
    let fx = sealed_fixture().await;
    fx.scheduler
        .put_secret(
            DEFAULT_ORG,
            "API_TOKEN",
            VALUE.as_bytes(),
            SecretScope::default(),
            &Principal::human("admin"),
            vec![],
        )
        .await
        .unwrap();
    let space = fx.scheduler.space(DEFAULT_ORG);
    let stored = space
        .records
        .meta()
        .get(&space.records.keys().exec_secret("API_TOKEN"))
        .await
        .unwrap()
        .unwrap();
    assert!(!stored.windows(VALUE.len()).any(|w| w == VALUE.as_bytes()));

    let outcome = fx
        .verify(fx.tree(None, &[("PLAN", PLAN)]).await, None)
        .await;
    assert!(outcome.green, "{:?}", outcome);
    assert_eq!(
        fx.scheduler.list_secrets(DEFAULT_ORG).await.unwrap()[0].name,
        "API_TOKEN"
    );
}
