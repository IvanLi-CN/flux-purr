#[path = "flux_purr/cli.rs"]
mod cli;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    cli::run().await
}
