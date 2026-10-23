//! `rill man`: man pages for every command, for packagers.

use std::path::PathBuf;

use clap::{Args, CommandFactory};

use crate::cli::Cli;
use crate::context::Ctx;
use crate::Result;

/// `rill man`.
#[derive(Args, Debug)]
pub struct ManArgs {
    /// Directory to write `rill.1`, `rill-land.1`, `rill-land-cancel.1`, ….
    #[arg(long, short = 'o', default_value = "man")]
    pub out: PathBuf,
}

fn write_pages(
    cmd: &clap::Command,
    prefix: &str,
    dir: &std::path::Path,
    count: &mut usize,
) -> std::io::Result<()> {
    let name = if prefix.is_empty() {
        cmd.get_name().to_string()
    } else {
        format!("{prefix}-{}", cmd.get_name())
    };
    let mut buffer = Vec::new();
    clap_mangen::Man::new(cmd.clone())
        .title(name.to_uppercase())
        .render(&mut buffer)?;
    std::fs::write(dir.join(format!("{name}.1")), buffer)?;
    *count += 1;
    for sub in cmd.get_subcommands().filter(|s| !s.is_hide_set()) {
        write_pages(sub, &name, dir, count)?;
    }
    Ok(())
}

/// Run `rill man`.
pub async fn run(ctx: &mut Ctx<'_>, args: ManArgs) -> Result<()> {
    std::fs::create_dir_all(&args.out)?;
    let mut command = Cli::command();
    command.build();
    let mut count = 0;
    write_pages(&command, "", &args.out, &mut count)?;
    ctx.emit(
        serde_json::json!({ "dir": args.out.display().to_string(), "pages": count }),
        |out| out.line(format!("wrote {count} man pages to {}", args.out.display())),
    )
}
