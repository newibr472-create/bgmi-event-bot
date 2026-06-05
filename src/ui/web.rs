use axum::{
    extract::State,
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

use crate::core::account::{Account, DeviceProfile, StoredCredential};
use crate::core::session::SessionManager;

pub type AppState = Arc<SessionManager>;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index_page))
        .route("/api/accounts", get(list_accounts).post(add_account))
        .route("/api/accounts/:id", axum::routing::delete(remove_account))
        .route("/api/collect/:id", post(collect_single))
        .route("/api/collect-all", post(collect_all))
        .route("/api/results", get(get_results))
        .route("/api/status", get(get_status))
        .with_state(state)
}

async fn index_page() -> Html<&'static str> {
    Html(include_str!("../../static/index.html"))
}

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
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    if state.remove_account(&id).await {
        ApiResponse::ok("removed")
    } else {
        ApiResponse::<&str>::err("account not found")
    }
}

async fn collect_single(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
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

#[derive(Serialize)]
struct StatusInfo {
    accounts: usize,
    total_results: usize,
    version: &'static str,
    protocol: &'static str,
}

async fn get_status(State(state): State<AppState>) -> impl IntoResponse {
    let accounts = state.get_accounts().await;
    let results = state.get_results().await;

    ApiResponse::ok(StatusInfo {
        accounts: accounts.len(),
        total_results: results.len(),
        version: env!("CARGO_PKG_VERSION"),
        protocol: "ITOP SDK v2.10.3 / globh.com HTTPS",
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
