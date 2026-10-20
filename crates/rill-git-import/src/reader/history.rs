//! Topological ordering of a commit graph.

use std::collections::{HashMap, HashSet};

use super::source::{GitCommit, GitSha, GitSource};
use crate::error::Result;

/// Every commit reachable from `head`, parents before children, with the
/// first-parent lineage explored before any other parent.
///
/// This is an iterative post-order DFS: a node is emitted once all of its
/// parents are, and among a merge's parents the first parent's whole
/// ancestry is emitted first. Recursion is avoided because histories can be
/// hundreds of thousands of commits deep.
pub fn history<S: GitSource + ?Sized>(source: &mut S, head: &GitSha) -> Result<Vec<GitCommit>> {
    let mut commits: HashMap<GitSha, GitCommit> = HashMap::new();
    let mut emitted: HashSet<GitSha> = HashSet::new();
    let mut order: Vec<GitCommit> = Vec::new();
    // (sha, parents already pushed)
    let mut stack: Vec<(GitSha, bool)> = vec![(head.clone(), false)];
    while let Some((sha, expanded)) = stack.pop() {
        if emitted.contains(&sha) {
            continue;
        }
        if expanded {
            emitted.insert(sha.clone());
            let commit = commits
                .remove(&sha)
                .expect("expanded commits were loaded on first visit");
            order.push(commit);
            continue;
        }
        if !commits.contains_key(&sha) {
            commits.insert(sha.clone(), source.commit(&sha)?);
        }
        let parents = commits[&sha].parents.clone();
        stack.push((sha, true));
        // LIFO: push the first parent last so it is processed first.
        for parent in parents.into_iter().rev() {
            if !emitted.contains(&parent) {
                stack.push((parent, false));
            }
        }
    }
    Ok(order)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::MemorySource;

    #[test]
    fn parents_first_mainline_first() {
        //   a - b - d - e (head)
        //     \ c /
        let mut src = MemorySource::new();
        src.add_commit("a", &[], &[]);
        src.add_commit("b", &["a"], &[]);
        src.add_commit("c", &["a"], &[]);
        src.add_commit("d", &["b", "c"], &[]);
        src.add_commit("e", &["d"], &[]);
        let order: Vec<String> = history(&mut src, &GitSha::new("e"))
            .unwrap()
            .into_iter()
            .map(|c| c.sha.as_str().to_string())
            .collect();
        assert_eq!(order, vec!["a", "b", "c", "d", "e"]);
    }
}
