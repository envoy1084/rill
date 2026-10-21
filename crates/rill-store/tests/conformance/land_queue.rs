//! Stored land requests on every backend: one unfinished request per
//! change, served by lane and then age, and finished for good.

use rill_store::Stores;
use rill_types::{
    ChangeId, ChangeVersionHash, LandRequest, LandStatus, Lane, Principal, RiskClass, Timestamp,
    TrunkSeq,
};

fn request(lane: Lane, at: i64) -> LandRequest {
    LandRequest {
        requested_at: Timestamp::from_millis(at),
        ..LandRequest::new(
            ChangeId::new(),
            ChangeVersionHash::ZERO,
            Principal::agent("bot"),
            RiskClass::Normal,
        )
    }
    .in_lane(lane)
}

pub async fn scenario(name: &str, stores: &Stores) {
    let lander = Principal::service("lander");
    let config = stores.create_repo("landq", "").await.unwrap();
    let repo = stores.repo(config.id);

    let old_agent = request(Lane::Agent, 1);
    let (agent, new) = repo.queue_land(&old_agent).await.unwrap();
    assert!(new, "{name}");
    let (again, new) = repo
        .queue_land(&LandRequest {
            requested_at: Timestamp::from_millis(9),
            ..old_agent.clone()
        })
        .await
        .unwrap();
    assert_eq!(
        (again, new),
        (agent, false),
        "{name}: one request per change"
    );
    let (human, _) = repo.queue_land(&request(Lane::Human, 5)).await.unwrap();
    let (revert, _) = repo.queue_land(&request(Lane::Emergency, 7)).await.unwrap();
    let (late_agent, _) = repo.queue_land(&request(Lane::Agent, 3)).await.unwrap();

    let order: Vec<_> = repo
        .pending_land_requests(10)
        .await
        .unwrap()
        .iter()
        .map(|r| r.ticket)
        .collect();
    assert_eq!(
        order,
        [revert, human, agent, late_agent],
        "{name}: lane, then age"
    );

    let lease = repo.try_acquire("lander", "a").await.unwrap().unwrap();
    repo.set_land_status(agent, LandStatus::Verifying, Some(&lease), &lander)
        .await
        .unwrap();
    repo.set_land_status(
        agent,
        LandStatus::Landed { seq: TrunkSeq(1) },
        Some(&lease),
        &lander,
    )
    .await
    .unwrap();
    assert!(
        repo.set_land_status(agent, LandStatus::Queued, Some(&lease), &lander)
            .await
            .unwrap_err()
            .is_conflict(),
        "{name}: finished for good"
    );
    assert_eq!(
        repo.pending_land_requests(10).await.unwrap().len(),
        3,
        "{name}"
    );
    assert_eq!(
        repo.active_land_request(&old_agent.change_id)
            .await
            .unwrap(),
        None,
        "{name}: the change may be requested again"
    );
    assert_eq!(
        repo.land_request(agent).await.unwrap().unwrap().status,
        LandStatus::Landed { seq: TrunkSeq(1) },
        "{name}: the outcome stays queryable"
    );

    repo.release(&lease).await.unwrap();
    repo.try_acquire("lander", "b").await.unwrap().unwrap();
    assert!(
        repo.set_land_status(human, LandStatus::Landing, Some(&lease), &lander)
            .await
            .unwrap_err()
            .is_conflict(),
        "{name}: a replaced lander records nothing"
    );

    let far = Timestamp::from_millis(Timestamp::now().millis() + 60_000);
    assert_eq!(
        repo.sweep_land_requests(far, 10).await.unwrap(),
        1,
        "{name}"
    );
    assert!(repo.land_request(agent).await.unwrap().is_none(), "{name}");
}
