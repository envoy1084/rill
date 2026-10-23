//! `rill resolve -i`: each conflict of each file shown side by side in
//! turn, with a choice of the side to keep; fully resolved files are then
//! marked resolved.

use bytes::Bytes;
use rill_core::merge::{parse_regions, Region};
use rill_daemon::proto as dpb;
use rill_types::{Merge, RepoPathBuf};
use serde_json::json as j;

use crate::context::Ctx;
use crate::error::CliError;
use crate::output::{Out, Role};
use crate::Result;

/// What to do with one conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
    /// Keep side `n` (0-based).
    Side(usize),
    /// Every side, one after the other.
    AllSides,
    /// The base the sides started from.
    Base,
    /// Leave the markers for later.
    Later,
}

/// The text a pick keeps; `None` leaves the conflict in place.
pub fn apply(pick: &Pick, terms: &Merge<Bytes>) -> Option<Vec<u8>> {
    match pick {
        Pick::Side(n) => terms.adds().nth(*n).map(|b| b.to_vec()),
        Pick::AllSides => Some(terms.adds().flat_map(|b| b.to_vec()).collect()),
        Pick::Base => terms.removes().next().map(|b| b.to_vec()),
        Pick::Later => None,
    }
}

/// The file rebuilt with each conflict replaced as `picks` say, and
/// whether every conflict was resolved.
pub fn rebuild(regions: &[Region], picks: &[Pick]) -> (Vec<u8>, bool) {
    let mut out = Vec::new();
    let mut resolved = true;
    let mut k = 0;
    for region in regions {
        match region {
            Region::Text(t) => out.extend_from_slice(t),
            Region::Conflict { raw, terms } => {
                match picks.get(k).and_then(|p| apply(p, terms)) {
                    Some(text) => out.extend_from_slice(&text),
                    None => {
                        out.extend_from_slice(raw);
                        resolved = false;
                    }
                }
                k += 1;
            }
        }
    }
    (out, resolved)
}

fn show_conflict(
    out: &mut Out<'_>,
    path: &RepoPathBuf,
    k: usize,
    total: usize,
    terms: &Merge<Bytes>,
) -> std::io::Result<()> {
    out.blank()?;
    let title = format!("{path}: conflict {} of {total}", k + 1);
    let painted = out.paint(Role::Header, &title);
    out.line(painted)?;
    if let Some(base) = terms.removes().next() {
        let label = out.paint(Role::Dim, "  base");
        out.line(label)?;
        for line in String::from_utf8_lossy(base).lines() {
            let painted = out.paint(Role::Dim, &format!("    │ {line}"));
            out.line(painted)?;
        }
    }
    for (i, side) in terms.adds().enumerate() {
        let role = if i == 0 { Role::DiffDel } else { Role::DiffAdd };
        let label = out.paint(role, &format!("  side {}", i + 1));
        out.line(label)?;
        for line in String::from_utf8_lossy(side).lines() {
            let painted = out.paint(role, &format!("    │ {line}"));
            out.line(painted)?;
        }
    }
    out.flush()
}

fn ask(sides: usize) -> Result<Pick> {
    let mut options: Vec<String> = (1..=sides).map(|n| format!("side {n}")).collect();
    options.push("all sides, in order".into());
    options.push("base".into());
    options.push("leave it for later".into());
    let choice = inquire::Select::new("keep:", options.clone())
        .prompt()
        .map_err(|e| CliError::usage(format!("no answer: {e}")))?;
    let at = options
        .iter()
        .position(|o| *o == choice)
        .unwrap_or(options.len() - 1);
    Ok(match at {
        n if n < sides => Pick::Side(n),
        n if n == sides => Pick::AllSides,
        n if n == sides + 1 => Pick::Base,
        _ => Pick::Later,
    })
}

/// Run `rill resolve -i` over `paths`.
pub async fn interactive(ctx: &mut Ctx<'_>, paths: Vec<RepoPathBuf>) -> Result<()> {
    if !ctx.interactive() || !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        return Err(CliError::usage("`rill resolve -i` needs a terminal"));
    }
    let dir = ctx.require_checkout_dir()?;
    let mut done = Vec::new();
    let mut left = Vec::new();
    for path in &paths {
        let mut file = dir.clone();
        file.extend(path.components());
        let bytes = std::fs::read(&file)?;
        let regions = parse_regions(&bytes)
            .ok_or_else(|| CliError::usage(format!("{path} has malformed conflict markers")))?;
        let conflicts: Vec<&Merge<Bytes>> = regions
            .iter()
            .filter_map(|r| match r {
                Region::Conflict { terms, .. } => Some(terms),
                Region::Text(_) => None,
            })
            .collect();
        let mut picks = Vec::with_capacity(conflicts.len());
        for (k, terms) in conflicts.iter().enumerate() {
            show_conflict(ctx.out(), path, k, conflicts.len(), terms)?;
            picks.push(ask(terms.num_sides())?);
        }
        let (text, resolved) = rebuild(&regions, &picks);
        std::fs::write(&file, text)?;
        if resolved {
            done.push(path.clone());
        } else {
            left.push(path.clone());
        }
    }
    if !done.is_empty() {
        let mount = ctx.mount().await?;
        ctx.daemon()
            .await?
            .resolve_paths(dpb::ResolvePathsRequest {
                name: mount.name,
                paths: done.iter().map(|p| p.as_str().to_string()).collect(),
                force: false,
            })
            .await?;
    }
    ctx.emit(
        j!({
            "resolved": done.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
            "remaining": left.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
        }),
        |out| {
            out.blank()?;
            for p in &done {
                let painted = out.paint(Role::Good, &format!("resolved {p}"));
                out.line(painted)?;
            }
            for p in &left {
                let painted = out.paint(Role::Warning, &format!("still conflicted {p}"));
                out.line(painted)?;
            }
            Ok(())
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_core::merge::materialize;

    #[test]
    fn picks_replace_their_conflicts_and_later_keeps_the_markers() {
        let terms = Merge::three_way(
            Some(Bytes::from_static(b"a\nb\nc\nd\ne\n")),
            Some(Bytes::from_static(b"a\nB1\nc\nd\nE1\n")),
            Some(Bytes::from_static(b"a\nB2\nc\nd\nE2\n")),
        );
        let text = materialize(&terms);
        let regions = parse_regions(&text).unwrap();
        let (all, resolved) = rebuild(&regions, &[Pick::Side(0), Pick::Side(1)]);
        assert_eq!(all, b"a\nB1\nc\nd\nE2\n");
        assert!(resolved);
        let (partial, resolved) = rebuild(&regions, &[Pick::Base, Pick::Later]);
        assert!(!resolved);
        assert!(partial.starts_with(b"a\nb\nc\nd\n"));
        assert_eq!(
            rill_core::parse_materialized(&partial).map(|m| m.is_resolved()),
            Some(false),
            "the left conflict still parses"
        );
        let (both, _) = rebuild(&regions, &[Pick::AllSides, Pick::AllSides]);
        assert_eq!(both, b"a\nB1\nB2\nc\nd\nE1\nE2\n");
    }
}
