use axum::http::{header, HeaderValue, Method};
use flowpay_runtime::{config::Config, routes, state::AppState};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
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
    let bind = config.bind.clone();
    let state = AppState::build(config).await?;

    let allowed_origins = std::env::var("FLOWPAY_ALLOWED_ORIGINS")
        .unwrap_or_else(|_| "http://localhost:3000,http://localhost:3001".into())
        .split(',')
        .filter_map(|origin| origin.trim().parse::<HeaderValue>().ok())
        .collect::<Vec<_>>();
    let cors = CorsLayer::new()
        .allow_origin(allowed_origins)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            header::HeaderName::from_static("x-flowpay-api-key"),
            header::HeaderName::from_static("idempotency-key"),
            header::HeaderName::from_static("x-alchemy-signature"),
        ]);

    let app = routes::router(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(%bind, "FlowPay API listening");
    axum::serve(listener, app).await?;
    Ok(())
}
