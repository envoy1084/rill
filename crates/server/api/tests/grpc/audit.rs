//! The audit log end to end: actions reach their org's chain, seals sign
//! it, and any edit or deletion of a stored entry is caught.

use std::collections::HashMap;

use rill_store::keys::{identity_pair, table};
use rill_store::meta::Transaction;
use rill_types::audit::{AuditEntry, AuditRoot};
use rill_types::Principal;

use super::*;

async fn chain(c: &mut Clients, org: &str) -> Vec<AuditEntry> {
    c.audit
        .list_audit_entries(pb::ListAuditEntriesRequest {
            org: org.into(),
            after_seq: 0,
            limit: 0,
        })
        .await
        .unwrap()
        .into_inner()
        .entries
        .iter()
        .map(|e| postcard::from_bytes(&e.encoded).unwrap())
        .collect()
}

async fn verify(
    c: &mut Clients,
    org: &str,
) -> Result<rill_policy::audit::ChainReport, rill_policy::audit::ChainBreak> {
    let entries = chain(c, org).await;
    let roots: Vec<AuditRoot> = c
        .audit
        .list_audit_roots(pb::ListAuditRootsRequest { org: org.into() })
        .await
        .unwrap()
        .into_inner()
        .roots
        .iter()
        .map(|r| postcard::from_bytes(&r.encoded).unwrap())
        .collect();
    let keys: HashMap<u32, Vec<u8>> = c
        .audit
        .list_audit_keys(pb::ListAuditKeysRequest {})
        .await
        .unwrap()
        .into_inner()
        .keys
        .into_iter()
        .map(|k| (k.id, k.public_key.to_vec()))
        .collect();
    rill_policy::audit::verify_chain(&entries, &roots, &keys)
}

#[tokio::test(flavor = "multi_thread")]
async fn actions_are_chained_sealed_and_tamper_evident() {
    let mut config = ServerConfig::default();
    config.identity.admins = vec!["root".into()];
    let server = TestServer::start(config).await;
    let mut root = clients(&server, Some(&server.token_for(&Principal::human("root")))).await;
    root.repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: "audited".into(),
            description: None,
            windows_safe: false,
        })
        .await
        .unwrap();
    root.access
        .add_relation(pb::AddRelationRequest {
            operation_id: Bytes::new(),
            relation: Some(pb::Relation {
                repo: "audited".into(),
                path: "src".into(),
                relation: "reader".into(),
                subject: format!("principal:{}", Principal::human("ann").id),
            }),
        })
        .await
        .unwrap();
    root.admin
        .create_org(pb::CreateOrgRequest {
            operation_id: Bytes::new(),
            name: "acme".into(),
            description: None,
        })
        .await
        .unwrap();

    // Sealing projects and chains what is outstanding first.
    let sealed = root
        .audit
        .seal_audit(pb::SealAuditRequest {
            org: "default".into(),
        })
        .await
        .unwrap()
        .into_inner()
        .root
        .expect("the chain grew");
    let entries = chain(&mut root, "default").await;
    let actions: Vec<_> = entries.iter().map(|e| e.record.action.as_str()).collect();
    for expected in ["repo.create", "relation.add"] {
        assert!(
            actions.contains(&expected),
            "{expected} missing from {actions:?}"
        );
    }
    let add = entries
        .iter()
        .find(|e| e.record.action == "relation.add")
        .unwrap();
    assert_eq!(add.record.actor.name, "root");
    assert_eq!(add.record.target, "repo:audited");
    let instance = chain(&mut root, "_instance").await;
    assert!(instance
        .iter()
        .any(|e| e.record.action == "org.create" && e.record.target == "org:acme"));
    assert_eq!(sealed.last_seq, entries.len() as u64);

    // The export is the sealed entries, in the org's own object store.
    let export = rill_types::ObjectHash::from_slice(&sealed.export).unwrap();
    let lines = server
        .state
        .stores
        .org("default")
        .objects
        .get(rill_types::ObjectKind::Blob, &export)
        .await
        .unwrap()
        .expect("the export is stored");
    assert_eq!(lines.iter().filter(|b| **b == b'\n').count(), entries.len());

    let report = verify(&mut root, "default").await.unwrap();
    assert_eq!(report.sealed_through, entries.len() as u64);

    // Someone with the metadata store rewrites who added the relation.
    let meta = server.state.stores.meta.clone();
    let key = identity_pair(table::AUDIT, "default", add.seq as i64);
    let mut forged = add.clone();
    forged.record.actor = Principal::human("ann");
    meta.transact(Transaction::new().set(
        key.clone(),
        rill_store::repo::encode_record(&forged).unwrap(),
    ))
    .await
    .unwrap();
    let broken = verify(&mut root, "default").await.unwrap_err();
    assert!(broken.seq <= add.seq + 1, "{broken}");

    // Or deletes it.
    meta.transact(Transaction::new().delete(key)).await.unwrap();
    assert!(verify(&mut root, "default").await.is_err());

    // Only administrators read it.
    let mut ann = clients(&server, Some(&server.token_for(&Principal::human("ann")))).await;
    let refused = ann
        .audit
        .list_audit_entries(pb::ListAuditEntriesRequest {
            org: "default".into(),
            after_seq: 0,
            limit: 0,
        })
        .await
        .unwrap_err();
    assert_eq!(refused.code(), Code::PermissionDenied);
    server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn refusals_are_audited_but_sampled() {
    let mut config = ServerConfig::default();
    config.identity.admins = vec!["root".into()];
    let server = TestServer::start(config).await;
    let mut root = clients(&server, Some(&server.token_for(&Principal::human("root")))).await;
    root.repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: "closed".into(),
            description: None,
            windows_safe: false,
        })
        .await
        .unwrap();
    let mut eve = clients(&server, Some(&server.token_for(&Principal::human("eve")))).await;
    for _ in 0..5 {
        let refused = eve
            .repo
            .list_dir(pb::ListDirRequest {
                repo: "closed".into(),
                root: None,
                path: "src".into(),
            })
            .await
            .unwrap_err();
        assert_eq!(refused.code(), Code::PermissionDenied);
    }
    let mut denied = Vec::new();
    for _ in 0..50 {
        rill_runtime::audit::worker::catch_up(&server.state)
            .await
            .unwrap();
        denied = chain(&mut root, "default")
            .await
            .into_iter()
            .filter(|e| e.record.actor.name == "eve")
            .collect();
        if !denied.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(denied.len(), 1, "five refusals in a minute are noted once");
    assert_eq!(denied[0].record.action, "repo.read");
    assert!(matches!(
        denied[0].record.outcome,
        rill_types::audit::AuditOutcome::Denied { .. }
    ));
    server.stop().await;
}
