//! The files changes edit and how an edit is drawn.

use rill_types::{ChangeVersionHash, Principal, RepoPathBuf};

use super::Ctx;

/// Files every change edits, plus one hot file most changes collide on.
pub const FILES: &[&str] = &["src/a.rs", "src/b.rs", "src/c.rs", "docs/notes.md"];
pub const HOT_FILE: &str = "Cargo.lock";
/// Lines in each file at the start.
pub const FILE_LINES: usize = 8;

impl Ctx<'_> {
    pub(super) async fn base(&mut self) -> Option<ChangeVersionHash> {
        let head = self.world.repo.trunk_head().await.ok()??;
        let (seq, hash) = head;
        if seq.get() > 1 && self.rng.chance(self.stale_base_share) {
            let older = 1 + self.rng.below(seq.get() as usize - 1) as u64;
            return self
                .world
                .repo
                .trunk_at(rill_types::TrunkSeq(older))
                .await
                .ok()
                .flatten();
        }
        Some(hash)
    }

    pub(super) fn path(&mut self) -> RepoPathBuf {
        let name = if self.rng.chance(self.hot_file_share) {
            HOT_FILE
        } else {
            FILES[self.rng.below(FILES.len())]
        };
        RepoPathBuf::new(name).expect("fixed paths are valid")
    }

    /// `path` at `base` with one line replaced by a line unique to this
    /// step, so concurrent edits sometimes overlap and sometimes do not.
    pub(super) async fn edited(
        &mut self,
        base: ChangeVersionHash,
        path: &RepoPathBuf,
        who: &Principal,
    ) -> Option<String> {
        let mut lines = self.world.file_lines(base, path).await.ok()?;
        let line = format!("{} step {}", who.name, self.step);
        if lines.is_empty() || self.rng.chance(0.1) {
            lines.push(line);
        } else {
            let at = self.rng.below(lines.len());
            lines[at] = line;
        }
        Some(lines.join("\n") + "\n")
    }
}
