//! Stacks: ordered chains of dependent changes that land in sequence.

use rill_types::{ChangeId, ChangeVersionHash, ReviewState};

use crate::error::{Error, Result};
use crate::service::{ReviewEvent, ReviewService};

/// What the lander must do after a stack member lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Restack {
    /// The version the first dependent must be rebased onto.
    pub base: ChangeVersionHash,
    /// Changes above the landed one, bottom to top. Each one's base is the
    /// rebased version of the one before it; only the first has a known
    /// base up front.
    pub dependents: Vec<ChangeId>,
}

impl ReviewService {
    /// Declare `change_ids` (bottom to top) as a stack. Every change must be
    /// under review and each one's current version must have the previous
    /// change's current version as its first parent. The stack is stored
    /// on every member and risk is re-evaluated with the new depths.
    pub async fn set_stack(&self, change_ids: &[ChangeId]) -> Result<Vec<ReviewState>> {
        if change_ids.is_empty() {
            return Err(Error::InvalidStack(
                "a stack needs at least one change".into(),
            ));
        }
        let mut states = Vec::with_capacity(change_ids.len());
        for id in change_ids {
            states.push(self.get(id).await?);
        }
        self.validate_chain(&states).await?;

        let stack: Vec<ChangeId> = change_ids.to_vec();
        let mut out = Vec::with_capacity(stack.len());
        for id in change_ids {
            let state = self.write_stack(id, &stack).await?;
            out.push(state);
        }
        Ok(out)
    }

    /// The stack a change belongs to; a standalone change yields just
    /// itself.
    pub async fn stack_of(&self, change_id: &ChangeId) -> Result<Vec<ChangeId>> {
        let state = self.get(change_id).await?;
        Ok(if state.stack.is_empty() {
            vec![*change_id]
        } else {
            state.stack
        })
    }

    /// After `landed_change_id` lands as `landed_version`, the dependents
    /// whose base must move. Returns `None` when the change is standalone
    /// or at the top of its stack.
    pub async fn restack_after_land(
        &self,
        landed_change_id: &ChangeId,
        landed_version: ChangeVersionHash,
    ) -> Result<Option<Restack>> {
        let stack = self.stack_of(landed_change_id).await?;
        let Some(pos) = stack.iter().position(|c| c == landed_change_id) else {
            return Ok(None);
        };
        let dependents: Vec<ChangeId> = stack[pos + 1..].to_vec();
        if dependents.is_empty() {
            return Ok(None);
        }
        Ok(Some(Restack {
            base: landed_version,
            dependents,
        }))
    }

    async fn validate_chain(&self, states: &[ReviewState]) -> Result<()> {
        let mut seen = std::collections::HashSet::new();
        for state in states {
            if !seen.insert(state.change_id) {
                return Err(Error::InvalidStack(format!(
                    "change {} appears twice",
                    state.change_id
                )));
            }
        }
        for pair in states.windows(2) {
            let (below, above) = (&pair[0], &pair[1]);
            let below_version = below
                .current_version()
                .ok_or(Error::NoVersions(below.change_id))?;
            let above_version = above
                .current_version()
                .ok_or(Error::NoVersions(above.change_id))?;
            let cv = self.derive.change(&above_version).await?;
            if cv.first_parent() != Some(below_version) {
                return Err(Error::InvalidStack(format!(
                    "change {} (version {}) is not based on change {} (version {})",
                    above.change_id,
                    above_version.short(),
                    below.change_id,
                    below_version.short()
                )));
            }
        }
        Ok(())
    }

    async fn write_stack(&self, change_id: &ChangeId, stack: &[ChangeId]) -> Result<ReviewState> {
        let current = self.get(change_id).await?;
        let mut probe = current.clone();
        probe.stack = stack.to_vec();
        let facts = self.change_facts(&probe).await?;
        let (state, transition) = self
            .update(change_id, |s| {
                s.stack = stack.to_vec();
                self.apply_risk(s, &facts);
                self.refresh_status(s, &facts)
            })
            .await?;
        self.emit(ReviewEvent::StackChanged {
            change_id: *change_id,
            stack: stack.to_vec(),
        });
        self.emit_transition(change_id, transition);
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::fixture;

    #[tokio::test]
    async fn validates_first_parent_chain() {
        let fx = fixture().await;
        let (_, base) = fx.root_change("a.txt", "1\n").await;
        let (c1, v1) = fx.child_change(base, "b.txt", "2\n").await;
        let (c2, v2) = fx.child_change(v1, "c.txt", "3\n").await;
        let (c3, _) = fx.child_change(base, "d.txt", "4\n").await;
        for c in [&c1, &c2, &c3] {
            fx.service.submit(c, &fx.alice).await.unwrap();
        }
        assert!(matches!(
            fx.service.set_stack(&[c1, c3]).await,
            Err(Error::InvalidStack(_))
        ));
        assert!(matches!(
            fx.service.set_stack(&[c2, c1]).await,
            Err(Error::InvalidStack(_))
        ));
        let states = fx.service.set_stack(&[c1, c2]).await.unwrap();
        assert!(states.iter().all(|s| s.stack == vec![c1, c2]));
        assert_eq!(fx.service.stack_of(&c2).await.unwrap(), vec![c1, c2]);
        assert_eq!(fx.service.stack_of(&c3).await.unwrap(), vec![c3]);

        let restack = fx.service.restack_after_land(&c1, v1).await.unwrap();
        assert_eq!(
            restack,
            Some(Restack {
                base: v1,
                dependents: vec![c2]
            })
        );
        assert_eq!(fx.service.restack_after_land(&c2, v2).await.unwrap(), None);
    }
}
