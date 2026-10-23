//! `rill org`: create and list organizations.

use clap::{Args, Subcommand};
use serde_json::json;

use crate::context::Ctx;
use crate::output::{when, Table};
use crate::Result;

/// `rill org <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum OrgCmd {
    /// List orgs on the server.
    List,
    /// Create an org; repositories in it are named `org/repo`.
    Create(CreateArgs),
    /// Show one org.
    Show(ShowArgs),
}

/// `rill org show`.
#[derive(Args, Debug)]
pub struct ShowArgs {
    /// The org.
    pub name: String,
}

/// `rill org create`.
#[derive(Args, Debug)]
pub struct CreateArgs {
    /// 1-63 lowercase letters, digits and dashes.
    pub name: String,
    /// Free-form description.
    #[arg(long)]
    pub description: Option<String>,
}

/// Run an org subcommand.
pub async fn run(ctx: &mut Ctx<'_>, cmd: OrgCmd) -> Result<()> {
    let admin = ctx.client()?.admin();
    match cmd {
        OrgCmd::List => {
            let orgs = admin.list_orgs().await?;
            let value = json!(orgs
                .iter()
                .map(|o| json!({
                    "name": o.name,
                    "description": o.description,
                    "created_at": o.created_at.to_string(),
                }))
                .collect::<Vec<_>>());
            ctx.emit(value, |out| {
                let mut table = Table::new(&["NAME", "CREATED", "DESCRIPTION"]);
                for o in &orgs {
                    table.row(vec![
                        o.name.clone(),
                        when(&o.created_at),
                        o.description.clone().unwrap_or_default(),
                    ]);
                }
                out.table(table)
            })
        }
        OrgCmd::Show(args) => {
            let org = admin.org(&args.name).await?;
            ctx.emit(json!({ "name": org.name, "description": org.description, "created_at": org.created_at.to_string() }), |out| {
                out.kv(&[
                    ("org", org.name.clone()),
                    ("created", when(&org.created_at)),
                    ("description", org.description.clone().unwrap_or_default()),
                ])
            })
        }
        OrgCmd::Create(args) => {
            let org = admin
                .create_org(&args.name, args.description.as_deref())
                .await?;
            ctx.emit(json!({ "name": org.name }), |out| {
                out.line(format!("created org {}", out.bold(&org.name)))
            })
        }
    }
}
