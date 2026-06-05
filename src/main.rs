mod core;
mod network;
mod ui;

use anyhow::Result;
use parking_lot::RwLock;
use std::sync::Arc;
use tracing::{info, warn};

use crate::core::account::AccountManager;
use crate::core::events::EventScheduler;
use crate::ui::web::launch_webview;

pub struct AppState {
    pub accounts: AccountManager,
    pub event_scheduler: EventScheduler,
    pub shutdown: tokio::sync::broadcast::Sender<()>,
}

impl AppState {
    fn new() -> Self {
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);
        Self {
            accounts: AccountManager::new(),
            event_scheduler: EventScheduler::new(),
            shutdown: shutdown_tx,
        }
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("bgmi_event_bot=debug,warn")
        .with_target(false)
        .compact()
        .init();

    info!("bgmi-event-bot v{}", env!("CARGO_PKG_VERSION"));

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;

    let state = Arc::new(RwLock::new(AppState::new()));
    let state_clone = state.clone();

    // spawn background tasks on tokio runtime
    rt.spawn(async move {
        let scheduler = {
            let s = state_clone.read();
            s.event_scheduler.clone()
        };
        if let Err(e) = scheduler.run_loop().await {
            warn!("event scheduler exited: {}", e);
        }
    });

    // launch webview on main thread (required by tao/wry)
    launch_webview(state)?;

    info!("shutting down");
    rt.shutdown_timeout(std::time::Duration::from_secs(3));
    Ok(())
}
