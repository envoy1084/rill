//! Ordered groups at the start of a window: a group lands only when every
//! member still queued is in the window, and not above a member that
//! finished without landing.

use std::collections::{BTreeSet, HashMap, HashSet};

use rill_types::{LandGroup, LandStatus, LandTicket};

use super::lander::Lander;
use crate::queue::QueuedRequest;
use crate::schedule::group_of;

impl Lander {
    /// Split `window` into what may be served now and what waits: a group
    /// with a lower member still queued outside the window waits for it.
    /// Members above a lower member that finished without landing fail.
    pub(super) async fn gate_groups(
        &self,
        window: Vec<QueuedRequest>,
    ) -> (Vec<QueuedRequest>, Vec<QueuedRequest>) {
        let mut present: HashMap<LandTicket, (LandGroup, BTreeSet<u32>)> = HashMap::new();
        for queued in &window {
            if let Some(group) = group_of(queued) {
                present
                    .entry(group.id())
                    .or_insert_with(|| (group.clone(), BTreeSet::new()))
                    .1
                    .insert(group.index);
            }
        }
        let mut hold = HashSet::new();
        let mut floor: HashMap<LandTicket, u32> = HashMap::new();
        for (id, (group, indices)) in &present {
            let top = indices.last().copied().unwrap_or(0);
            for (index, ticket) in group.tickets.iter().enumerate().take(top as usize) {
                if indices.contains(&(index as u32)) {
                    continue;
                }
                match self.queue().status(*ticket).await {
                    Ok(Some(LandStatus::Landed { .. })) => {}
                    Ok(Some(status)) if status.is_terminal() => {
                        floor.insert(*id, index as u32);
                        break;
                    }
                    Ok(None) => {
                        floor.insert(*id, index as u32);
                        break;
                    }
                    Ok(Some(_)) | Err(_) => {
                        hold.insert(*id);
                        break;
                    }
                }
            }
        }
        let mut now = Vec::with_capacity(window.len());
        let mut later = Vec::new();
        for queued in window {
            let Some(group) = group_of(&queued) else {
                now.push(queued);
                continue;
            };
            let id = group.id();
            if hold.contains(&id) {
                later.push(queued);
            } else if floor.get(&id).is_some_and(|f| group.index > *f) {
                let reason = crate::schedule::group_failure(rill_types::GroupMode::Prefix);
                self.fail(&queued, reason.into()).await;
            } else {
                now.push(queued);
            }
        }
        (now, later)
    }
}
