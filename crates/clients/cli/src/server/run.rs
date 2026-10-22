//! Running the service until ctrl-c, and writing its configuration template.

use rill_api::ServerConfig;
use tokio_util::sync::CancellationToken;

use super::args::{ApiArgs, ApiCmd, DevArgs};
use super::config::{api_config, dev_config};
use super::principal::parse_principal;
use super::template::annotated_config;
use crate::error::CliError;
use crate::Result;

/// Run `rill dev` until ctrl-c.
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

/// Run `rill api` until ctrl-c, or write the configuration template.
pub async fn api(ctx: &mut crate::context::Ctx<'_>, args: ApiArgs) -> Result<()> {
    match &args.command {
        Some(ApiCmd::InitConfig { out, force }) => write_template(out.as_deref(), *force),
        Some(ApiCmd::MintToken {
            principal,
            org,
            ttl,
        }) => {
            let config = api_config(&args)?;
            let principal = parse_principal(principal)?;
            let token = rill_runtime::identity::mint_token(&config, &principal, org, *ttl).await?;
            ctx.emit(serde_json::json!({ "token": token }), |out| {
                out.line(&token)
            })
        }
        None => serve(api_config(&args)?).await,
    }
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

fn write_template(out: Option<&std::path::Path>, force: bool) -> Result<()> {
    let text = annotated_config()?;
    match out {
        None => print!("{text}"),
        Some(path) => {
            if path.exists() && !force {
                return Err(CliError::Usage(format!(
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
