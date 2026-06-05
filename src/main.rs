#![allow(dead_code)]

mod core;
mod network;
mod ui;

use anyhow::Result;
use std::sync::Arc;
use tracing::info;

use crate::core::account::AccountManager;
use crate::core::events::EventScheduler;
use crate::ui::web::start_server;

pub struct AppState {
    pub accounts: AccountManager,
    pub event_scheduler: EventScheduler,
    pub bot_running: parking_lot::RwLock<bool>,
    pub logs: parking_lot::RwLock<Vec<LogEntry>>,
    pub shutdown: tokio::sync::broadcast::Sender<()>,
}

#[derive(Clone, serde::Serialize)]
pub struct LogEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub message: String,
}

impl AppState {
    fn new() -> Self {
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);
        Self {
            accounts: AccountManager::new(),
            event_scheduler: EventScheduler::new(),
            bot_running: parking_lot::RwLock::new(false),
            logs: parking_lot::RwLock::new(Vec::new()),
            shutdown: shutdown_tx,
        }
    }

    pub fn push_log(&self, message: impl Into<String>) {
        let entry = LogEntry {
            timestamp: chrono::Utc::now(),
            message: message.into(),
        };
        let mut logs = self.logs.write();
        logs.push(entry);
        if logs.len() > 200 {
            logs.drain(0..50);
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("bgmi_event_bot=debug,warn")
        .with_target(false)
        .compact()
        .init();

    info!("bgmi-event-bot v{}", env!("CARGO_PKG_VERSION"));

    let state = Arc::new(AppState::new());
    let state_clone = state.clone();

    // spawn background event scheduler
    tokio::spawn(async move {
        let scheduler = state_clone.event_scheduler.clone();
        if let Err(e) = scheduler.run_loop().await {
            tracing::warn!("event scheduler exited: {}", e);
        }
    });

    // start web server
    start_server(state).await?;

    Ok(())
}
