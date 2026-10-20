//! The risk classifier: a fixed, ordered list of features over
//! [`ChangeFacts`], each of which either sets a floor, bumps the class or
//! overrides it. Every feature that fires is named in the assessment so a
//! reviewer can see exactly why a change landed where it did.

use rill_types::{RepoPath, RiskClass};
use serde::{Deserialize, Serialize};

use super::config::CompiledRiskConfig;
use super::facts::{ChangeFacts, PathFacts};

/// Paths under this directory hold the repository's own policy and are
/// always critical: a change here can change who may land what.
const POLICY_DIR: &str = ".rill";

/// Prefix of every reason produced by a sensitive-glob match. Stable so that
/// consumers holding only `ReviewState::risk_reasons` can recover the fact.
pub const SENSITIVE_REASON_PREFIX: &str = "sensitive path ";

/// The result of classifying a change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskAssessment {
    /// The computed class.
    pub class: RiskClass,
    /// Human-readable names of every feature that fired, in evaluation
    /// order. Suitable for storing in `ReviewState::risk_reasons`.
    pub reasons: Vec<String>,
}

impl RiskAssessment {
    /// Whether a sensitive-path glob fired.
    pub fn touches_sensitive(&self) -> bool {
        touches_sensitive(&self.reasons)
    }
}

/// Whether a list of risk reasons includes a sensitive-path match.
pub fn touches_sensitive(reasons: &[String]) -> bool {
    reasons
        .iter()
        .any(|r| r.starts_with(SENSITIVE_REASON_PREFIX))
}

/// Classify a change. Deterministic: the same facts and config always give
/// the same assessment.
///
/// Evaluation order (later steps see the result of earlier ones):
///
/// 1. baseline: `Trivial` if every path is docs or cosmetic-only, `Low` if
///    every path is a test, otherwise `Normal`;
/// 2. floors: each sensitive glob raises the class to at least its own;
///    policy files under `.rill/` raise it to `Critical`;
/// 3. bumps: an agent-resolved conflict, too many files, too many lines and
///    unresolved conflicts each bump one step;
/// 4. an owner's `risk_override` on the intent replaces the result.
pub fn classify(facts: &ChangeFacts, config: &CompiledRiskConfig) -> RiskAssessment {
    let mut reasons = Vec::new();
    let mut class = baseline(facts, config, &mut reasons);

    for p in &facts.paths {
        for (glob, floor) in config.sensitive_matches(&p.path) {
            reasons.push(format!(
                "{SENSITIVE_REASON_PREFIX}{} matches {glob:?} (at least {floor})",
                p.path
            ));
            class = class.max(floor);
        }
        if is_policy_path(&p.path) {
            reasons.push(format!("policy file {} under {POLICY_DIR}/", p.path));
            class = RiskClass::Critical;
        }
    }

    if facts.resolved_by_agent() {
        reasons.push("conflict resolved by agent".into());
        class = class.bump();
    }
    if facts.has_conflicts() {
        reasons.push("unresolved conflicts".into());
        class = class.bump();
    }
    if facts.total_files > config.max_files() {
        reasons.push(format!(
            "{} files exceeds {}",
            facts.total_files,
            config.max_files()
        ));
        class = class.bump();
    }
    if facts.total_lines > config.max_lines() {
        reasons.push(format!(
            "{} lines exceeds {}",
            facts.total_lines,
            config.max_lines()
        ));
        class = class.bump();
    }

    if let Some(override_class) = facts.risk_override {
        reasons.push(format!("intent risk override {override_class}"));
        class = override_class;
    }

    RiskAssessment { class, reasons }
}

fn baseline(
    facts: &ChangeFacts,
    config: &CompiledRiskConfig,
    reasons: &mut Vec<String>,
) -> RiskClass {
    if facts.paths.is_empty() {
        reasons.push("no paths touched".into());
        return RiskClass::Trivial;
    }
    // A truncated path list cannot prove anything about the files it omits.
    let complete = facts.total_files as usize <= facts.paths.len();
    if complete && facts.paths.iter().all(|p| is_cosmetic(p, config)) {
        reasons.push("docs or comments only".into());
        return RiskClass::Trivial;
    }
    if complete && facts.paths.iter().all(|p| config.is_test(&p.path)) {
        reasons.push("tests only".into());
        return RiskClass::Low;
    }
    RiskClass::Normal
}

fn is_cosmetic(p: &PathFacts, config: &CompiledRiskConfig) -> bool {
    config.is_docs(&p.path) || p.cosmetic_only == Some(true)
}

