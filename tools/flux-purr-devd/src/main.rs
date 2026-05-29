use std::{env, net::SocketAddr, path::PathBuf};

use flux_purr_devd::{AppConfig, AppState, DEFAULT_SERIAL_PORT, app};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    if env::args().any(|arg| arg == "-h" || arg == "--help") {
        print_help();
        return;
    }

    let config = parse_config();
    let listener = TcpListener::bind(config.bind)
        .await
        .expect("failed to bind flux-purr-devd listener");
    let state = AppState::new(config);

    eprintln!(
        "flux-purr-devd listening on {}",
        listener.local_addr().unwrap()
    );
    axum::serve(listener, app(state))
        .await
        .expect("flux-purr-devd server failed");
}

fn print_help() {
    println!(
        "flux-purr-devd\n\n\
         Environment:\n\
           FLUX_PURR_DEVD_BIND=127.0.0.1:30080\n\
           FLUX_PURR_DEVD_ARTIFACT_ROOT=<path>\n\
           FLUX_PURR_DEVD_SERIAL_PORT=/dev/cu.usbmodem21221401\n\
           FLUX_PURR_DEVD_DEV_CORS=0|1 (default: enabled for loopback binds)\n\
           FLUX_PURR_DEVD_ALLOW_REAL_FLASH=1"
    );
}

fn parse_config() -> AppConfig {
    let bind = env::var("FLUX_PURR_DEVD_BIND")
        .ok()
        .and_then(|value| value.parse::<SocketAddr>().ok())
        .unwrap_or_else(|| "127.0.0.1:30080".parse().unwrap());
    let artifact_root = env::var("FLUX_PURR_DEVD_ARTIFACT_ROOT")
        .ok()
        .map(PathBuf::from);
    let allow_dev_cors = env::var("FLUX_PURR_DEVD_DEV_CORS")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or_else(|_| default_dev_cors_for_bind(bind));
    let allow_real_flash = env::var("FLUX_PURR_DEVD_ALLOW_REAL_FLASH")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let serial_port = Some(PathBuf::from(
        env::var("FLUX_PURR_DEVD_SERIAL_PORT").unwrap_or_else(|_| DEFAULT_SERIAL_PORT.to_string()),
    ));

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
    use super::default_dev_cors_for_bind;

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
