use std::{env, net::SocketAddr, path::PathBuf};

use clap::{Args, Parser, Subcommand};
use flux_purr_devd::{AppConfig, AppState, app};
use tokio::net::TcpListener;

#[derive(Debug, Parser)]
#[command(name = "flux-purr-devd", version = flux_purr_devd::PRODUCT_VERSION)]
#[command(about = "Flux Purr local USB/devd bridge")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Serve(ServeArgs),
}

#[derive(Debug, Args)]
struct ServeArgs {
    #[arg(long)]
    bind: Option<SocketAddr>,
    #[arg(long = "artifact-root")]
    artifact_root: Option<PathBuf>,
    #[arg(long = "serial-port")]
    serial_port: Option<PathBuf>,
    #[arg(long = "allow-dev-cors")]
    allow_dev_cors: bool,
    #[arg(long = "no-dev-cors")]
    no_dev_cors: bool,
    #[arg(long = "allow-real-flash")]
    allow_real_flash: bool,
}

impl Default for ServeArgs {
    fn default() -> Self {
        Self {
            bind: None,
            artifact_root: None,
            serial_port: None,
            allow_dev_cors: false,
            no_dev_cors: false,
            allow_real_flash: false,
        }
    }
}

#[tokio::main]
async fn main() {
    #[cfg(unix)]
    absorb_serial_hangups();

    let cli = Cli::parse();
    let args = match cli.command {
        Some(Command::Serve(args)) => args,
        None => ServeArgs::default(),
    };
    let config = parse_config(args);
    let listener = TcpListener::bind(config.bind)
        .await
        .expect("failed to bind flux-purr-devd listener");
    let state = AppState::new(config);
    tokio::spawn(state.clone().run_lease_reaper());

    eprintln!(
        "flux-purr-devd listening on {}",
        listener.local_addr().unwrap()
    );
    axum::serve(listener, app(state))
        .await
        .expect("flux-purr-devd server failed");
}

#[cfg(unix)]
fn absorb_serial_hangups() {
    use tokio::signal::unix::{SignalKind, signal};

    let mut hangups = signal(SignalKind::hangup())
        .expect("failed to register the Flux Purr serial hangup handler");
    tokio::spawn(async move {
        // ESP32-S3 USB Serial/JTAG disconnects during a reset or flash can
        // generate SIGHUP. The serial RPC layer already handles reconnects;
        // terminating the daemon here aborts the protected flash transaction.
        while hangups.recv().await.is_some() {}
    });
}

fn parse_config(args: ServeArgs) -> AppConfig {
    let bind = args
        .bind
        .or_else(|| {
            env::var("FLUX_PURR_DEVD_BIND")
                .ok()
                .and_then(|value| value.parse::<SocketAddr>().ok())
        })
        .unwrap_or_else(|| "127.0.0.1:30080".parse().unwrap());
    let artifact_root = args.artifact_root.or_else(|| {
        env::var("FLUX_PURR_DEVD_ARTIFACT_ROOT")
            .ok()
            .map(PathBuf::from)
    });
    let allow_dev_cors = if args.no_dev_cors {
        false
    } else if args.allow_dev_cors {
        true
    } else {
        env::var("FLUX_PURR_DEVD_DEV_CORS")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or_else(|_| default_dev_cors_for_bind(bind))
    };
    let allow_real_flash = args.allow_real_flash
        || env::var("FLUX_PURR_DEVD_ALLOW_REAL_FLASH")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
    let serial_port = args.serial_port;

    AppConfig {
        bind,
        artifact_root,
        allow_dev_cors,
        allow_real_flash,
        serial_port,
    }
}

fn default_dev_cors_for_bind(bind: SocketAddr) -> bool {
    bind.ip().is_loopback()
}

#[cfg(test)]
mod tests {
    use super::{ServeArgs, default_dev_cors_for_bind, parse_config};

    #[test]
    fn omitted_serial_port_does_not_select_a_device() {
        assert_eq!(parse_config(ServeArgs::default()).serial_port, None);
    }

    #[test]
    fn default_dev_cors_is_enabled_for_loopback_binds() {
        assert!(default_dev_cors_for_bind(
            "127.0.0.1:30080".parse().unwrap()
        ));
        assert!(default_dev_cors_for_bind("[::1]:30080".parse().unwrap()));
    }

    #[test]
    fn default_dev_cors_is_disabled_for_non_loopback_binds() {
        assert!(!default_dev_cors_for_bind("0.0.0.0:30080".parse().unwrap()));
    }
}
