//! A registered agent for tests that need one.

use rill_types::identity::{AccountStatus, AgentBudgets, AgentRecord, Sponsor, UserRecord};
use rill_types::{Principal, Timestamp};

/// Register agent `bot` in the default org, sponsored by user `ann`, with
/// `budgets`.
pub(crate) async fn register_bot(stores: &rill_store::Stores, budgets: AgentBudgets) {
    stores.ensure_default_org().await.unwrap();
    let root = Principal::human("root");
    let now = Timestamp::now();
    stores
        .identity()
        .create_user(
            &UserRecord {
                org: "default".into(),
                principal: Principal::human("ann"),
                email: None,
                display_name: None,
                external: vec![],
                status: AccountStatus::Active,
                roles: vec![],
                teams: vec![],
                scim_id: None,
                external_id: None,
                created_at: now,
                updated_at: now,
            },
            &root,
        )
        .await
        .unwrap();
    stores
        .identity()
        .create_agent(
            &AgentRecord {
                org: "default".into(),
                principal: Principal::agent("bot"),
                sponsor: Sponsor::User("ann".into()),
                purpose: "tests".into(),
                repos: vec!["*".into()],
                paths: vec![],
                models: vec![],
                budgets,
                workloads: vec![],
                status: AccountStatus::Active,
                expires_at: None,
                created_by: root.clone(),
                created_at: now,
                updated_at: now,
                reviewer: None,
            },
            &root,
        )
        .await
        .unwrap();
}
