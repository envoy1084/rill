//! `rill ref`: list, show, set, delete.

use clap::{Args, Subcommand};
use rill_mcp::json;
use rill_types::{ChangeVersionHash, RefName};
use serde_json::json as j;

use crate::context::Ctx;
use crate::error::CliError;
use crate::output::{short, Table};
use crate::Result;

/// `rill ref <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum RefCmd {
    /// List refs.
    List(ListArgs),
    /// Show a ref's target.
    Show(NameArg),
    /// Move or create a ref (compare-and-swap with --expected).
    Set(SetArgs),
    /// Delete a ref.
    Delete(DeleteArgs),
}

/// `rill ref list`.
#[derive(Args, Debug)]
pub struct ListArgs {
    /// Only refs starting with this prefix.
    pub prefix: Option<String>,
}

/// A command taking a ref name.
#[derive(Args, Debug)]
pub struct NameArg {
    /// Ref name.
    pub name: String,
}

/// `rill ref set`.
#[derive(Args, Debug)]
pub struct SetArgs {
    /// Ref name.
    pub name: String,
    /// New target version hash.
    pub hash: String,
    /// Required current target (`none` for "must not exist"); unchecked when absent.
    #[arg(long)]
    pub expected: Option<String>,
}

/// `rill ref delete`.
#[derive(Args, Debug)]
pub struct DeleteArgs {
    /// Ref name.
    pub name: String,
    /// Required current target; unchecked when absent.
    #[arg(long)]
    pub expected: Option<String>,
}

fn parse_name(text: &str) -> Result<RefName> {
    RefName::new(text).map_err(|e| CliError::usage(format!("invalid ref name: {e}")))
}

fn parse_hash(text: &str) -> Result<ChangeVersionHash> {
    ChangeVersionHash::from_hex(text)
        .map_err(|e| CliError::usage(format!("invalid version hash {text:?}: {e}")))
}

/// Run a ref subcommand.
pub async fn run(ctx: &mut Ctx<'_>, cmd: RefCmd) -> Result<()> {
    match cmd {
        RefCmd::List(args) => list(ctx, args).await,
        RefCmd::Show(args) => show(ctx, args).await,
        RefCmd::Set(args) => set(ctx, args).await,
        RefCmd::Delete(args) => delete(ctx, args).await,
    }
}

async fn list(ctx: &mut Ctx<'_>, args: ListArgs) -> Result<()> {
    let refs = ctx.repo()?.list_refs(args.prefix.as_deref()).await?;
    ctx.emit(
        j!({ "refs": refs.iter().map(json::ref_info).collect::<Vec<_>>() }),
        |out| {
            if refs.is_empty() {
                return out.line(out.dim("no refs"));
            }
            let mut table = Table::new(&["NAME", "TARGET", "SEQ"]);
            for r in &refs {
                table.row(vec![
                    out.bold(r.name.as_str()),
                    out.id(&short(&r.target)),
                    r.trunk_seq
                        .map(|s| format!("trunk@{}", s.get()))
                        .unwrap_or_default(),
                ]);
            }
            out.table(table)
        },
    )
}

async fn show(ctx: &mut Ctx<'_>, args: NameArg) -> Result<()> {
    let name = parse_name(&args.name)?;
    let (hash, seq) = ctx.repo()?.resolve_ref(&name).await?;
    ctx.emit(
        j!({ "name": name.as_str(), "target": hash.to_hex(), "trunk_seq": seq.map(|s| s.get()) }),
        |out| {
            out.line(format!(
                "{} -> {}{}",
                out.bold(name.as_str()),
                hash.to_hex(),
                seq.map(|s| format!(" (trunk@{})", s.get()))
                    .unwrap_or_default()
            ))
        },
    )
}

/// `--expected`: `none` means the ref must not exist, absent means
/// unchecked. Unchecked is modelled by reading the current target first.
async fn expected_target(
    repo: &rill_sdk::RepoClient,
    name: &RefName,
    expected: Option<&str>,
) -> Result<Option<ChangeVersionHash>> {
    match expected {
        Some("none") => Ok(None),
        Some(h) => Ok(Some(parse_hash(h)?)),
        None => match repo.resolve_ref(name).await {
            Ok((h, _)) => Ok(Some(h)),
            Err(e) if e.is_not_found() => Ok(None),
            Err(e) => Err(e.into()),
        },
    }
}

async fn set(ctx: &mut Ctx<'_>, args: SetArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let name = parse_name(&args.name)?;
    let target = parse_hash(&args.hash)?;
    let expected = expected_target(&repo, &name, args.expected.as_deref()).await?;
    let result = repo.move_ref(name.clone(), expected, Some(target)).await?;
    ctx.emit(
        j!({ "name": name.as_str(), "target": result.map(|h| h.to_hex()) }),
        |out| {
            out.line(format!(
                "{} -> {}",
                out.bold(name.as_str()),
                result.map(|h| h.to_hex()).unwrap_or_default()
            ))
        },
    )
}

async fn delete(ctx: &mut Ctx<'_>, args: DeleteArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let name = parse_name(&args.name)?;
    if name.is_trunk() {
        return Err(CliError::usage("trunk cannot be deleted"));
    }
    let expected = expected_target(&repo, &name, args.expected.as_deref()).await?;
    repo.move_ref(name.clone(), expected, None).await?;
    ctx.emit(j!({ "deleted": name.as_str() }), |out| {
        out.line(format!("deleted {}", out.bold(name.as_str())))
    })
}
