//! Reviewing and landing a change through the review service.

use std::time::Duration;

use bytes::Bytes;
use rill_proto::rill::v1 as pb;

use super::clients::Clients;

pub(crate) async fn land(clients: &mut Clients, repo: &str, change_id: Bytes) -> u64 {
    land_with(clients, repo, change_id, &[]).await
}

/// [`land`] with further votes, such as an `Owner` approval.
pub(crate) async fn land_with(
    clients: &mut Clients,
    repo: &str,
    change_id: Bytes,
    extra_votes: &[(&str, i32)],
) -> u64 {
    match try_land_with(clients, repo, change_id, extra_votes).await {
        Ok(seq) => seq,
        Err(e) => panic!("landing failed: {e}"),
    }
}

/// Submit, approve and land `change_id`; the trunk position, or why it
/// did not land.
pub(crate) async fn try_land_with(
    clients: &mut Clients,
    repo: &str,
    change_id: Bytes,
    extra_votes: &[(&str, i32)],
) -> Result<u64, String> {
    clients
        .review
        .submit_for_review(pb::SubmitForReviewRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            change_id: change_id.clone(),
        })
        .await
        .unwrap();
    for &(label, value) in [("Verified", 1), ("Review", 2)].iter().chain(extra_votes) {
        clients
            .review
            .vote(pb::VoteRequest {
                operation_id: Bytes::new(),
                repo: repo.into(),
                change_id: change_id.clone(),
                label: label.into(),
                value,
                reason: None,
                version: None,
            })
            .await
            .unwrap();
    }
    request_land(clients, repo, change_id).await
}

/// Ask to land `change_id`, already reviewed, and wait for the outcome.
pub(crate) async fn request_land(
    clients: &mut Clients,
    repo: &str,
    change_id: Bytes,
) -> Result<u64, String> {
    let ticket = clients
        .review
        .request_land(pb::RequestLandRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            change_id: change_id.clone(),
            onto: None,
        })
        .await
        .unwrap()
        .into_owned()
        .ticket
        .unwrap();
    for _ in 0..200 {
        let status = clients
            .review
            .get_land_status(pb::GetLandStatusRequest {
                repo: repo.into(),
                ticket: ticket.id.clone(),
            })
            .await
            .unwrap()
            .into_owned();
        match rill_proto::convert::known::<pb::LandStatus>(
            status.ticket.as_option().unwrap().status,
        )
        .unwrap()
        {
            pb::LandStatus::Landed => return Ok(status.seq.unwrap()),
            pb::LandStatus::Failed | pb::LandStatus::Cancelled => {
                return Err(status.error.unwrap_or_default())
            }
            _ => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }
    Err("landing did not complete".into())
}
