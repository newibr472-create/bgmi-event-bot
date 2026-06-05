pub mod web;

use axum::{
    body::Bytes,
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::AppState;

pub fn api_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(index_handler))
        .route("/api/status", get(status_handler))
        .route("/api/accounts", get(list_accounts).post(add_account))
        .route("/api/start", post(start_bot))
        .route("/api/stop", post(stop_bot))
        .route("/api/events", get(get_events))
        .route("/api/logs", get(get_logs))
}

async fn index_handler() -> Html<&'static str> {
    Html(web::DASHBOARD_HTML)
}

async fn status_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let running = *state.bot_running.read();
    let account_count = state.accounts.count();
    Json(serde_json::json!({
        "running": running,
        "accounts": account_count,
        "uptime_sec": 0,
    }))
}

#[derive(Deserialize)]
struct AddAccountRequest {
    token: String,
}

async fn add_account(
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> impl IntoResponse {
    let payload: AddAccountRequest = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            // try to extract token from raw body as fallback
            let raw = String::from_utf8_lossy(&body);
            state.push_log(format!("Import failed: {}", e));
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e.to_string(), "raw_len": raw.len()})),
            );
        }
    };
    if payload.token.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "token is required"})),
        );
    }

    match state.accounts.import_from_token(&payload.token) {
        Ok(acc) => {
            state.push_log(format!("Account imported: {} ({})", acc.display_name, acc.open_id));
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "id": acc.id,
                    "display_name": acc.display_name,
                    "open_id": acc.open_id,
                })),
            )
        }
        Err(e) => {
            state.push_log(format!("Import failed: {}", e));
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e.to_string()})),
            )
        }
    }
}

async fn list_accounts(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let accounts: Vec<_> = state
        .accounts
        .list()
        .into_iter()
        .map(|a| {
            serde_json::json!({
                "id": a.id,
                "display_name": a.display_name,
                "open_id": a.open_id,
                "session_active": a.session_active,
                "region": a.region,
                "last_login": a.last_login,
            })
        })
        .collect();
    Json(serde_json::json!({"accounts": accounts}))
}

async fn start_bot(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut running = state.bot_running.write();
    if *running {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "bot already running"})),
        );
    }
    *running = true;
    state.push_log("Bot started".to_string());
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "started"})),
    )
}

async fn stop_bot(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut running = state.bot_running.write();
    if !*running {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "bot not running"})),
        );
    }
    *running = false;
    state.push_log("Bot stopped".to_string());
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "stopped"})),
    )
}

async fn get_events(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let events = state.event_scheduler.get_claimable_events().await;
    let events_json: Vec<_> = events
        .into_iter()
        .map(|e| {
            serde_json::json!({
                "event_id": e.event_id,
                "name": e.name,
                "event_type": e.event_type,
                "progress": e.progress,
                "claimed": e.claimed,
                "rewards": e.rewards,
                "end_time": e.end_time,
            })
        })
        .collect();
    Json(serde_json::json!({"events": events_json}))
}

async fn get_logs(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let logs = state.logs.read();
    let recent: Vec<_> = logs.iter().rev().take(50).cloned().collect();
    Json(serde_json::json!({"logs": recent}))
}
