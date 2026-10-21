//! The audit log: queued records become one unbroken chain per org, even
//! with several chainers and projectors at once.

use std::collections::BTreeMap;

use rill_store::Stores;
use rill_types::audit::{AuditHead, AuditOutcome, AuditRecord, AuditRoot};
use rill_types::{ObjectHash, ObjectKind, Principal, Timestamp, Versionstamp};

fn record(org: &str, n: u32) -> AuditRecord {
    AuditRecord {
        org: org.into(),
        at: Timestamp::from_millis(n as i64),
        actor: Principal::human("ann"),
        action: "test.act".into(),
        target: format!("thing:{n}"),
        outcome: AuditOutcome::Allowed,
        detail: BTreeMap::new(),
    }
}

pub async fn scenario(name: &str, stores: &Stores) {
    let audit = stores.audit();
    for n in 0..40 {
        audit.record(&record("acme", n)).await.unwrap();
        audit.record(&record("globex", n)).await.unwrap();
    }
    assert_eq!(
        audit.pending_orgs().await.unwrap(),
        ["acme", "globex"],
        "{name}"
    );

    // Four chainers race; the chain still has every record once, in order.
    let chainers: Vec<_> = (0..4)
        .map(|_| {
            let audit = audit.clone();
            tokio::spawn(async move {
                for _ in 0..50 {
                    for org in ["acme", "globex"] {
                        audit.chain(org).await.unwrap();
                    }
                }
            })
        })
        .collect();
    for c in chainers {
        c.await.unwrap();
    }
    for org in ["acme", "globex"] {
        let entries = audit.entries(org, 0, 1000).await.unwrap();
        assert_eq!(entries.len(), 40, "{name}: {org}");
        let mut head = AuditHead::default();
        for (i, entry) in entries.iter().enumerate() {
            assert_eq!(entry.seq, i as u64 + 1, "{name}");
            assert_eq!(entry.prev, head.hash, "{name}: {org} link {}", entry.seq);
            assert_eq!(entry.record.org, org);
            head = AuditHead {
                seq: entry.seq,
                hash: entry.hash(),
            };
        }
        assert_eq!(audit.head(org).await.unwrap(), head, "{name}");
        assert_eq!(audit.entries(org, 38, 10).await.unwrap().len(), 2, "{name}");
    }
    assert!(audit.pending_orgs().await.unwrap().is_empty(), "{name}");

    // A cursor moves once from where it was read.
    let (start, raw) = audit.cursor("identity").await.unwrap();
    assert_eq!(start, None, "{name}");
    let to = Versionstamp([1; 12]);
    audit
        .project("identity", raw.clone(), to, &[record("acme", 99)])
        .await
        .unwrap();
    assert!(
        audit
            .project("identity", raw, to, &[record("acme", 99)])
            .await
            .unwrap_err()
            .is_conflict(),
        "{name}: a second projector of the same entries loses"
    );
    assert_eq!(
        audit.cursor("identity").await.unwrap().0,
        Some(to),
        "{name}"
    );

    let root = AuditRoot {
        org: "acme".into(),
        first_seq: 1,
        last_seq: 40,
        head: [7; 32],
        export: ObjectHash::digest(ObjectKind::Blob, b"x"),
        sealed_at: Timestamp::from_millis(5),
        key_id: 1,
        public_key: vec![1],
        signature: vec![2],
    };
    audit.put_root(&root).await.unwrap();
    assert!(
        audit.put_root(&root).await.is_err(),
        "{name}: one root per seal"
    );
    let overlapping = AuditRoot {
        last_seq: 42,
        ..root.clone()
    };
    assert!(
        audit
            .put_root(&overlapping)
            .await
            .unwrap_err()
            .is_conflict(),
        "{name}: a racing sealer that read no root cannot reseal sealed entries"
    );
    assert_eq!(
        audit.latest_root("acme").await.unwrap(),
        Some(root.clone()),
        "{name}"
    );
    let next = AuditRoot {
        first_seq: 41,
        last_seq: 42,
        ..root
    };
    audit.put_root(&next).await.unwrap();
    assert_eq!(audit.roots("acme").await.unwrap().len(), 2, "{name}");
}
