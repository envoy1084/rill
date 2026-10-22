//! Which steps a change needs: those gating its risk class whose inputs
//! it touched.

use rill_types::{RepoPathBuf, RiskClass};

use super::plan::{Step, PLAN_PATH};

/// Whether a step runs for a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    Run,
    /// The change touched none of the step's inputs, so the step's action
    /// is the one the base already ran.
    Unaffected,
    /// The step does not gate the change's risk class.
    NotGating,
}

/// Select `step` for a change at `risk` that changed `changed` paths;
/// `None` for either means unknown, which runs the step.
pub fn select(step: &Step, changed: Option<&[RepoPathBuf]>, risk: Option<RiskClass>) -> Selection {
    if let (Some(classes), Some(risk)) = (&step.risk, risk) {
        if !classes.contains(&risk) {
            return Selection::NotGating;
        }
    }
    match changed {
        None => Selection::Run,
        Some(paths) => {
            let affected = paths
                .iter()
                .any(|p| p.as_str() == PLAN_PATH || step.inputs.matches(p));
            if affected {
                Selection::Run
            } else {
                Selection::Unaffected
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::plan::Plan;

    fn step(extra: &str) -> Step {
        Plan::parse(&format!("[[step]]\ncommand = [\"x\"]\n{extra}"))
            .unwrap()
            .steps
            .remove(0)
    }

    fn paths(list: &[&str]) -> Vec<RepoPathBuf> {
        list.iter().map(|p| RepoPathBuf::new(p).unwrap()).collect()
    }

    #[test]
    fn a_change_outside_the_inputs_skips_the_step() {
        let rust = step("inputs = [\"**/*.rs\", \"Cargo.lock\"]");
        assert_eq!(
            select(&rust, Some(&paths(&["docs/a.md"])), None),
            Selection::Unaffected
        );
        assert_eq!(
            select(&rust, Some(&paths(&["docs/a.md", "src/x/y.rs"])), None),
            Selection::Run
        );
        assert_eq!(select(&rust, None, None), Selection::Run);
    }

    #[test]
    fn editing_the_plan_runs_every_step() {
        let rust = step("inputs = [\"**/*.rs\"]");
        assert_eq!(
            select(&rust, Some(&paths(&[PLAN_PATH])), None),
            Selection::Run
        );
    }

    #[test]
    fn a_step_gates_only_its_risk_classes() {
        let slow = step("risk = [\"sensitive\", \"critical\"]");
        assert_eq!(
            select(&slow, None, Some(RiskClass::Low)),
            Selection::NotGating
        );
        assert_eq!(
            select(&slow, None, Some(RiskClass::Critical)),
            Selection::Run
        );
        assert_eq!(select(&slow, None, None), Selection::Run);
    }
}
