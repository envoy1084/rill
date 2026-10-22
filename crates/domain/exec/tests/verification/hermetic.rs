//! Hermeticity probes and the guards on the action cache.

use std::time::Duration;

use prost::Message;
use rill_exec::remote::BlobDigest;
use rill_reapi_proto::reapi::{ActionResult, Digest, OutputFile};
use rill_store::DEFAULT_ORG;

use crate::fixture::Fixture;

/// Waits until `label` is flagged, or panics.
async fn flagged(fx: &Fixture, label: &str) -> rill_exec::remote::cache::Nonhermetic {
    let space = fx.scheduler.space(DEFAULT_ORG);
    for _ in 0..100 {
        if let Some(flag) = fx.scheduler.nonhermetic(&space, label).await.unwrap() {
            return flag;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("{label} was never flagged");
}

#[tokio::test]
async fn a_step_whose_rerun_differs_is_flagged_and_no_longer_cached() {
    let fx = Fixture::probing(1.0).await;
    let plan = r#"
[[step]]
name = "stamp"
command = ["sh", "-c", "od -An -N8 -tx8 /dev/urandom > stamp"]
outputs = ["stamp"]
"#;
    let root = fx.tree(None, &[("PLAN", plan)]).await;
    let first = fx.verify(root, None).await;
    assert!(first.green && !first.steps[0].nonhermetic);

    let flag = flagged(&fx, "verify:app/stamp").await;
    assert_eq!(flag.differences, ["stamp differs"]);
    let space = fx.scheduler.space(DEFAULT_ORG);
    let action = first.steps[0].action.unwrap();
    assert!(fx
        .scheduler
        .cached_result(&space, &action)
        .await
        .unwrap()
        .is_none());

    for _ in 0..2 {
        let again = fx.verify(root, None).await;
        assert!(again.green);
        assert!(again.steps[0].nonhermetic && !again.steps[0].cached);
    }
}

#[tokio::test]
async fn a_hermetic_step_keeps_its_cached_result_after_a_probe() {
    let fx = Fixture::probing(1.0).await;
    let plan = "[[step]]\nname = \"same\"\ncommand = [\"sh\", \"-c\", \"echo fixed > out\"]\noutputs = [\"out\"]\n";
    let root = fx.tree(None, &[("PLAN", plan)]).await;
    assert!(fx.verify(root, None).await.green);
    // Let the probe run and be judged.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let space = fx.scheduler.space(DEFAULT_ORG);
    assert!(fx
        .scheduler
        .nonhermetic(&space, "verify:app/same")
        .await
        .unwrap()
        .is_none());
    let again = fx.verify(root, None).await;
    assert!(again.steps[0].cached);
}

#[tokio::test]
async fn a_result_naming_content_that_is_not_stored_is_neither_cached_nor_served() {
    let fx = Fixture::new().await;
    let space = fx.scheduler.space(DEFAULT_ORG);
    let action = BlobDigest::of(b"an action");
    let stored = space
        .put(bytes::Bytes::from_static(b"real output"))
        .await
        .unwrap();
    let result = |digest: Digest| ActionResult {
        output_files: vec![OutputFile {
            path: "out".into(),
            digest: Some(digest),
            ..Default::default()
        }],
        ..Default::default()
    };
    let phantom = BlobDigest::of(b"never uploaded").to_proto();
    assert!(!fx
        .scheduler
        .cache_result(&space, &action, &result(phantom))
        .await
        .unwrap());
    assert!(fx
        .scheduler
        .cached_result(&space, &action)
        .await
        .unwrap()
        .is_none());
    assert!(fx
        .scheduler
        .cache_result(&space, &action, &result(stored.to_proto()))
        .await
        .unwrap());
    assert!(fx
        .scheduler
        .cached_result(&space, &action)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn an_entry_written_around_the_scheduler_does_not_open() {
    let fx = crate::secrets::sealed_fixture().await;
    let space = fx.scheduler.space(DEFAULT_ORG);
    let action = BlobDigest::of(b"an action");
    let planted = ActionResult {
        exit_code: 0,
        ..Default::default()
    };
    space
        .records
        .put_derived(
            "reapi:ac",
            &action.hash,
            bytes::Bytes::from(planted.encode_to_vec()),
        )
        .await
        .unwrap();
    assert!(fx
        .scheduler
        .cached_result(&space, &action)
        .await
        .unwrap()
        .is_none());

    assert!(fx
        .scheduler
        .cache_result(&space, &action, &planted)
        .await
        .unwrap());
    assert!(fx
        .scheduler
        .cached_result(&space, &action)
        .await
        .unwrap()
        .is_some());
}
