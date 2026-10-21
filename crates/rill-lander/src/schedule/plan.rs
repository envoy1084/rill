//! The speculation plan for one batch: which earlier requests each one
//! must be rebased on top of, and which can be rebased in parallel.

use super::footprint::Footprint;

/// Where one request is speculated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Slot {
    /// On the trunk head, in parallel with every other independent request.
    Head,
    /// On top of request `.0`, whose chain holds every earlier request it
    /// overlaps.
    After(usize),
    /// It overlaps requests on different chains: speculated on the batch
    /// once the requests before it are lined up.
    Linear,
}

/// The plan for a batch of footprints, in commit order.
#[derive(Debug, Clone)]
pub struct Plan {
    /// For each request, the earlier ones it overlaps.
    overlaps: Vec<Vec<usize>>,
}

impl Plan {
    pub fn new(footprints: &[Footprint]) -> Self {
        let overlaps = footprints
            .iter()
            .enumerate()
            .map(|(i, f)| (0..i).filter(|&j| footprints[j].overlaps(f)).collect())
            .collect();
        Plan { overlaps }
    }

    pub fn len(&self) -> usize {
        self.overlaps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.overlaps.is_empty()
    }

    /// The earlier requests `i` overlaps.
    pub fn overlapped(&self, i: usize) -> &[usize] {
        &self.overlaps[i]
    }

    /// Where request `i` goes, given which earlier requests survived and
    /// where each was placed (`placed[j]` for `j < i`).
    pub fn slot(&self, i: usize, alive: &[bool], placed: &[Slot]) -> Slot {
        let live: Vec<usize> = self.overlaps[i]
            .iter()
            .copied()
            .filter(|&j| alive[j])
            .collect();
        let Some(&last) = live.last() else {
            return Slot::Head;
        };
        if live.iter().any(|&j| placed[j] == Slot::Linear) {
            return Slot::Linear;
        }
        let mut chain = vec![last];
        let mut at = last;
        while let Slot::After(p) = placed[at] {
            chain.push(p);
            at = p;
        }
        if live.iter().all(|j| chain.contains(j)) {
            Slot::After(last)
        } else {
            Slot::Linear
        }
    }

    /// Requests grouped into waves: each depends only on earlier waves, so
    /// a wave's members are speculated in parallel.
    pub fn waves(&self) -> Vec<Vec<usize>> {
        let mut level = vec![0usize; self.overlaps.len()];
        for i in 0..self.overlaps.len() {
            level[i] = self.overlaps[i]
                .iter()
                .map(|&j| level[j] + 1)
                .max()
                .unwrap_or(0);
        }
        let depth = level.iter().copied().max().map_or(0, |d| d + 1);
        let mut waves = vec![Vec::new(); depth];
        for (i, l) in level.into_iter().enumerate() {
            waves[l].push(i);
        }
        waves
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use rill_core::ChangedPathSet;
    use rill_types::RepoPathBuf;

    use super::*;
    use crate::schedule::LandingRules;

    fn fp(paths: &[&str]) -> Footprint {
        Footprint::new(
            ChangedPathSet::new(paths.iter().map(|p| RepoPathBuf::new(p).unwrap())),
            &LandingRules::default(),
        )
    }

    #[test]
    fn independent_requests_share_the_first_wave() {
        let plan = Plan::new(&[fp(&["a"]), fp(&["b"]), fp(&["a", "c"]), fp(&["d"])]);
        assert_eq!(plan.waves(), vec![vec![0, 1, 3], vec![2]]);
        let alive = [true; 4];
        let mut placed = vec![Slot::Head; 4];
        placed[2] = plan.slot(2, &alive, &placed);
        assert_eq!(placed[2], Slot::After(0));
        assert_eq!(
            plan.slot(2, &[false, true, true, true], &placed),
            Slot::Head,
            "its only overlap failed"
        );
    }

    #[test]
    fn overlapping_two_chains_waits_for_the_line() {
        let plan = Plan::new(&[fp(&["x"]), fp(&["y"]), fp(&["x", "y"])]);
        let placed = vec![Slot::Head, Slot::Head, Slot::Head];
        assert_eq!(plan.slot(2, &[true; 3], &placed), Slot::Linear);
        let chained = vec![Slot::Head, Slot::Head, Slot::Head];
        let plan = Plan::new(&[fp(&["x"]), fp(&["x", "y"]), fp(&["x", "y"])]);
        let mut placed = chained;
        placed[1] = plan.slot(1, &[true; 3], &placed);
        assert_eq!(
            plan.slot(2, &[true; 3], &placed),
            Slot::After(1),
            "one chain holds both"
        );
    }

    proptest! {
        #[test]
        fn every_overlap_is_speculated_first(
            sets in prop::collection::vec(prop::collection::btree_set(0u8..6, 1..3), 1..20),
            dead in prop::collection::vec(any::<bool>(), 20),
        ) {
            let fps: Vec<Footprint> = sets
                .iter()
                .map(|s| {
                    let names: Vec<String> = s.iter().map(|n| format!("p{n}")).collect();
                    fp(&names.iter().map(String::as_str).collect::<Vec<_>>())
                })
                .collect();
            let plan = Plan::new(&fps);
            let mut wave_of = vec![0; fps.len()];
            for (w, members) in plan.waves().iter().enumerate() {
                for &i in members {
                    wave_of[i] = w;
                }
            }
            let alive: Vec<bool> = (0..fps.len()).map(|i| !dead[i]).collect();
            let mut placed = vec![Slot::Head; fps.len()];
            for members in plan.waves() {
                for i in members {
                    placed[i] = plan.slot(i, &alive, &placed);
                }
            }
            for i in 0..fps.len() {
                for &j in plan.overlapped(i) {
                    prop_assert!(wave_of[j] < wave_of[i]);
                }
                if let Slot::After(last) = placed[i] {
                    let mut chain = vec![last];
                    let mut at = last;
                    while let Slot::After(p) = placed[at] { chain.push(p); at = p; }
                    for &j in plan.overlapped(i) {
                        prop_assert!(!alive[j] || chain.contains(&j), "{i} misses {j}");
                    }
                }
                if placed[i] == Slot::Head {
                    prop_assert!(plan.overlapped(i).iter().all(|&j| !alive[j]));
                }
            }
        }
    }
}
