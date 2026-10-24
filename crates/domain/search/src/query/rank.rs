//! Ranking matched files (v1): weighted path signals, symbol definitions
//! and recent change.

use rill_types::RepoPath;

/// What ranking knows about one matched file.
pub struct Signals<'a> {
    pub path: &'a RepoPath,
    /// The file name itself matches the query.
    pub name_matches: bool,
    /// A matched line defines a symbol.
    pub defines: bool,
    /// The file changed in the last few landings.
    pub recent: bool,
}

/// Higher ranks first.
pub fn score(s: &Signals<'_>) -> i32 {
    let path = s.path.as_str();
    let depth = path.matches('/').count() as i32;
    let has = |seg: &str| path.starts_with(seg) || path.contains(&format!("/{seg}"));
    let mut score = 100 - 3 * depth;
    if s.name_matches {
        score += 20;
    }
    if s.defines {
        score += 25;
    }
    if s.recent {
        score += 10;
    }
    if has("src/") || has("lib/") {
        score += 15;
    }
    if has("vendor/") || has("third_party/") || has("node_modules/") || has("generated/") {
        score -= 30;
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    if has("test/")
        || has("tests/")
        || name.contains("_test.")
        || name.contains(".test.")
        || name.contains(".spec.")
    {
        score -= 15;
    }
    score
}

#[cfg(test)]
mod tests {
    use rill_types::RepoPathBuf;

    use super::*;

    fn at(p: &str, defines: bool) -> i32 {
        let path = RepoPathBuf::new(p).unwrap();
        score(&Signals {
            path: &path,
            name_matches: false,
            defines,
            recent: false,
        })
    }

    #[test]
    fn definitions_and_source_outrank_vendored_tests() {
        assert!(at("src/lib.rs", true) > at("src/lib.rs", false));
        assert!(at("src/parser.rs", false) > at("vendor/x/src/parser.rs", false));
        assert!(at("crates/a/src/lib.rs", false) > at("crates/a/tests/lib.rs", false));
        assert!(at("a.rs", false) > at("a/b/c/d/e.rs", false));
    }
}
