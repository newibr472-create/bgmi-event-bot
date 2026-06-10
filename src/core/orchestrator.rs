//! BGMI Bot Orchestrator
//!
//! Full flow: Login → Lobby → Matchmaking → Match → Auto-Play
//! This is the high-level controller that ties all modules together.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::task;
use tracing::{error, info, warn};

use crate::core::account::Account;
use crate::core::lobby::{GameMode, LobbyConnection, LobbySessionParams, LobbyState, MapId, MatchAssignment};
use crate::core::match_conn::{MatchConnection, MatchStats};
use crate::network::client::BgmiClient;

/// Configuration for a bot run
#[derive(Debug, Clone)]
pub struct BotConfig {
    pub game_mode: GameMode,
    pub map: MapId,
    /// How long to wait for matchmaking (seconds)
    pub matchmaking_timeout_secs: u64,
    /// How long to keep alive in lobby before requesting match (seconds)
    pub lobby_warmup_secs: u64,
    /// Whether to send GCloud config request (looks more legitimate)
    pub send_gcloud_config: bool,
}

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            game_mode: GameMode::ClassicSquadTPP,
            map: MapId::Erangel,
            matchmaking_timeout_secs: 120,
            lobby_warmup_secs: 5,
            send_gcloud_config: true,
        }
    }
}

/// Result of a complete bot run
#[derive(Debug, Clone)]
pub struct BotRunResult {
    pub success: bool,
    pub login_ok: bool,
    pub lobby_connected: bool,
    pub match_found: bool,
    pub match_stats: Option<MatchStats>,
    pub error: Option<String>,
}

/// Main orchestrator for the full bot flow
pub struct BotOrchestrator {
    account: Account,
    config: BotConfig,
}

impl BotOrchestrator {
    pub fn new(account: Account, config: BotConfig) -> Self {
        Self { account, config }
    }

