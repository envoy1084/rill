//! Ordered groups (stacks) in the queue: served together, bottom first,
//! never split across windows or batches.

use std::collections::{HashMap, HashSet};

use rill_types::{GroupMode, LandGroup, LandTicket};

use super::footprint::Footprint;
use crate::queue::QueuedRequest;

/// The group `queued` lands with, if any.
pub fn group_of(queued: &QueuedRequest) -> Option<&LandGroup> {
    queued.request.group.as_ref()
}

/// Why a member that was ready did not land.
pub fn group_failure(mode: GroupMode) -> &'static str {
    match mode {
        GroupMode::All => "another change in its stack did not land, and the stack lands whole",
        GroupMode::Prefix => "a change below it in its stack did not land",
    }
}

/// `order` with every group's members moved up to where its first member
/// is served, bottom first; everything else keeps its place.
pub fn keep_groups_together(order: Vec<QueuedRequest>) -> Vec<QueuedRequest> {
    let mut members: HashMap<LandTicket, Vec<QueuedRequest>> = HashMap::new();
    let mut slots = Vec::with_capacity(order.len());
    for queued in order {
        match group_of(&queued).map(LandGroup::id) {
            Some(id) => {
                let group = members.entry(id).or_default();
                if group.is_empty() {
                    slots.push(Err(id));
                }
                group.push(queued);
            }
            None => slots.push(Ok(queued)),
        }
    }
    let mut out = Vec::new();
    for slot in slots {
        match slot {
            Ok(queued) => out.push(queued),
            Err(id) => {
                let mut group = members.remove(&id).unwrap_or_default();
                group.sort_by_key(|q| group_of(q).map_or(0, |g| g.index));
                out.extend(group);
            }
        }
    }
    out
}

/// How many requests from the front of `order` make a window of at most
/// `max` without splitting a group (groups are at most `max` long).
pub fn window_len(order: &[QueuedRequest], max: usize) -> usize {
    let mut take = order.len().min(max);
    if take == order.len() {
        return take;
    }
    let id = |i: usize| group_of(&order[i]).map(LandGroup::id);
    while take > 0 && id(take).is_some() && id(take) == id(take - 1) {
        take -= 1;
    }
    if take == 0 {
        order.len().min(max)
    } else {
        take
    }
}

/// Move every group with a member in `later` wholly into `later`, so a
/// batch never lands part of a group because the rest was held back.
pub fn hold_split_groups(
    now: Vec<(QueuedRequest, Footprint)>,
    mut later: Vec<QueuedRequest>,
) -> (Vec<(QueuedRequest, Footprint)>, Vec<QueuedRequest>) {
    let split: HashSet<LandTicket> = later
        .iter()
        .filter_map(|q| group_of(q).map(LandGroup::id))
        .collect();
    if split.is_empty() {
        return (now, later);
    }
    let mut kept = Vec::with_capacity(now.len());
    for (queued, footprint) in now {
        match group_of(&queued) {
            Some(g) if split.contains(&g.id()) => later.push(queued),
            _ => kept.push((queued, footprint)),
        }
    }
    (kept, later)
}

/// The conflict domain every member of a group shares, so each member is
/// speculated and verified on top of the one below it.
pub fn group_domain(group: &LandGroup) -> String {
    format!("stack:{}", group.id())
}

#[cfg(test)]
mod tests {
    use rill_types::{
        ChangeId, LandRequest, LandRequestRecord, LandStatus, ObjectHash, ObjectKind, Principal,
        RiskClass, Timestamp,
    };

    use super::*;

    fn queued(ticket: u64, group: Option<(u64, u32)>) -> QueuedRequest {
        let mut request = LandRequest::new(
            ChangeId::new(),
            ObjectHash::digest(ObjectKind::ChangeVersion, &ticket.to_be_bytes()),
            Principal::agent("a"),
            RiskClass::Low,
        );
        request.group = group.map(|(id, index)| LandGroup {
            mode: GroupMode::All,
            tickets: (0..3).map(|i| LandTicket(id + i)).collect(),
            index,
        });
        LandRequestRecord {
            ticket: LandTicket(ticket),
            request,
            status: LandStatus::Queued,
            updated_at: Timestamp::from_millis(0),
        }
        .into()
    }

    fn tickets(order: &[QueuedRequest]) -> Vec<u64> {
        order.iter().map(|q| q.ticket.0).collect()
    }

    #[test]
    fn a_group_is_served_together_bottom_first_where_its_first_member_was() {
        let order = vec![
            queued(1, None),
            queued(12, Some((10, 2))),
            queued(2, None),
            queued(10, Some((10, 0))),
            queued(11, Some((10, 1))),
            queued(3, None),
        ];
        assert_eq!(
            tickets(&keep_groups_together(order)),
            vec![1, 10, 11, 12, 2, 3]
        );
    }

    #[test]
    fn windows_end_before_a_group_that_would_not_fit() {
        let order = vec![
            queued(1, None),
            queued(10, Some((10, 0))),
            queued(11, Some((10, 1))),
            queued(12, Some((10, 2))),
        ];
        assert_eq!(window_len(&order, 3), 1);
        assert_eq!(window_len(&order, 4), 4);
        assert_eq!(
            window_len(&order[1..], 2),
            2,
            "a group longer than a window is cut"
        );
    }

    #[test]
    fn a_held_member_holds_its_whole_group() {
        let fp = Footprint::new(rill_core::ChangedPathSet::new([]), &Default::default());
        let now = vec![
            (queued(10, Some((10, 0))), fp.clone()),
            (queued(1, None), fp.clone()),
        ];
        let (now, later) = hold_split_groups(now, vec![queued(11, Some((10, 1)))]);
        assert_eq!(now.len(), 1);
        assert_eq!(tickets(&later), vec![11, 10]);
    }
}
