use anyhow::Result;
use parking_lot::RwLock;
use std::sync::Arc;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;
use tracing::{error, info};
use wry::WebViewBuilder;

use super::APP_HTML;
use crate::AppState;

pub fn launch_webview(state: Arc<RwLock<AppState>>) -> Result<()> {
    let event_loop = EventLoop::new();

    let window = WindowBuilder::new()
        .with_title("BGMI Event Bot")
        .with_inner_size(tao::dpi::LogicalSize::new(920.0, 680.0))
        .with_min_inner_size(tao::dpi::LogicalSize::new(640.0, 480.0))
        .build(&event_loop)?;

    let state_for_ipc = state.clone();

    let _webview = WebViewBuilder::new(&window)
        .with_html(APP_HTML)
        .with_ipc_handler(move |msg| {
            handle_ipc_message(&state_for_ipc, msg.body());
        })
        .with_devtools(cfg!(debug_assertions))
        .build()?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                info!("window close requested");
                let s = state.read();
                let _ = s.shutdown.send(());
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}

fn handle_ipc_message(state: &Arc<RwLock<AppState>>, raw: &str) {
    let msg: serde_json::Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(e) => {
            error!("invalid ipc message: {}", e);
            return;
        }
    };

    let cmd = msg.get("cmd").and_then(|v| v.as_str()).unwrap_or("");
    let args = msg.get("args").cloned().unwrap_or(serde_json::json!({}));

    match cmd {
        "add_account" => {
            let token = args.get("token").and_then(|v| v.as_str()).unwrap_or("");
            if token.is_empty() {
                return;
            }
            let s = state.read();
            match s.accounts.import_from_token(token) {
                Ok(acc) => {
                    info!("imported account: {} ({})", acc.display_name, acc.open_id);
                }
                Err(e) => {
                    error!("failed to import account: {}", e);
                }
            }
        }
        "remove_account" => {
            let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if !id.is_empty() {
                let s = state.read();
                if let Some(acc) = s.accounts.remove(id) {
                    info!("removed account: {}", acc.display_name);
                }
            }
        }
        "refresh_events" => {
            info!("refresh events requested");
            // trigger event list fetch on active sessions
        }
        "claim_all" => {
            info!("claim all requested");
            // iterate active accounts and claim pending rewards
        }
        "start_match" => {
            info!("match simulation start requested");
            // spawn match sim task
        }
        "stop_match" => {
            info!("match simulation stop requested");
        }
        other => {
            error!("unknown ipc command: {}", other);
        }
    }
}