    /// Run the complete flow: login → lobby → match → auto-play
    pub async fn run(&self) -> BotRunResult {
        let mut result = BotRunResult {
            success: false,
            login_ok: false,
            lobby_connected: false,
            match_found: false,
            match_stats: None,
            error: None,
        };

        // ─── Step 1: Login via HTTPS ────────────────────────────────────────
        info!("=== Step 1: Login ===");
        let (openid, ticket) = match self.do_login().await {
            Ok(v) => v,
            Err(e) => {
                error!("login failed: {}", e);
                result.error = Some(format!("login: {}", e));
                return result;
            }
        };
        result.login_ok = true;
        info!("login success: openid={}", openid);

        // Small delay to mimic real app
        tokio::time::sleep(Duration::from_millis(1500)).await;

        // ─── Step 2: Connect to Lobby ───────────────────────────────────────
        info!("=== Step 2: Lobby Connection ===");
        let lobby_session = LobbySessionParams::from_login(&openid, &ticket);
        let lobby = match LobbyConnection::new(lobby_session) {
            Ok(l) => Arc::new(l),
            Err(e) => {
                error!("lobby creation failed: {}", e);
                result.error = Some(format!("lobby create: {}", e));
                return result;
            }
        };

        // Connect to gateway
        if let Err(e) = lobby.connect_gateway() {
            warn!("gateway connect failed (continuing with defaults): {}", e);
        }
        result.lobby_connected = true;

        // Start keepalive in background thread
        let lobby_clone = lobby.clone();
        let keepalive_handle = task::spawn_blocking(move || {
            lobby_clone.run_keepalive_loop()
        });

        // Warmup: stay in lobby a bit before requesting match
        info!("lobby warmup: {}s", self.config.lobby_warmup_secs);
        tokio::time::sleep(Duration::from_secs(self.config.lobby_warmup_secs)).await;

        // Send GCloud config request (optional, for legitimacy)
        if self.config.send_gcloud_config {
            let _ = lobby.fetch_voice_config();
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // ─── Step 3: Request Matchmaking ────────────────────────────────────
        info!("=== Step 3: Matchmaking ===");
        if let Err(e) = lobby.request_match(self.config.game_mode, self.config.map) {
            error!("match request failed: {}", e);
            lobby.stop();
            result.error = Some(format!("match request: {}", e));
            return result;
        }

        // Wait for match assignment
        let assignment = match self.wait_for_match(&lobby).await {
            Ok(a) => a,
            Err(e) => {
                error!("matchmaking failed: {}", e);
                lobby.stop();
                result.error = Some(format!("matchmaking: {}", e));
                return result;
            }
        };
        result.match_found = true;
        info!("match found: server={}, id={}", assignment.server_addr, assignment.match_id);

        // Stop lobby keepalive
        lobby.stop();
        let _ = keepalive_handle.await;

        tokio::time::sleep(Duration::from_millis(500)).await;

        // ─── Step 4: Join Match ─────────────────────────────────────────────
        info!("=== Step 4: Join Match ===");
        let match_conn = match MatchConnection::new(assignment) {
            Ok(c) => Arc::new(c),
            Err(e) => {
                error!("match connection failed: {}", e);
                result.error = Some(format!("match connect: {}", e));
                return result;
            }
        };

        if let Err(e) = match_conn.join() {
            error!("match join failed: {}", e);
            result.error = Some(format!("match join: {}", e));
            return result;
        }

        // ─── Step 5: Auto-Play (AFK Loop) ──────────────────────────────────
        info!("=== Step 5: Auto-Play ===");
        let match_clone = match_conn.clone();
        let match_result = task::spawn_blocking(move || {
            match_clone.run_match_loop()
        }).await;

        match match_result {
            Ok(Ok(stats)) => {
                info!("match completed: {:?}", stats);
                result.match_stats = Some(stats);
                result.success = true;
            }
            Ok(Err(e)) => {
                error!("match loop error: {}", e);
                result.error = Some(format!("match loop: {}", e));
            }
            Err(e) => {
                error!("match task panicked: {}", e);
                result.error = Some(format!("match task: {}", e));
            }
        }

        result
    }

    // ─── Internal Steps ─────────────────────────────────────────────────────

    async fn do_login(&self) -> Result<(String, String)> {
        let mut client = BgmiClient::new(
            &self.account.device.device_id,
            &self.account.device.model,
            &self.account.device.brand,
        )?;

        let cred = self.account.credential.to_auth_credential();
        let guest_id = self.account.guest_id().to_string();

        // Login
        let login_resp = client.login(&cred, &guest_id).await
            .context("HTTPS login failed")?;

        let openid = login_resp.openid.clone();

        // Get ticket
        tokio::time::sleep(Duration::from_millis(800)).await;
        let ticket_resp = client.get_ticket(&guest_id).await
            .context("get ticket failed")?;

        Ok((openid, ticket_resp.ticket))
    }

    async fn wait_for_match(&self, lobby: &LobbyConnection) -> Result<MatchAssignment> {
        let deadline = tokio::time::Instant::now()
            + Duration::from_secs(self.config.matchmaking_timeout_secs);

        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;

            if tokio::time::Instant::now() >= deadline {
                anyhow::bail!("matchmaking timeout ({}s)", self.config.matchmaking_timeout_secs);
            }

            match lobby.state() {
                LobbyState::MatchFound => {
                    if let Some(assignment) = lobby.take_match_assignment() {
                        return Ok(assignment);
                    }
                }
                LobbyState::Disconnected => {
                    anyhow::bail!("lobby disconnected during matchmaking");
                }
                _ => {
                    // Still waiting
                }
            }
        }
    }
}

/// Quick helper to run a single match for an account
pub async fn run_single_match(account: Account, config: BotConfig) -> BotRunResult {
    let orchestrator = BotOrchestrator::new(account, config);
    orchestrator.run().await
}
