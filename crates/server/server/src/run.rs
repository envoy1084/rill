//! Running the services until ctrl-c, minting a bootstrap token, and
//! writing the configuration template.

use std::io::Write;

use rill_api::ServerConfig;
use tokio_util::sync::CancellationToken;

use crate::args::{DevArgs, MintArgs, ServeArgs};
use crate::config::{dev_config, serve_config};
use crate::error::{Error, Result};
use crate::principal::parse_principal;
use crate::template::annotated_config;

/// Run `dev` until ctrl-c.
pub async fn dev(args: DevArgs) -> Result<()> {
    let config = dev_config(&args)?;
    if let rill_identity::keys::KeyServiceConfig::Local {
        master_key_file: Some(path),
        ..
    } = &config.identity.keys
    {
        ensure_master_key(path).await?;
    }
    serve(config).await
}

/// Run `run` until ctrl-c.
pub async fn run(args: ServeArgs) -> Result<()> {
    serve(serve_config(&args)?).await
}

/// Print a token for `mint-token`.
pub async fn mint(args: MintArgs, out: &mut dyn Write) -> Result<()> {
    let config = serve_config(&args.serve)?;
    let principal = parse_principal(&args.principal)?;
    let token =
        rill_runtime::identity::mint_token(&config, &principal, &args.org, args.ttl).await?;
    writeln!(out, "{token}")?;
    Ok(())
}

/// Create the dev server's master key file the first time, so its token
/// keys open again after a restart.
async fn ensure_master_key(path: &std::path::Path) -> Result<()> {
    if tokio::fs::try_exists(path).await? {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    let key = hex::encode(rand::random::<[u8; 32]>());
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    use tokio::io::AsyncWriteExt;
    options.open(path).await?.write_all(key.as_bytes()).await?;
    Ok(())
}

async fn serve(config: ServerConfig) -> Result<()> {
    let _telemetry = rill_runtime::telemetry::init(&config.telemetry)?;
    if config.dev_mode {
        tracing::warn!(storage = ?config.storage, "dev mode: no authentication, every request is permitted");
    }
    let shutdown = CancellationToken::new();
    let signal = shutdown.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            tracing::info!("ctrl-c received, shutting down");
            signal.cancel();
        }
    });
    Ok(rill_api::run(config, shutdown).await?)
}

/// Write the configuration template to `out`, or to the file `path`.
pub fn write_template(
    path: Option<&std::path::Path>,
    force: bool,
    out: &mut dyn Write,
) -> Result<()> {
    let text = annotated_config()?;
    match path {
        None => out.write_all(text.as_bytes())?,
        Some(path) => {
            if path.exists() && !force {
                return Err(Error::Usage(format!(
                    "{} exists; pass --force to overwrite",
                    path.display()
                )));
            }
            std::fs::write(path, text)?;
            eprintln!("wrote {}", path.display());
        }
    }
    Ok(())
}
