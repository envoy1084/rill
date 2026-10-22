//! What a request touches, for deciding which requests interact.

use std::collections::BTreeSet;

use rill_core::ChangedPathSet;

use super::rules::LandingRules;

/// Repository configuration: a change to it lands alone, so the rules in
/// force for a batch never change within it.
const CONFIG_DIR: &str = ".rill/";

/// The conflict domains of one request.
#[derive(Debug, Clone)]
pub struct Footprint {
    pub paths: ChangedPathSet,
    /// Declared groups (`.rill/landing.toml`) its paths fall in.
    pub groups: BTreeSet<String>,
    /// It changes repository configuration and lands alone.
    pub exclusive: bool,
}

impl Footprint {
    /// The footprint of a version that changed `paths`, under `rules`.
    pub fn new(paths: ChangedPathSet, rules: &LandingRules) -> Self {
        let mut groups = BTreeSet::new();
        let mut exclusive = false;
        for path in paths.paths() {
            let path = path.as_str();
            exclusive |= path.starts_with(CONFIG_DIR) || path == ".rill";
            groups.extend(rules.groups_of(path).map(str::to_string));
        }
        Footprint {
            paths,
            groups,
            exclusive,
        }
    }

    /// Whether two requests interact: one is exclusive, they share a
    /// declared group, or their paths overlap (the same path, or one under
    /// the other). A shared parent directory alone does not count.
    pub fn overlaps(&self, other: &Footprint) -> bool {
        self.exclusive
            || other.exclusive
            || !self.groups.is_disjoint(&other.groups)
            || self.paths.overlaps(&other.paths)
    }

    /// The domains conflicts are counted in for throttling: top-level
    /// directories of its paths and its declared groups.
    pub fn domains(&self) -> BTreeSet<String> {
        let mut out: BTreeSet<String> = self
            .paths
            .paths()
            .iter()
            .map(|p| match p.as_str().split_once('/') {
                Some((top, _)) => format!("{top}/"),
                None => p.as_str().to_string(),
            })
            .collect();
        out.extend(self.groups.iter().map(|g| format!("group:{g}")));
        out
    }
}

#[cfg(test)]
mod tests {
    use rill_types::RepoPathBuf;

    use super::*;

    fn fp(paths: &[&str], rules: &LandingRules) -> Footprint {
        Footprint::new(
            ChangedPathSet::new(paths.iter().map(|p| RepoPathBuf::new(p).unwrap())),
            rules,
        )
    }

    #[test]
    fn only_shared_paths_groups_or_configuration_interact() {
        let none = LandingRules::default();
        assert!(
            !fp(&["src/a.rs"], &none).overlaps(&fp(&["src/b.rs"], &none)),
            "a shared directory"
        );
        assert!(fp(&["src/a.rs"], &none).overlaps(&fp(&["src/a.rs"], &none)));
        assert!(
            fp(&["src"], &none).overlaps(&fp(&["src/a.rs"], &none)),
            "one under the other"
        );
        let config = fp(&[".rill/landing.toml"], &none);
        assert!(config.exclusive && config.overlaps(&fp(&["docs/x.md"], &none)));

        let rules = LandingRules::parse(
            "[[serialize]]\nname = \"db\"\npaths = [\"schema/**\", \"migrations/**\"]\n",
        )
        .unwrap();
        let schema = fp(&["schema/users.sql"], &rules);
        assert!(schema.overlaps(&fp(&["migrations/001.sql"], &rules)));
        assert!(!schema.overlaps(&fp(&["src/a.rs"], &rules)));
        assert!(schema.domains().contains("group:db"));
    }

    #[test]
    fn bad_rules_are_refused() {
        assert!(LandingRules::parse("[[serialize]]\nname = \"x\"\npaths = [\"[\"]\n").is_err());
        assert!(LandingRules::parse("[throttle]\nconflict_rate = 2.0\n").is_err());
        assert!(LandingRules::parse("unknown = 1\n").is_err());
    }
}
