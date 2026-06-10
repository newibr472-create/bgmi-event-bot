//! Web UI and API routes
//! Adds matchmaking endpoints alongside existing event collection

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse},
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tracing::info;

use crate::core::account::{Account, DeviceProfile, StoredCredential};
use crate::core::lobby::{GameMode, MapId};
use crate::core::orchestrator::{BotConfig, BotOrchestrator, BotRunResult};
use crate::core::session::SessionManager;

pub type AppState = Arc<SessionManager>;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index_page))
        // Account management
        .route("/api/accounts", get(list_accounts).post(add_account))
        .route("/api/accounts/{id}", delete(remove_account))
        // Event collection (existing)
        .route("/api/collect/{id}", post(collect_single))
        .route("/api/collect-all", post(collect_all))
        .route("/api/results", get(get_results))
        // NEW: Matchmaking / Auto-play
        .route("/api/match/start/{id}", post(start_match))
        .route("/api/match/status", get(match_status))
        // Status
        .route("/api/status", get(get_status))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn index_page() -> Html<&'static str> {
    Html(include_str!("../../static/index.html"))
}

// ─── Account Endpoints ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct AddAccountRequest {
    label: String,
    credential_type: String,
    oauth_token: Option<String>,
    oauth_token_secret: Option<String>,
    access_token: Option<String>,
    device_id: Option<String>,
    model: Option<String>,
    brand: Option<String>,
}

#[derive(Serialize)]
struct ApiResponse<T: Serialize> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    fn ok(data: T) -> Json<Self> {
        Json(Self {
            success: true,
            data: Some(data),
            error: None,
        })
    }

    fn err(msg: &str) -> Json<Self> {
        Json(Self {
            success: false,
            data: None,
            error: Some(msg.to_string()),
        })
    }
}

async fn list_accounts(State(state): State<AppState>) -> impl IntoResponse {
    let accounts = state.get_accounts().await;
    let display: Vec<AccountDisplay> = accounts.iter().map(AccountDisplay::from).collect();
    ApiResponse::ok(display)
}

async fn add_account(
    State(state): State<AppState>,
    Json(req): Json<AddAccountRequest>,
) -> impl IntoResponse {
    let credential = match req.credential_type.as_str() {
        "twitter" => {
            let token = match req.oauth_token {
                Some(t) => t,
                None => return ApiResponse::<String>::err("missing oauth_token"),
            };
            let secret = match req.oauth_token_secret {
                Some(s) => s,
                None => return ApiResponse::<String>::err("missing oauth_token_secret"),
            };
            StoredCredential::Twitter {
                oauth_token: token,
                oauth_token_secret: secret,
            }
        }
        "facebook" => {
            let token = match req.access_token {
                Some(t) => t,
                None => return ApiResponse::<String>::err("missing access_token"),
            };
            StoredCredential::Facebook {
                access_token: token,
            }
        }
        "google" => {
            let token = match req.access_token {
                Some(t) => t,
                None => return ApiResponse::<String>::err("missing access_token (id_token)"),
            };
            StoredCredential::Google { id_token: token }
        }
        "guest" => StoredCredential::Guest,
        _ => return ApiResponse::err("invalid credential_type"),
    };

    let device = DeviceProfile {
        device_id: req
            .device_id
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        model: req.model.unwrap_or_else(|| "I2405".to_string()),
        brand: req.brand.unwrap_or_else(|| "iQOO".to_string()),
        ..Default::default()
    };

    let account = Account::new(&req.label, credential, device);
    let id = account.id.clone();
    state.add_account(account).await;

    info!("added account: {} ({})", req.label, id);
    ApiResponse::ok(id)
}

async fn remove_account(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if state.remove_account(&id).await {
        ApiResponse::ok("removed")
    } else {
        ApiResponse::<&str>::err("account not found")
    }
}

// ─── Event Collection Endpoints ─────────────────────────────────────────────

async fn collect_single(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.collect_for_account(&id).await {
        Ok(results) => ApiResponse::ok(results),
        Err(e) => ApiResponse::err(&e.to_string()),
    }
}

async fn collect_all(State(state): State<AppState>) -> impl IntoResponse {
    let results = state.collect_all().await;
    ApiResponse::ok(results)
}

async fn get_results(State(state): State<AppState>) -> impl IntoResponse {
    let results = state.get_results().await;
    ApiResponse::ok(results)
}

// ─── Matchmaking Endpoints ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct MatchRequest {
    #[serde(default = "default_mode")]
    game_mode: String,
    #[serde(default = "default_map")]
    map: String,
}

fn default_mode() -> String {
    "classic_squad_tpp".to_string()
}
fn default_map() -> String {
    "erangel".to_string()
}

#[derive(Serialize)]
struct MatchResponse {
    status: String,
    account_id: String,
    result: Option<BotRunResultDisplay>,
}

