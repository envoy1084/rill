//! Keys of one line of landings: trunk's own tables, or a release's rows
//! in the release tables, so both share one code path.

use bytes::Bytes;
use rill_types::{Line, ObjectHash};

use super::repo_keys::RepoKeys;
use super::table;
use crate::meta::seq_key;

/// The keys of `line` in one repository.
#[derive(Debug, Clone, Copy)]
pub struct LineKeys<'a> {
    keys: &'a RepoKeys,
    line: &'a Line,
}

impl RepoKeys {
    /// The keys of `line`.
    pub fn line<'a>(&'a self, line: &'a Line) -> LineKeys<'a> {
        LineKeys { keys: self, line }
    }
}

impl LineKeys<'_> {
    /// The head row: be64 seq ++ version hash.
    pub fn head(&self) -> Bytes {
        match self.line {
            Line::Trunk => self.keys.trunk_head(),
            Line::Release(name) => self.keys.row(table::RELEASE_HEAD, &name.as_str()),
        }
    }

    /// The prefix of the positions, for scans.
    pub fn positions(&self) -> Bytes {
        match self.line {
            Line::Trunk => self.keys.trunk_table(),
            Line::Release(name) => self.keys.row(table::RELEASE, &name.as_str()),
        }
    }

    pub fn position(&self, seq: u64) -> Bytes {
        seq_key(&self.positions(), seq)
    }

    pub fn by_hash(&self, hash: &ObjectHash) -> Bytes {
        match self.line {
            Line::Trunk => self.keys.trunk_by_hash(hash),
            Line::Release(name) => self.keys.row(
                table::RELEASE_BY_HASH,
                &(name.as_str(), hash.as_bytes().as_slice()),
            ),
        }
    }

    /// The prefix of the landing records, for scans.
    pub fn landings(&self) -> Bytes {
        match self.line {
            Line::Trunk => self.keys.landings(),
            Line::Release(name) => self.keys.row(table::RELEASE_LANDING, &name.as_str()),
        }
    }

    pub fn landing(&self, seq: u64) -> Bytes {
        seq_key(&self.landings(), seq)
    }

    /// The prefix of the unfinished requests, in the order they are served.
    pub fn pending_all(&self) -> Bytes {
        match self.line {
            Line::Trunk => self.keys.land_pending_all(),
            Line::Release(name) => self.keys.row(table::RELEASE_PENDING, &name.as_str()),
        }
    }

    pub fn pending(&self, lane: u8, requested_at: i64, ticket: u64) -> Bytes {
        let mut key = self.pending_all().to_vec();
        foundationdb_tuple::pack_into(&(i64::from(lane), requested_at, ticket as i64), &mut key);
        Bytes::from(key)
    }

    /// The counter new requests move: trunk's, or one for every release.
    pub fn wake(&self) -> Bytes {
        match self.line {
            Line::Trunk => self.keys.table(table::LAND_WAKE),
            Line::Release(_) => self.keys.table(table::RELEASE_WAKE),
        }
    }
}

#[cfg(test)]
mod tests {
    use rill_types::RepoId;

    use super::*;

    #[test]
    fn trunk_keeps_its_keys_and_releases_do_not_share_rows() {
        let keys = RepoKeys::new("ns", RepoId::new());
        let trunk = Line::Trunk;
        assert_eq!(keys.line(&trunk).head(), keys.trunk_head());
        assert_eq!(keys.line(&trunk).position(3), keys.trunk(3));
        assert_eq!(
            keys.line(&trunk).pending(1, 2, 3),
            keys.land_pending(1, 2, 3)
        );
        let (a, ab) = (Line::Release("a".into()), Line::Release("ab".into()));
        assert!(!keys
            .line(&ab)
            .position(1)
            .starts_with(&keys.line(&a).positions()));
        assert!(!keys.line(&a).head().starts_with(&keys.trunk_head()));
    }
}
