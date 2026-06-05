mod core;
mod network;
mod ui;

use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::core::session::SessionManager;
use crate::ui::web::{build_router, AppState};

#[tokio::main]
async fn main() {
    // init logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "bgmi_event_bot=debug,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("bgmi-event-bot v{}", env!("CARGO_PKG_VERSION"));
    info!("protocol: ITOP SDK v2.10.3 / globh.com HTTPS");

    let session_mgr = Arc::new(SessionManager::new());
    let state: AppState = session_mgr;

    let app = build_router(state);

    let bind = "0.0.0.0:3000";
    info!("dashboard running at http://localhost:3000");

    let listener = tokio::net::TcpListener::bind(bind).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