#[derive(Serialize)]
struct BotRunResultDisplay {
    success: bool,
    login_ok: bool,
    lobby_connected: bool,
    match_found: bool,
    match_stats: Option<MatchStatsDisplay>,
    error: Option<String>,
}

#[derive(Serialize)]
struct MatchStatsDisplay {
    match_id: String,
    duration_secs: u64,
    alive_time_secs: u64,
    players_remaining: u8,
    placement: u8,
}

impl From<&BotRunResult> for BotRunResultDisplay {
    fn from(r: &BotRunResult) -> Self {
        Self {
            success: r.success,
            login_ok: r.login_ok,
            lobby_connected: r.lobby_connected,
            match_found: r.match_found,
            match_stats: r.match_stats.as_ref().map(|s| MatchStatsDisplay {
                match_id: s.match_id.clone(),
                duration_secs: s.duration_secs,
                alive_time_secs: s.alive_time_secs,
                players_remaining: s.players_remaining,
                placement: s.placement,
            }),
            error: r.error.clone(),
        }
    }
}

async fn start_match(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<MatchRequest>,
) -> impl IntoResponse {
    // Find account
    let accounts = state.get_accounts().await;
    let account = match accounts.iter().find(|a| a.id == id) {
        Some(a) => a.clone(),
        None => return ApiResponse::err("account not found"),
    };

    // Parse game mode
    let game_mode = match req.game_mode.as_str() {
        "classic_squad_tpp" => GameMode::ClassicSquadTPP,
        "classic_squad_fpp" => GameMode::ClassicSquadFPP,
        "classic_duo_tpp" => GameMode::ClassicDuoTPP,
        "classic_duo_fpp" => GameMode::ClassicDuoFPP,
        "classic_solo_tpp" => GameMode::ClassicSoloTPP,
        "classic_solo_fpp" => GameMode::ClassicSoloFPP,
        "arcade_squad_tpp" => GameMode::ArcadeSquadTPP,
        "arena_tpp" => GameMode::ArenaTPP,
        _ => GameMode::ClassicSquadTPP,
    };

    // Parse map
    let map = match req.map.as_str() {
        "random" => MapId::Random,
        "erangel" => MapId::Erangel,
        "miramar" => MapId::Miramar,
        "sanhok" => MapId::Sanhok,
        "vikendi" => MapId::Vikendi,
        "livik" => MapId::Livik,
        "karakin" => MapId::Karakin,
        "nusa" => MapId::Nusa,
        _ => MapId::Erangel,
    };

    let config = BotConfig {
        game_mode,
        map,
        ..Default::default()
    };

    info!("starting match for account {}: {:?} on {:?}", id, game_mode, map);

    // Run bot (this will take a while - ideally would be spawned async)
    let orchestrator = BotOrchestrator::new(account, config);
    let run_result = orchestrator.run().await;

    let display = BotRunResultDisplay::from(&run_result);

    ApiResponse::ok(MatchResponse {
        status: if run_result.success { "completed" } else { "failed" }.to_string(),
        account_id: id,
        result: Some(display),
    })
}

async fn match_status(State(_state): State<AppState>) -> impl IntoResponse {
    // TODO: Track active matches and return their status
    ApiResponse::ok(serde_json::json!({
        "active_matches": 0,
        "message": "use POST /api/match/start/:id to start a match"
    }))
}

// ─── Status ─────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct StatusInfo {
    accounts: usize,
    total_results: usize,
    version: &'static str,
    protocol: &'static str,
    features: Vec<&'static str>,
}

async fn get_status(State(state): State<AppState>) -> impl IntoResponse {
    let accounts = state.get_accounts().await;
    let results = state.get_results().await;

    ApiResponse::ok(StatusInfo {
        accounts: accounts.len(),
        total_results: results.len(),
        version: env!("CARGO_PKG_VERSION"),
        protocol: "ITOP SDK v2.10.3 / TGCP UDP / globh.com HTTPS",
        features: vec![
            "login",
            "event_collection",
            "lobby_keepalive",
            "matchmaking",
            "match_join",
            "auto_play_afk",
        ],
    })
}

#[derive(Serialize)]
struct AccountDisplay {
    id: String,
    label: String,
    credential_type: String,
    status: String,
    openid: Option<String>,
    username: Option<String>,
}

impl From<&Account> for AccountDisplay {
    fn from(a: &Account) -> Self {
        let cred_type = match &a.credential {
            StoredCredential::Twitter { .. } => "twitter",
            StoredCredential::Facebook { .. } => "facebook",
            StoredCredential::Google { .. } => "google",
            StoredCredential::Guest => "guest",
        };

        Self {
            id: a.id.clone(),
            label: a.label.clone(),
            credential_type: cred_type.to_string(),
            status: a.status.to_string(),
            openid: a.openid.clone(),
            username: a.username.clone(),
        }
    }
}
