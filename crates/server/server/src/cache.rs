//! `rill-server cache`: the regional cache tier in front of an origin.

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Args;
use rill_cache::CacheConfig;
use tokio_util::sync::CancellationToken;

use crate::error::{Error, Result};

/// `rill-server cache`.
#[derive(Args, Debug)]
pub struct CacheArgs {
    /// The origin's HTTP listener, e.g. `https://rill.internal:7421`.
    #[arg(long, env = "RILL_CACHE_ORIGIN")]
    pub origin: String,
    /// Directory objects are kept in.
    #[arg(long, env = "RILL_CACHE_DIR")]
    pub dir: PathBuf,
    /// Most bytes kept, e.g. `500G`, `64M` or a byte count.
    #[arg(long, env = "RILL_CACHE_MAX_BYTES", default_value = "10G")]
    pub max_bytes: String,
    /// Listen address.
    #[arg(long, env = "RILL_CACHE_LISTEN", default_value = "0.0.0.0:7422")]
    pub listen: SocketAddr,
    /// Seconds between checks of the origin's redactions.
    #[arg(long, env = "RILL_CACHE_REDACTION_POLL", default_value_t = 10)]
    pub redaction_poll: u64,
    /// A service token the origin accepts for its redaction list.
    #[arg(long, env = "RILL_CACHE_ORIGIN_TOKEN", hide_env_values = true)]
    pub origin_token: Option<String>,
}

/// `500G`, `64M`, `1T`, `4096`.
pub fn parse_size(text: &str) -> Result<u64> {
    let text = text.trim();
    let (digits, unit) = text
        .find(|c: char| !c.is_ascii_digit())
        .map_or((text, ""), |i| text.split_at(i));
    let n: u64 = digits
        .parse()
        .map_err(|_| Error::usage(format!("invalid size {text:?}")))?;
    let scale: u64 = match unit.to_ascii_uppercase().as_str() {
        "" | "B" => 1,
        "K" | "KB" => 1 << 10,
        "M" | "MB" => 1 << 20,
        "G" | "GB" => 1 << 30,
        "T" | "TB" => 1 << 40,
        _ => return Err(Error::usage(format!("invalid size unit in {text:?}"))),
    };
    n.checked_mul(scale)
        .ok_or_else(|| Error::usage(format!("size {text:?} is too large")))
}

/// Run the cache until interrupted.
pub async fn run(args: CacheArgs) -> Result<()> {
    let _telemetry = rill_runtime::telemetry::init(&Default::default())?;
    let mut config = CacheConfig::new(args.origin, args.dir, parse_size(&args.max_bytes)?);
    config.listen = args.listen;
    config.redaction_poll = std::time::Duration::from_secs(args.redaction_poll.max(1));
    config.redaction_max_age = (config.redaction_poll * 6).max(std::time::Duration::from_secs(60));
    config.origin_token = args.origin_token;
    let shutdown = CancellationToken::new();
    let signal = shutdown.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            signal.cancel();
        }
    });
    Ok(rill_cache::serve(config, shutdown).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_take_binary_units() {
        assert_eq!(parse_size("4096").unwrap(), 4096);
        assert_eq!(parse_size("64M").unwrap(), 64 << 20);
        assert_eq!(parse_size("1tb").unwrap(), 1 << 40);
        assert!(parse_size("12X").is_err());
        assert!(parse_size("G").is_err());
    }
}
