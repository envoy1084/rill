//! The order requests are served in: lanes by urgency, and within a lane
//! weighted fair queuing across sponsors, so one swarm cannot starve
//! everyone else in its lane.

use std::collections::{BTreeMap, HashMap};

use crate::queue::QueuedRequest;

/// Who a request's share of its lane is charged to: its sponsor, or the
/// requester itself.
pub fn share_key(request: &QueuedRequest) -> String {
    request
        .request
        .sponsor
        .clone()
        .unwrap_or_else(|| request.request.requester.to_string())
}

/// `pending` (in lane, then arrival order) in the order to serve it. The
/// n-th request of a share key with weight w gets the virtual finish time
/// n / w; within a lane requests go by finish time, then queue order.
pub fn serve_order(
    pending: Vec<QueuedRequest>,
    weights: &BTreeMap<String, u32>,
) -> Vec<QueuedRequest> {
    let mut served: HashMap<(rill_types::Lane, String), u64> = HashMap::new();
    let mut tagged: Vec<(rill_types::Lane, f64, usize, QueuedRequest)> = pending
        .into_iter()
        .enumerate()
        .map(|(at, q)| {
            let key = share_key(&q);
            let weight = weights.get(&key).copied().unwrap_or(1).max(1) as f64;
            let n = served.entry((q.request.lane, key)).or_insert(0);
            *n += 1;
            (q.request.lane, *n as f64 / weight, at, q)
        })
        .collect();
    tagged.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
    tagged.into_iter().map(|(_, _, _, q)| q).collect()
}

#[cfg(test)]
mod tests {
    use rill_types::{
        ChangeId, LandRequest, LandRequestRecord, LandStatus, LandTicket, Lane, Principal,
        RiskClass, Timestamp,
    };

    use super::*;

    fn queued(ticket: u64, lane: Lane, who: &str, sponsor: Option<&str>) -> QueuedRequest {
        let request = LandRequest::new(
            ChangeId::new(),
            rill_types::ObjectHash::digest(
                rill_types::ObjectKind::ChangeVersion,
                &ticket.to_be_bytes(),
            ),
            Principal::agent(who),
            RiskClass::Trivial,
        )
        .in_lane(lane)
        .sponsored_by(sponsor.map(str::to_string));
        QueuedRequest::from(LandRequestRecord {
            ticket: LandTicket(ticket),
            request,
            status: LandStatus::Queued,
            updated_at: Timestamp::from_millis(0),
        })
    }

    #[test]
    fn a_swarm_cannot_starve_another_sponsor_or_a_human() {
        let mut pending: Vec<QueuedRequest> = (0..50)
            .map(|i| queued(i, Lane::Agent, &format!("bot{i}"), Some("user:swarm")))
            .collect();
        pending.push(queued(50, Lane::Agent, "helper", Some("user:ann")));
        pending.push(queued(51, Lane::Agent, "helper2", Some("user:ann")));
        pending.push(queued(52, Lane::Human, "bob", None));
        pending.sort_by_key(|q| (q.request.lane, q.ticket.0));
        let order = serve_order(pending.clone(), &Default::default());
        assert_eq!(order[0].ticket.0, 52, "humans first");
        let ann: Vec<usize> = order
            .iter()
            .enumerate()
            .filter(|(_, q)| q.request.sponsor.as_deref() == Some("user:ann"))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(ann, vec![2, 4], "alternates with the swarm");

        let weights = [("user:swarm".to_string(), 3)].into_iter().collect();
        let order = serve_order(pending, &weights);
        let first_ann = order
            .iter()
            .position(|q| q.request.sponsor.as_deref() == Some("user:ann"))
            .unwrap();
        assert_eq!(
            first_ann, 4,
            "after the human, a weight of 3 serves three of the swarm per turn"
        );
    }
}
