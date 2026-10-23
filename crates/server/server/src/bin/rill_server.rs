//! The `rill-server` binary; everything lives in the library.

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    // The services install their own telemetry; the tools log warnings.
    let serving = args
        .iter()
        .skip(1)
        .find(|a| !a.to_string_lossy().starts_with('-'))
        .is_some_and(|a| a == "run" || a == "dev" || a == "cache");
    if !serving {
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .without_time()
            .try_init();
    }
    let code = rill_server::run_with_args(args, &mut std::io::stdout()).await;
    std::process::ExitCode::from(u8::try_from(code).unwrap_or(1))
}