fn is_policy_path(path: &RepoPath) -> bool {
    path.components().next() == Some(POLICY_DIR)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::risk::config::{RiskConfig, SensitiveGlob};
    use rill_types::{IntentKind, Principal, Provenance, RepoPathBuf};

    fn p(s: &str) -> RepoPathBuf {
        RepoPathBuf::new(s).unwrap()
    }

    fn cfg() -> CompiledRiskConfig {
        RiskConfig {
            sensitive: vec![
                SensitiveGlob {
                    glob: "src/billing/**".into(),
                    class: RiskClass::Sensitive,
                },
                SensitiveGlob {
                    glob: "src/auth/**".into(),
                    class: RiskClass::Critical,
                },
            ],
            max_files: 3,
            max_lines: 100,
            ..Default::default()
        }
        .compile()
        .unwrap()
    }

    fn facts(paths: Vec<PathFacts>) -> ChangeFacts {
        ChangeFacts::new(Principal::human("alice"), paths)
    }

    #[test]
    fn docs_only_is_trivial() {
        let a = classify(
            &facts(vec![
                PathFacts::modified(p("README.md")),
                PathFacts::added(p("docs/guide.rst")),
            ]),
            &cfg(),
        );
        assert_eq!(a.class, RiskClass::Trivial);
        assert_eq!(a.reasons, vec!["docs or comments only"]);
    }

    #[test]
    fn comments_only_is_trivial() {
        let a = classify(
            &facts(vec![
                PathFacts::modified(p("src/lib.rs")).with_cosmetic_only(true)
            ]),
            &cfg(),
        );
        assert_eq!(a.class, RiskClass::Trivial);
    }

    #[test]
    fn tests_only_is_low() {
        let a = classify(
            &facts(vec![PathFacts::modified(p("crates/x/tests/it.rs"))]),
            &cfg(),
        );
        assert_eq!(a.class, RiskClass::Low);
        assert_eq!(a.reasons, vec!["tests only"]);
    }

    #[test]
    fn code_is_normal_with_no_reasons() {
        let a = classify(&facts(vec![PathFacts::modified(p("src/lib.rs"))]), &cfg());
        assert_eq!(a.class, RiskClass::Normal);
        assert!(a.reasons.is_empty());
    }

    #[test]
    fn sensitive_glob_sets_floor() {
        let a = classify(
            &facts(vec![PathFacts::modified(p("src/billing/invoice.rs"))]),
            &cfg(),
        );
        assert_eq!(a.class, RiskClass::Sensitive);
        assert!(a.reasons[0].contains("src/billing/**"));

        let a = classify(
            &facts(vec![PathFacts::modified(p("src/auth/login.rs"))]),
            &cfg(),
        );
        assert_eq!(a.class, RiskClass::Critical);
    }

    #[test]
    fn docs_inside_sensitive_dir_still_raise_floor() {
        let a = classify(
            &facts(vec![PathFacts::modified(p("src/billing/README.md"))]),
            &cfg(),
        );
        assert_eq!(a.class, RiskClass::Sensitive);
        assert_eq!(a.reasons.len(), 2);
    }

    #[test]
    fn agent_resolution_bumps() {
        let f = facts(vec![PathFacts::modified(p("src/lib.rs"))]).with_provenance(Provenance {
            resolved_by: Some("agent".into()),
            ..Default::default()
        });
        let a = classify(&f, &cfg());
        assert_eq!(a.class, RiskClass::Sensitive);
        assert_eq!(a.reasons, vec!["conflict resolved by agent"]);
    }

    #[test]
    fn size_thresholds_bump() {
        let many: Vec<_> = (0..4)
            .map(|i| PathFacts::modified(p(&format!("src/f{i}.rs"))))
            .collect();
        let a = classify(&facts(many), &cfg());
        assert_eq!(a.class, RiskClass::Sensitive);
        assert!(a.reasons[0].contains("4 files exceeds 3"));

        let big = PathFacts::modified(p("src/f.rs")).with_lines(90, 20);
        let a = classify(&facts(vec![big]), &cfg());
        assert_eq!(a.class, RiskClass::Sensitive);
        assert!(a.reasons[0].contains("110 lines exceeds 100"));
    }

    #[test]
    fn bumps_stack_and_saturate() {
        let many: Vec<_> = (0..4)
            .map(|i| PathFacts::modified(p(&format!("src/f{i}.rs"))).with_lines(50, 0))
            .collect();
        let f = facts(many).with_provenance(Provenance {
            resolved_by: Some("agent".into()),
            ..Default::default()
        });
        let a = classify(&f, &cfg());
        assert_eq!(a.class, RiskClass::Critical);
        assert_eq!(a.reasons.len(), 3);
    }

    #[test]
    fn policy_files_are_critical() {
        let a = classify(
            &facts(vec![PathFacts::modified(p(".rill/submit.cedar"))]),
            &cfg(),
        );
        assert_eq!(a.class, RiskClass::Critical);
        // Even a markdown file under .rill is policy.
        let a = classify(&facts(vec![PathFacts::modified(p(".rill/x.md"))]), &cfg());
        assert_eq!(a.class, RiskClass::Critical);
    }

    #[test]
    fn override_wins() {
        let f = facts(vec![PathFacts::modified(p("src/auth/login.rs"))])
            .with_intent(IntentKind::Chore, Some(RiskClass::Low));
        let a = classify(&f, &cfg());
        assert_eq!(a.class, RiskClass::Low);
        assert!(a.reasons.last().unwrap().contains("override"));
    }

    #[test]
    fn truncated_path_list_is_not_trivial() {
        let mut f = facts(vec![PathFacts::modified(p("README.md"))]);
        f.total_files = 10;
        let a = classify(&f, &cfg());
        assert_eq!(a.class, RiskClass::Sensitive);
    }

    #[test]
    fn empty_change_is_trivial() {
        let a = classify(&facts(vec![]), &cfg());
        assert_eq!(a.class, RiskClass::Trivial);
    }
}
