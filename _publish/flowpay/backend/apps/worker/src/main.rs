use flowpay_runtime::{config::Config, state::AppState, workers};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,flowpay=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    let config = Config::from_env()?;
    tokio::fs::create_dir_all(&config.evidence_dir).await?;
    let state = AppState::build(config).await?;
    workers::run(state).await
}
