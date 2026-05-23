use std::{env, net::SocketAddr, path::PathBuf};

use flux_purr_devd::{AppConfig, AppState, app};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
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
        .unwrap_or(false);
    let allow_real_flash = env::var("FLUX_PURR_DEVD_ALLOW_REAL_FLASH")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    AppConfig {
        bind,
        artifact_root,
        allow_dev_cors,
        allow_real_flash,
    }
}
